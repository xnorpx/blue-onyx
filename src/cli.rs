use crate::LogLevel;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author = "Marcus Asteborg", version=env!("CARGO_PKG_VERSION"), about = "TODO")]
pub struct Cli {
    /// The port on which the server will listen for HTTP requests.
    /// Default is 32168. Example usage: --port 1337
    //#[arg(long, default_value_t = 32168)]
    #[arg(long, default_value_t = 32168)]
    pub port: u16,
    /// Path to the ONNX rt-detrv2 onnx model file.
    /// If not given the default model small model is used.
    #[clap(long)]
    pub model: Option<PathBuf>,
    /// Path to the object classes yaml file
    /// Default: coco_classes.yaml which is the 80 standard COCO classes
    #[clap(long)]
    pub object_classes: Option<PathBuf>,
    /// Filters the results to include only the specified labels. Provide labels separated by ','.
    /// Example: --object_filter "person,cup"
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub object_filter: Vec<String>,
    /// Sets the level of logging
    #[clap(long, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,
    /// If log_path is set, then stdout logging will be disabled and it will log to file
    #[clap(long)]
    pub log_path: Option<PathBuf>,
    /// Confidence threshold for object detection
    #[clap(long, default_value_t = 0.5)]
    pub confidence_threshold: f32,
    /// Force using CPU for inference
    #[clap(long, default_value_t = false)]
    pub force_cpu: bool,
    /// Intra thread parallelism max is cpu cores - 1
    #[clap(long, default_value_t = 192)]
    pub intra_threads: usize,
    /// Inter thread parallelism max is cpu cores - 1
    #[clap(long, default_value_t = 192)]
    pub inter_threads: usize,
    /// Optional path to save the processed images
    #[clap(long)]
    pub save_image_path: Option<PathBuf>,
    /// Save the reference image (only if save_image_path is provided)
    #[clap(long, default_value_t = false)]
    pub save_ref_image: bool,
    /// GPU Index, best effort to select the correct one if multiple GPUs exist.
    /// Default is 0. The list and actual GPU index might differ.
    /// If the wrong GPU is selected, try changing this value.
    /// Verify through GPU usage to ensure the correct GPU is selected.
    #[clap(long, default_value_t = 0)]
    pub gpu_index: i32,
    /// Save inference stats to file
    #[clap(long)]
    pub save_stats_path: Option<PathBuf>,
    /// Path to download all models to
    /// This command will only download the models to the specified path
    /// and then exit
    #[clap(long)]
    pub download_model_path: Option<PathBuf>,
}
