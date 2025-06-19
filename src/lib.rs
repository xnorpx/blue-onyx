use clap::ValueEnum;
use cli::Cli;
use detector::OnnxConfig;
use serde::Deserialize;
use server::run_server;
use std::{future::Future, path::PathBuf};
use tokio_util::sync::CancellationToken;
use tracing::{info, Level};
pub mod api;
pub mod cli;
pub mod detector;
pub mod download_models;
pub mod image;
pub mod server;
pub mod system_info;
pub mod worker;

pub static DOG_BIKE_CAR_BYTES: &[u8] = include_bytes!("../assets/dog_bike_car.jpg");
pub static SMALL_RT_DETR_V2_MODEL_FILE_NAME: &str = "rt-detrv2-s.onnx";
pub static COCO_CLASSES_STR: &str = include_str!("../assets/coco_classes.yaml");

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct CocoClasses {
    NAMES: Vec<String>,
}

pub fn blue_onyx_service(
    args: Cli,
) -> anyhow::Result<(
    impl Future<Output = anyhow::Result<bool>>, // Return bool for restart indication
    CancellationToken,
    CancellationToken, // Add restart token
    std::thread::JoinHandle<()>,
)> {
    let detector_config = detector::DetectorConfig {
        object_detection_onnx_config: OnnxConfig {
            force_cpu: args.force_cpu,
            gpu_index: args.gpu_index,
            intra_threads: args.intra_threads,
            inter_threads: args.inter_threads,
            model: args.model,
        },
        object_classes: args.object_classes,
        object_filter: args.object_filter,
        confidence_threshold: args.confidence_threshold,
        save_image_path: args.save_image_path,
        save_ref_image: args.save_ref_image,
        timeout: args.request_timeout,
        object_detection_model: args.object_detection_model_type,
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
    let restart_token = CancellationToken::new();
    let server_future = run_server(
        args.port,
        cancel_token.clone(),
        restart_token.clone(),
        sender,
        metrics,
    );

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

    Ok((server_future, cancel_token, restart_token, thread_handle))
}

pub fn get_object_classes(yaml_file: Option<PathBuf>) -> anyhow::Result<Vec<String>> {
    let yaml_data = match yaml_file {
        Some(yaml_file) => std::fs::read_to_string(yaml_file)?,
        None => COCO_CLASSES_STR.to_string(),
    };
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

#[cfg(target_os = "windows")]
pub fn init_service_logging(log_level: LogLevel, _log_path: &mut Option<PathBuf>) {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    // Create Windows Event Log layer only - no file or stdout logging
    let eventlog_layer = tracing_layer_win_eventlog::EventLogLayer::new("Blue Onyx Service")
        .unwrap_or_else(|e| {
            eprintln!("Failed to create Windows Event Log layer: {}", e);
            panic!("Could not initialize Windows Event Log");
        });

    // Initialize with Event Log only
    tracing_subscriber::registry()
        .with(eventlog_layer)
        .with(tracing_subscriber::filter::LevelFilter::from(Level::from(
            log_level,
        )))
        .init();
    info!(
        ?log_level,
        "Service logging initialized with Windows Event Log only"
    );
}

fn setup_ansi_support() {
    #[cfg(target_os = "windows")]
    if let Err(e) = ansi_term::enable_ansi_support() {
        eprintln!("Failed to enable ANSI support: {}", e);
    }
}

#[derive(
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    ValueEnum,
    Debug,
    serde::Serialize,
    serde::Deserialize,
)]
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

/// Ensures model and yaml files exist, downloading them if needed
/// Returns the paths to the model and yaml files
pub fn ensure_model_files(model_name: Option<String>) -> anyhow::Result<(PathBuf, PathBuf)> {
    // Use default model if none provided
    let model_filename = model_name.unwrap_or_else(|| SMALL_RT_DETR_V2_MODEL_FILE_NAME.to_string());

    // Get the directory where models are stored (next to the executable)
    let exe_path = std::env::current_exe()?;
    let models_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get parent directory of executable"))?;

    let model_path = models_dir.join(&model_filename);
    let yaml_filename = model_filename.replace(".onnx", ".yaml");
    let yaml_path = models_dir.join(&yaml_filename); // Check if model exists, download if not
    if !model_path.exists() {
        info!("Model {} not found, downloading...", model_filename);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            download_models::download_file_to_dir(&model_filename, models_dir).await
        })?;
    }

    // Verify model file exists after download
    if !model_path.exists() {
        return Err(anyhow::anyhow!(
            "Model file {} is required but could not be found or downloaded",
            model_filename
        ));
    }

    // Check if yaml exists, download if not (MANDATORY)
    if !yaml_path.exists() {
        info!("Yaml file {} not found, downloading...", yaml_filename);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        rt.block_on(async {
            download_models::download_file_to_dir(&yaml_filename, models_dir).await
        })?;
    }

    // Verify yaml file exists after download
    if !yaml_path.exists() {
        return Err(anyhow::anyhow!(
            "YAML file {} is required but could not be found or downloaded",
            yaml_filename
        ));
    }

    info!(
        "Model and YAML files ready: {} and {}",
        model_path.display(),
        yaml_path.display()
    );
    Ok((model_path, yaml_path))
}
