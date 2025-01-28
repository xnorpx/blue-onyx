use clap::ValueEnum;
use cli::Cli;
use detector::{ObjectDetectionModel, OnnxConfig};
use download_models::{download_model, Model};
use serde::Deserialize;
use server::run_server;
use std::{future::Future, path::PathBuf};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, Level};
pub mod api;
pub mod cli;
pub mod detector;
pub mod download_models;
pub mod image;
pub mod server;
pub mod system_info;
pub mod worker;

pub static DOG_BIKE_CAR_BYTES: &[u8] = include_bytes!("../assets/dog_bike_car.jpg");
pub static DEFAULT_MODEL: &str = "rt-detrv2-s.onnx";

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct CocoClasses {
    NAMES: Vec<String>,
}

pub fn check_model_available(
    model: Option<PathBuf>,
    object_classes: Option<PathBuf>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    // If model is None then use the default model
    let model = if let Some(model) = model {
        model.canonicalize()?
    } else {
        std::env::current_exe()
            .map_err(|e| anyhow::anyhow!(e))
            .and_then(|exe_path| {
                exe_path
                    .parent()
                    .map(|p| p.join(DEFAULT_MODEL))
                    .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory"))
            })?
    };

    debug!(?model, "Model path");

    // If object_classes is None then use the models classes
    let object_classes = if let Some(object_classes) = object_classes {
        object_classes.canonicalize()?
    } else {
        model.with_extension("yaml")
    };

    // Check if model is available and download it if it is not
    if !model.exists() || !object_classes.exists() {
        debug!(
            ?model,
            "Model or object classes does not exist trying to download it"
        );
        let model_path = model
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to get model directory"))?;
        let model_file = model
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Failed to get model file name"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert model file name to string"))?;
        let model_file = Model::Model(model_file.to_string());
        download_model(model_path.to_path_buf(), model_file)?;
    }

    Ok((model, object_classes))
}

pub fn blue_onyx_service(
    args: Cli,
) -> anyhow::Result<(
    impl Future<Output = anyhow::Result<()>>,
    CancellationToken,
    std::thread::JoinHandle<()>,
)> {
    let (model, object_classes) = check_model_available(args.model, args.object_classes)?;
    let object_detection_model = match args.object_detection_model_type {
        Some(model_type) => model_type,
        None => {
            ObjectDetectionModel::try_from(model.file_name().unwrap().to_str().unwrap()).unwrap()
        }
    };

    let detector_config = detector::DetectorConfig {
        object_detection_onnx_config: OnnxConfig {
            force_cpu: args.force_cpu,
            gpu_index: args.gpu_index,
            intra_threads: args.intra_threads,
            inter_threads: args.inter_threads,
            model,
        },
        object_classes,
        object_filter: args.object_filter,
        confidence_threshold: args.confidence_threshold,
        save_image_path: args.save_image_path,
        save_ref_image: args.save_ref_image,
        timeout: args.request_timeout,
        object_detection_model,
    };

    // Run a separate thread for the detector worker
    let (sender, mut detector_worker) =
        worker::DetectorWorker::new(detector_config, args.worker_queue_size)?;

    let detector = detector_worker.get_detector();
    let model_name = detector.get_model_name();
    let using_gpu = detector.is_using_gpu();
    let execution_providers_name = detector.get_endpoint_provider_name();

    let device_name = if using_gpu {
        system_info::gpu_model(args.gpu_index as usize)
    } else {
        system_info::cpu_model()
    };
    let metrics = server::Metrics::new(
        model_name.clone(),
        device_name,
        execution_providers_name,
        args.log_path,
    );
    let cancel_token = CancellationToken::new();
    let server_future = run_server(args.port, cancel_token.clone(), sender, metrics);

    let thread_handle = std::thread::spawn(move || {
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
        detector_worker.run();
    });

    Ok((server_future, cancel_token, thread_handle))
}

pub fn get_object_classes(yaml_file: PathBuf) -> anyhow::Result<Vec<String>> {
    let yaml_data = std::fs::read_to_string(yaml_file)?;
    Ok(serde_yaml::from_str::<CocoClasses>(yaml_data.as_str())?.NAMES)
}

pub fn direct_ml_available() -> bool {
    #[cfg(not(windows))]
    {
        false
    }
    #[cfg(windows)]
    {
        let Ok(exe_path) = std::env::current_exe() else {
            return false;
        };
        let Some(exe_dir) = exe_path.parent() else {
            return false;
        };
        exe_dir.join("DirectML.dll").exists()
    }
}

pub fn init_logging(
    log_level: LogLevel,
    log_path: &mut Option<PathBuf>,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    setup_ansi_support();

    let guard = log_path.clone().map(|path| {
        let log_directory = if path.starts_with(".") {
            let stripped = path.strip_prefix(".").unwrap_or(&path).to_path_buf();
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.join(stripped.clone())))
                .unwrap_or(stripped)
        } else {
            path
        };

        *log_path = Some(log_directory.clone());

        let log_file = log_directory.join("blue_onyx.log");
        println!("Starting Blue Onyx, logging into: {}", log_file.display());

        let file_appender = tracing_appender::rolling::daily(&log_directory, "blue_onyx.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        tracing_subscriber::fmt()
            .with_writer(non_blocking)
            .with_max_level(Level::from(log_level))
            .with_ansi(false)
            .init();

        guard
    });

    if guard.is_some() {
        guard
    } else {
        tracing_subscriber::fmt()
            .with_max_level(Level::from(log_level))
            .init();
        info!(?log_level, "Logging initialized");
        None
    }
}

fn setup_ansi_support() {
    #[cfg(target_os = "windows")]
    if let Err(e) = ansi_term::enable_ansi_support() {
        eprintln!("Failed to enable ANSI support: {}", e);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for Level {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Trace => Level::TRACE,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}
