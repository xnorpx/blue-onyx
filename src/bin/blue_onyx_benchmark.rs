//! Blue Onyx Benchmark Application
//!
//! This application benchmarks the inference performance of the rt-detrv2 model across
//! different device configurations. It records statistics such as total inference time,
//! average, minimum, and maximum inference durations, as well as images processed per second.
//! The results is logged and can be saved to a file.
//!
//! The default model is the small rt-detrv2 model, and the default object classes are the 80
//! standard COCO classes. The application can also filter the results to include only the specified
//! labels. The confidence threshold for object detection can be set, and the application can be
//! forced to use the CPU for inference. The application can also be configured to save the processed
//! image and the reference image, repeat the image processing.
//!
//! The application can be run with the following command:
//!
//! Downloaded binary:
//! ```sh
//! blue_onyx_benchmark --help
//! ```
//!
//! From repository:
//! ```sh
//! cargo run --bin blue_onyx_benchmark -- --help
//! ```
//!
use anyhow::bail;
use blue_onyx::{
    LogLevel,
    detector::{
        Detector, DetectorConfig, DeviceType, EndpointProvider, ObjectDetectionModel, OnnxConfig,
    },
    image::load_image,
    init_logging,
    system_info::{cpu_model, gpu_model, system_info},
};
use bytes::Bytes;
use clap::Parser;
use std::{io::Write, path::PathBuf, time::Duration};
use tracing::{error, info};

#[derive(Parser)]
#[command(author = "Marcus Asteborg", version=env!("CARGO_PKG_VERSION"), about = "
Blue Onyx Benchmark Application

This application benchmarks the inference performance of the rt-detrv2 model across
different device configurations. It records statistics such as total inference time,
average, minimum, and maximum inference durations, as well as images processed per second.
The results is logged and can be saved to a file.

The default model is the small rt-detrv2 model, and the default object classes are the 80
standard COCO classes. The application can also filter the results to include only the specified
labels. The confidence threshold for object detection can be set, and the application can be
forced to use the CPU for inference. The application can also be configured to save the processed
image and the reference image, repeat the image processing. ")]
struct Cli {
    /// Path to the image file
    /// If not given default test image is used
    #[clap(long)]
    image: Option<PathBuf>,
    /// Path to the ONNX model file.
    /// If not specified, the default rt-detrv2 small model will be used
    /// provided it is available in the directory.
    #[clap(long)]
    pub model: Option<PathBuf>,
    /// Type of model type to use.
    /// Default: rt-detrv2
    #[clap(long, default_value_t = ObjectDetectionModel::RtDetrv2)]
    pub object_detection_model_type: ObjectDetectionModel,
    /// Path to the object classes yaml file
    /// Default: coco_classes.yaml which is the 80 standard COCO classes
    #[clap(long)]
    object_classes: Option<PathBuf>,
    /// Filters the results to include only the specified labels. Provide labels separated by ','.
    /// Example: --object_filter "person,cup"
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub object_filter: Vec<String>,
    /// Sets the level of logging
    #[clap(long, value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
    /// Confidence threshold for object detection
    #[clap(long, default_value_t = 0.5)]
    confidence_threshold: f32,
    /// Force using CPU for inference
    #[clap(long, default_value_t = false)]
    force_cpu: bool,
    /// Intra thread parallelism max is cpu cores - 1
    #[clap(long, default_value_t = 192)]
    intra_threads: usize,
    /// Inter thread parallelism max is cpu cores - 1
    #[clap(long, default_value_t = 192)]
    inter_threads: usize,
    /// Optional path to save the processed image
    #[clap(long)]
    save_image_path: Option<PathBuf>,
    /// Save the reference image (only if save_image_path is provided)
    #[clap(long, default_value_t = false)]
    save_ref_image: bool,
    /// Repeat the image processing
    #[clap(long, default_value_t = 1)]
    repeat: u32,
    /// GPU
    #[clap(long, default_value_t = 0)]
    gpu_index: i32,
    /// Save inference stats to file
    #[clap(long)]
    save_stats_path: Option<PathBuf>,
    /// Path to download all models to
    /// This command will only download the models to the specified path
    /// and then exit
    #[clap(long)]
    download_model_path: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();
    let _guard = init_logging(args.log_level, &mut None);
    system_info()?;

    if args.download_model_path.is_some() {
        blue_onyx::download_models::download_models(args.download_model_path.unwrap(), false)?;
        return Ok(());
    }

    let detector_config = DetectorConfig {
        object_detection_onnx_config: OnnxConfig {
            model: args.model,
            force_cpu: args.force_cpu,
            gpu_index: args.gpu_index,
            intra_threads: args.intra_threads,
            inter_threads: args.inter_threads,
        },
        object_classes: args.object_classes,
        object_filter: args.object_filter,
        confidence_threshold: args.confidence_threshold,
        save_image_path: args.save_image_path,
        save_ref_image: args.save_ref_image,
        timeout: Duration::MAX,
        object_detection_model: args.object_detection_model_type,
    };

    let mut detector = Detector::new(detector_config)?;

    let (image_bytes, image_name) = if let Some(image) = args.image {
        (load_image(&image)?, image.to_string_lossy().to_string())
    } else {
        (
            Bytes::from(blue_onyx::DOG_BIKE_CAR_BYTES),
            "dog_bike_car.jpg".to_string(),
        )
    };

