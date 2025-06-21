use crate::{
    api::{VisionDetectionRequest, VisionDetectionResponse},
    detector::{Detector, DetectorConfig, DeviceType},
    image::create_random_jpeg_name,
};
use crossbeam::channel::{Receiver, Sender};
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

pub struct DetectorWorker {
    receiver: Receiver<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
        Instant,
    )>,
    detector: Detector,
    request_timeout: Duration,
}

#[allow(clippy::type_complexity)]
impl DetectorWorker {
    pub fn new(
        detector_config: DetectorConfig,
        worker_queue_size: Option<usize>,
    ) -> anyhow::Result<(
        Sender<(
            VisionDetectionRequest,
            oneshot::Sender<VisionDetectionResponse>,
            Instant,
        )>,
        Self,
    )> {
        let request_timeout = detector_config.timeout;
        let mut detector = Detector::new(detector_config)?;

        let worker_queue_size = match worker_queue_size {
            Some(size) => {
                info!(?size, "User set worker queue size");
                size
            }
            None => {
                let min_processing_time = detector.get_min_processing_time()?;
                // Estimate queue size based on timeout and min processing time.
                // If min processing time is 100ms and timeout is 1000ms, we can
                // process 10 images in 1000ms, so queue size should be 10.
                // A queue size of 1000 makes no sense if we can only process 10
                // images in 1000ms. This allows us to drop requests at the
                // service level instead of the worker level.
                let estimated_queue_size =
                    (request_timeout.as_millis() / min_processing_time.as_millis()) as usize;
                info!(
                    ?estimated_queue_size,
                    ?request_timeout,
                    ?min_processing_time,
                    "Estimated worker queue"
                );
                estimated_queue_size
            }
        };

        let (sender, receiver) = crossbeam::channel::bounded(worker_queue_size);

        Ok((
            sender,
            DetectorWorker {
                receiver,
                detector,
                request_timeout,
            },
        ))
    }
    pub fn get_detector(&self) -> &Detector {
        &self.detector
    }
    pub fn run(&mut self) {
        info!("Detector worker thread: Starting detector worker loop");
        while let Ok((vision_request, response_sender, start_request_time)) = self.receiver.recv() {
            let queue_time = start_request_time.elapsed();
            debug!(
                ?queue_time,
                "Received request from worker time spent in queue"
            );
            let VisionDetectionRequest {
                image_data,
                image_name,
                min_confidence,
                ..
            } = vision_request;

            let image_name = if image_name == "image.jpg" {
                Some(create_random_jpeg_name())
            } else {
                Some(image_name)
            };

            let min_confidence = (min_confidence > 0.01).then_some(min_confidence);

            let detect_result = self.detector.detect(image_data, image_name, min_confidence);

            let detect_response = match detect_result {
                Ok(detect_result) => VisionDetectionResponse {
                    success: true,
                    message: "".into(),
                    error: None,
                    predictions: detect_result.predictions.to_vec(),
                    count: detect_result.predictions.len() as i32,
                    command: "detect".into(),
                    moduleId: self.detector.get_model_name().clone(),
                    executionProvider: detect_result.endpoint_provider.to_string(),
                    canUseGPU: detect_result.device_type == DeviceType::GPU,
                    inferenceMs: detect_result.inference_time.as_millis() as i32,
                    processMs: detect_result.processing_time.as_millis() as i32,
                    analysisRoundTripMs: 0_i32,
                },
                Err(err) => VisionDetectionResponse {
                    success: false,
                    message: "Failboat".into(),
                    error: Some(err.to_string()),
                    predictions: vec![],
                    count: 0,
                    command: "detect".into(),
                    moduleId: self.detector.get_model_name().clone(),
                    executionProvider: "CPU".into(),
                    canUseGPU: false,
                    inferenceMs: 0_i32,
                    processMs: 0_i32,
                    analysisRoundTripMs: 0_i32,
                },
            };

            let request_time = start_request_time.elapsed();
            if request_time > self.request_timeout {
                warn!(?detect_response, ?request_time, ?self.request_timeout, "Request timed out, this means that the server is overloaded and we will drop this response.");
                warn!("If you see this message spamming you should reduce the number of requests or upgrade your service to be faster.");
            }

            if let Err(err) = response_sender.send(detect_response) {
                warn!(?err, ?request_time, ?self.request_timeout, "Failed to send response from worker, the client request has most likely timed out so receiver is gone.");
                warn!("If you see this message spamming you should reduce the number of requests or upgrade your service to be faster.");
            }
        }
        info!("Detector worker thread: Completed and exiting");
    }

    /// Spawns the detector worker thread with optimized settings
    pub fn spawn_worker_thread(mut self) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            #[cfg(windows)]
            unsafe {
                use windows::Win32::System::Threading::{
                    GetCurrentProcessorNumber, GetCurrentThread, SetThreadAffinityMask,
                    SetThreadPriority, THREAD_PRIORITY_TIME_CRITICAL,
                };
                let thread_handle = GetCurrentThread();
                if let Err(err) = SetThreadPriority(thread_handle, THREAD_PRIORITY_TIME_CRITICAL) {
                    tracing::error!(?err, "Failed to set thread priority to time critical");
                }
                let processor_number = GetCurrentProcessorNumber();
                let core_mask = 1usize << processor_number;
                let previous_mask = SetThreadAffinityMask(thread_handle, core_mask);
                if previous_mask == 0 {
                    tracing::error!("Failed to set thread affinity.");
                }
            }
            self.run();
        })
    }
}
