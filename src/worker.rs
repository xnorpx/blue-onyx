use tokio::sync::oneshot;
use tracing::error;
use crate::{
    api::{VisionDetectionRequest, VisionDetectionResponse},
    detector::{Detector, DetectorConfig, DeviceType},
    image::create_random_jpeg_name,
};
use std::sync::mpsc::Receiver;

pub struct DetectorWorker {
    receiver: Receiver<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
    )>,
    detector: Detector,
}

impl DetectorWorker {
    pub fn new(
        detector_config: DetectorConfig,
        receiver: Receiver<(
            VisionDetectionRequest,
            oneshot::Sender<VisionDetectionResponse>,
        )>,
    ) -> anyhow::Result<Self> {
        Ok(DetectorWorker {
            receiver,
            detector: Detector::new(detector_config)?,
        })
    }

    pub fn run(&mut self) {
        while let Ok((vision_request, response_sender)) = self.receiver.recv() {
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

            let detect_result = self.detector.detect(image_data, image_name, Some(min_confidence));

            let detect_response = match detect_result {
                Ok(detect_result) => VisionDetectionResponse {
                    success: true,
                    message: "".into(),
                    error: None,
                    predictions: detect_result.predictions.to_vec(),
                    count: detect_result.predictions.len() as i32,
                    command: "detect".into(),
                    module_id: "rt-detrv2".into(),
                    execution_provider: detect_result.endpoint_provider.to_string(),
                    can_useGPU: detect_result.device_type == DeviceType::GPU,
                    inference_ms: detect_result.inference_time.as_millis() as i32,
                    process_ms: detect_result.processing_time.as_millis() as i32,
                    analysis_round_trip_ms: 0_i32,
                },
                Err(err) => VisionDetectionResponse {
                    success: false,
                    message: "Failboat".into(),
                    error: Some(err.to_string()),
                    predictions: vec![],
                    count: 0,
                    command: "detect".into(),
                    module_id: "rt-detrv2".into(),
                    execution_provider: "CPU".into(),
                    can_useGPU: false,
                    inference_ms: 0_i32,
                    process_ms: 0_i32,
                    analysis_round_trip_ms: 0_i32,
                },
            };

            if let Err(err) = response_sender.send(detect_response) {
                error!(?err, "Failed to send response from worker"); 
            }
        }
    }
}