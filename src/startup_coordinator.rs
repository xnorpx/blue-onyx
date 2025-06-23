use crate::{
    api::VisionDetectionRequest, api::VisionDetectionResponse, detector::DetectorConfig,
    detector::ExecutionProvider, worker::DetectorWorker,
};
use crossbeam::channel::Sender;
use std::time::Instant;
use tokio::sync::oneshot;
use tracing::{error, info};

/// Information about the initialized detector
#[derive(Debug, Clone)]
pub struct DetectorInfo {
    pub model_name: String,
    pub execution_provider: ExecutionProvider,
}

/// Result of detector initialization
pub enum InitResult {
    Success {
        sender: Sender<(
            VisionDetectionRequest,
            oneshot::Sender<VisionDetectionResponse>,
            Instant,
        )>,
        detector_info: DetectorInfo,
        worker_thread_handle: std::thread::JoinHandle<()>,
    },
    Failed(String),
}

/// Creates a startup worker thread that initializes the detector and returns
/// a receiver that will get the sender once initialization is complete
pub fn spawn_detector_initialization(
    detector_config: DetectorConfig,
    worker_queue_size: Option<usize>,
) -> tokio::sync::oneshot::Receiver<InitResult> {
    let (init_sender, init_receiver) = tokio::sync::oneshot::channel();

    // Spawn background thread to initialize detector
    std::thread::spawn(move || {
        startup_worker_thread(init_sender, detector_config, worker_queue_size);
    });

    init_receiver
}

/// The background worker thread that initializes the detector
/// This function runs in a separate thread and sends the result via the channel
fn startup_worker_thread(
    init_sender: tokio::sync::oneshot::Sender<InitResult>,
    detector_config: DetectorConfig,
    worker_queue_size: Option<usize>,
) {
    info!("Startup worker thread: Beginning detector initialization...");

    // Initialize the detector worker in this background thread
    let init_result = DetectorWorker::new(detector_config.clone(), worker_queue_size);

    match init_result {
        Ok((sender, detector_worker)) => {
            // Get detector information before transferring ownership
            let detector = detector_worker.get_detector();

            let execution_provider = if detector_config.object_detection_onnx_config.force_cpu
                || !detector.is_using_gpu()
            {
                ExecutionProvider::CPU
            } else {
                ExecutionProvider::DirectML(
                    detector_config.object_detection_onnx_config.gpu_index as usize,
                )
            };

            let detector_info = DetectorInfo {
                model_name: detector.get_model_name().clone(),
                execution_provider: execution_provider.clone(),
            };
            info!(
                model_name = %detector_info.model_name,
                execution_provider = ?detector_info.execution_provider,
                "Startup worker thread: Detector initialization complete, starting worker thread"
            ); // Start the detector worker in a separate thread (this will continue running)
            let worker_thread_handle = detector_worker.spawn_worker_thread();

            // Hand over the sender to the server
            let result = InitResult::Success {
                sender,
                detector_info,
                worker_thread_handle,
            };
            if init_sender.send(result).is_err() {
                error!("Startup worker thread: Failed to send initialization result to server");
            } else {
                info!(
                    "Startup worker thread: Handover complete, detector is now available to server"
                );
            }
            info!(
                "Startup worker thread: Completed successfully (worker thread continues in background)"
            );

            // The startup thread exits here, but the worker thread continues running
            // The server now owns the sender and can communicate with the detector
        }
        Err(e) => {
            error!(error = %e, "Startup worker thread: Detector initialization failed");
            let result = InitResult::Failed(e.to_string());
            if init_sender.send(result).is_err() {
                error!("Startup worker thread: Failed to send failure result to server");
            }
            info!("Startup worker thread: Completed due to initialization failure");
        }
    }
}
