use clap::ValueEnum;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{info, warn, Level};

pub mod api;
pub mod detector;
pub mod download_models;
pub mod image;
pub mod server;
pub mod system_info;
pub mod worker;

pub static BIKE_IMAGE_BYTES: &[u8] = include_bytes!("../assets/crossing.jpg");
pub static DOG_BIKE_CAR_BYTES: &[u8] = include_bytes!("../assets/dog_bike_car.jpg");
pub static SMALL_RT_DETR_V2_MODEL_BYTES: &[u8] = include_bytes!("../models/rt-detrv2-s.onnx");
pub static COCO_CLASSES_STR: &str = include_str!("../assets/coco_classes.yaml");

#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct CocoClasses {
    NAMES: Vec<String>,
}

pub fn get_object_classes(yaml_file: Option<PathBuf>) -> anyhow::Result<Vec<String>> {
    let yaml_data = match yaml_file {
        Some(yaml_file) => std::fs::read_to_string(yaml_file)?,
        None => COCO_CLASSES_STR.to_string(),
    };
    Ok(serde_yaml::from_str::<CocoClasses>(yaml_data.as_str())?.NAMES)
}

pub fn direct_ml_available() -> bool {
    if let Ok(exe_path) = std::env::current_exe() {
        let exe_dir = exe_path.parent().unwrap();
        let direct_ml_path = exe_dir.join("DirectML.dll");
        let direct_ml_available = direct_ml_path.exists();
        if !direct_ml_available {
            warn!("DirectML.dll not found in the same directory as the executable");
        }
        return direct_ml_available;
    }
    warn!("Failed to get current executable path");
    false
}

pub fn init_logging(
    log_level: LogLevel,
    log_path: Option<String>,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    setup_ansi_support();

    if let Some(log_path) = log_path {
        println!(
            "Starting Blue Onyx, logging into: {}/blue_onyx.log",
            log_path
        );
        let file_appender = tracing_appender::rolling::daily(&log_path, "blue_onyx.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        tracing_subscriber::fmt()
            .with_writer(non_blocking)
            .with_max_level(Level::from(log_level))
            .with_ansi(false)
            .init();
        Some(_guard)
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