    let mut inference_times: Vec<Duration> = Vec::with_capacity(args.repeat as usize);

    info!(
        "Starting inference benchmark with {} repetitions",
        args.repeat
    );
    let start_time = std::time::Instant::now();
    let mut predictions = detector.detect(image_bytes.clone(), Some(image_name.clone()), None)?;
    if predictions.predictions.is_empty() {
        error!(?predictions, "No objects detected");
        bail!("No objects detected");
    }
    inference_times.push(predictions.inference_time);

    for _ in 1..args.repeat {
        predictions = detector.detect(image_bytes.clone(), Some(image_name.clone()), None)?;
        inference_times.push(predictions.inference_time);
    }
    let elapsed = start_time.elapsed();
    info!("All done predictions: {:#?} in {:?}", predictions, elapsed);

    let device_name = match predictions.device_type {
        DeviceType::CPU => cpu_model(),
        DeviceType::GPU => gpu_model(args.gpu_index as usize),
    };

    let inference_stats = InferenceStats::new(
        detector.get_model_name().clone(),
        device_name,
        predictions.device_type,
        predictions.endpoint_provider,
        inference_times,
    );

    inference_stats.print_table();
    inference_stats.save_to_file(args.save_stats_path)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub enum Platform {
    Linux,
    Windows,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Linux => write!(f, "Linux  "),
            Platform::Windows => write!(f, "Windows"),
        }
    }
}
impl Default for Platform {
    fn default() -> Self {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else {
            panic!("Unsupported platform");
        }
    }
}

#[derive(Debug, Clone)]
pub struct InferenceStats {
    pub model_name: String,
    pub version: String,
    pub device_name: String,
    pub device_type: DeviceType,
    pub platform: Platform,
    pub endpoint_provider: EndpointProvider,
    pub number_of_images: u64,
    pub total_inference: Duration,
    pub images_per_second: f64,
    pub min_inference: Duration,
    pub max_inference: Duration,
    pub average_inference: Duration,
}

impl InferenceStats {
    pub fn new(
        model_name: String,
        device_name: String,
        device_type: DeviceType,
        endpoint_provider: EndpointProvider,
        inference_times: Vec<Duration>,
    ) -> Self {
        let number_of_images = inference_times.len() as u64;
        let total_inference: Duration = inference_times.iter().sum();
        let average_inference =
            Duration::from_secs_f64(total_inference.as_secs_f64() / number_of_images as f64);
        let min_inference = *inference_times.iter().min().unwrap_or(&Duration::ZERO);
        let max_inference = *inference_times.iter().max().unwrap_or(&Duration::ZERO);
        let total_inference_secs = total_inference.as_secs_f64();
        let images_per_second = if total_inference_secs > 1. {
            number_of_images as f64 / total_inference_secs
        } else {
            0. // Not enough time to calculate images per second moar images please!
        };
        Self {
            model_name: model_name.replace(".onnx", ""),
            version: env!("CARGO_PKG_VERSION").to_string(),
            device_name,
            device_type,
            platform: Platform::default(),
            endpoint_provider,
            average_inference,
            min_inference,
            max_inference,
            total_inference,
            images_per_second,
            number_of_images,
        }
    }

    pub fn print_table(&self) {
        info!("Inference stats for {}", self.device_name);
        info!("{}", InferenceStats::format_stats_header());
        info!("{}", self.format_stats());
    }

    pub fn format_stats_header() -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            "Model Name",
            "Device Name",
            "Version",
            "Type",
            "Platform",
            "EndpointProvider",
            "Images",
            "Total [s]",
            "Min [ms]",
            "Max [ms]",
            "Average [ms]",
            "FPS"
        )
    }

    pub fn format_stats(&self) -> String {
        let total_inference_secs = format!("{:.1}", self.total_inference.as_secs_f64());
        let min_inference_ms = format!("{:.1}", self.min_inference.as_micros() as f64 / 1000.0);
        let max_inference_ms = format!("{:.1}", self.max_inference.as_micros() as f64 / 1000.0);
        let average_inference_ms =
            format!("{:.1}", self.average_inference.as_micros() as f64 / 1000.0);
        let images_per_second = format!("{:.1}", self.images_per_second);

        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            self.model_name,
            self.device_name,
            self.version,
            self.device_type,
            self.platform,
            self.endpoint_provider,
            self.number_of_images,
            total_inference_secs,
            min_inference_ms,
            max_inference_ms,
            average_inference_ms,
            images_per_second
        )
    }

    pub fn save_to_file(&self, path: Option<PathBuf>) -> std::io::Result<()> {
        let Some(path) = path else {
            return Ok(());
        };
        let sanitized_device_name: String = self
            .device_name
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let sanitized_model_name = self.model_name.replace(" ", "_").replace(".onnx", "");
        let file_name = format!(
            "blue_onyx_{}_{}_report.txt",
            sanitized_device_name, sanitized_model_name
        );
        let path = path.join(file_name);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path.clone())?;

        writeln!(file, "{}", InferenceStats::format_stats_header())?;
        write!(file, "{}", self.format_stats())?;
        info!(?path, "Inference stats saved");
        Ok(())
    }
}
