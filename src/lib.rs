use clap::ValueEnum;
use cli::Cli;
use detector::OnnxConfig;
use serde::Deserialize;
use server::run_server;
use startup_coordinator::spawn_detector_initialization;
use std::{future::Future, path::PathBuf};
use tokio_util::sync::CancellationToken;
use tracing::{Level, info};
pub mod api;
pub mod cli;
pub mod detector;
pub mod download_models;
pub mod image;
pub mod server;
pub mod startup_coordinator;
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

/// Type alias for the service result containing restart flag and optional worker thread handle
pub type ServiceResult = anyhow::Result<(bool, Option<std::thread::JoinHandle<()>>)>;

pub fn blue_onyx_service(
    args: Cli,
) -> anyhow::Result<(
    impl Future<Output = ServiceResult>,
    CancellationToken,
    CancellationToken, // Add restart token
)> {
    // Get the config path for the server
    let config_path = args.get_current_config_path()?;

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

    // Log available GPU information
    log_available_gpus();

    // Start the detector initialization in the background
    let detector_init_receiver =
        spawn_detector_initialization(detector_config, args.worker_queue_size); // Create placeholder metrics (will be updated when detector is ready)
    let metrics = server::Metrics::new(
        "Initializing...".to_string(),
        "Initializing...".to_string(),
        args.log_path,
    );

    let cancel_token = CancellationToken::new();
    let restart_token = CancellationToken::new();
    let server_future = run_server(
        args.port,
        cancel_token.clone(),
        restart_token.clone(),
        detector_init_receiver,
        metrics,
        config_path,
    );

    Ok((server_future, cancel_token, restart_token))
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

/// Log information about available GPU devices
pub fn log_available_gpus() {
    #[cfg(windows)]
    if direct_ml_available() {
        info!("DirectML is available for GPU inference");
    } else {
        info!("DirectML is not available - only CPU inference will be supported");
    }

    #[cfg(not(windows))]
    info!("GPU acceleration not available on this platform - only CPU inference will be supported");

    // Log available GPU devices
    match system_info::gpu_info(true) {
        Ok(_) => {
            // gpu_info already logs the available GPUs when log_info is true
        }
        Err(e) => {
            tracing::warn!("Failed to enumerate GPU devices: {}", e);
        }
    }
}

use std::sync::OnceLock;

// Global reload handle for regular blue_onyx binary
static REGULAR_LOG_RELOAD_HANDLE: OnceLock<
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>,
> = OnceLock::new();

#[cfg(target_os = "windows")]
// Global reload handle for service binary
static SERVICE_LOG_RELOAD_HANDLE: OnceLock<
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>,
> = OnceLock::new();

pub fn init_logging(
    log_level: LogLevel,
    log_path: &mut Option<PathBuf>,
) -> anyhow::Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, reload};

    setup_ansi_support();

    // Create a reloadable env filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level_to_filter_string(log_level)));

    let (env_filter, reload_handle) = reload::Layer::new(env_filter);

    // Store the reload handle globally for later use
    REGULAR_LOG_RELOAD_HANDLE
        .set(reload_handle)
        .map_err(|_| anyhow::anyhow!("Failed to set log reload handle"))?;

    let guard = if let Some(path) = log_path.clone() {
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

        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(file_layer)
            .try_init()
            .map_err(|_| anyhow::anyhow!("Logging already initialized"))?;

        Some(guard)
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .try_init()
            .map_err(|_| anyhow::anyhow!("Logging already initialized"))?;

        None
    };

    info!(
        ?log_level,
        "Logging initialized with dynamic filtering support"
    );
    Ok(guard)
}

pub fn update_log_level(new_log_level: LogLevel) -> anyhow::Result<()> {
    use tracing_subscriber::EnvFilter;

    if let Some(reload_handle) = REGULAR_LOG_RELOAD_HANDLE.get() {
        let new_filter = EnvFilter::new(level_to_filter_string(new_log_level));
        reload_handle
            .reload(new_filter)
            .map_err(|e| anyhow::anyhow!("Failed to reload log filter: {}", e))?;

        info!(?new_log_level, "Log level updated dynamically");
        Ok(())
    } else {
        Err(anyhow::anyhow!("Log reload handle not available"))
    }
}

#[cfg(target_os = "windows")]
pub fn init_service_logging(log_level: LogLevel) -> anyhow::Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::{EnvFilter, reload};

    // Create a reloadable env filter
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level_to_filter_string(log_level)));

    let (env_filter, reload_handle) = reload::Layer::new(env_filter);

    // Store the reload handle globally for later use
    SERVICE_LOG_RELOAD_HANDLE
        .set(reload_handle)
        .map_err(|_| anyhow::anyhow!("Failed to set log reload handle"))?;

    // Create Windows Event Log layer only - no file or stdout logging
    let eventlog_layer = tracing_layer_win_eventlog::EventLogLayer::new("Blue Onyx Service")
        .map_err(|e| anyhow::anyhow!("Failed to create Windows Event Log layer: {}", e))?;

    // Try to initialize with Event Log and reloadable filter
    tracing_subscriber::registry()
        .with(env_filter)
        .with(eventlog_layer)
        .try_init()
        .map_err(|_| anyhow::anyhow!("Logging already initialized"))?;

    info!(
        ?log_level,
        "Service logging initialized with Windows Event Log and dynamic filtering"
    );
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn update_service_log_level(new_log_level: LogLevel) -> anyhow::Result<()> {
    use tracing_subscriber::EnvFilter;

    if let Some(reload_handle) = SERVICE_LOG_RELOAD_HANDLE.get() {
        let new_filter = EnvFilter::new(level_to_filter_string(new_log_level));
        reload_handle
            .reload(new_filter)
            .map_err(|e| anyhow::anyhow!("Failed to reload log filter: {}", e))?;

        info!(?new_log_level, "Log level updated dynamically");
        Ok(())
    } else {
        Err(anyhow::anyhow!("Log reload handle not available"))
    }
}

fn level_to_filter_string(log_level: LogLevel) -> String {
    match log_level {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
    .to_string()
}

fn setup_ansi_support() {
    #[cfg(target_os = "windows")]
    if let Err(e) = ansi_term::enable_ansi_support() {
        eprintln!("Failed to enable ANSI support: {e}");
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
