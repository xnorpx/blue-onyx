use crate::{detector::ObjectDetectionModel, LogLevel};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Parser, Serialize, Deserialize, Clone)]
#[command(author = "Marcus Asteborg", version=env!("CARGO_PKG_VERSION"), about = "TODO")]
#[serde(default)]
pub struct Cli {
    /// Path to configuration file (JSON format)
    #[arg(long)]
    #[serde(skip)]
    pub config: Option<PathBuf>,
    /// The port on which the server will listen for HTTP requests.
    /// Default is 32168. Example usage: --port 1337
    //#[arg(long, default_value_t = 32168)]
    #[arg(long, default_value_t = 32168)]
    pub port: u16,
    /// Duration to wait for a response from the detection worker.
    /// Ideally, this should be similar to the client's timeout setting.
    #[arg(long, default_value = "15", value_parser = parse_duration)]
    #[serde(with = "duration_serde")]
    pub request_timeout: Duration,
    /// Worker queue size.
    /// The number of requests that can be queued before the server starts rejecting them.
    /// If not set, the server will estimate the queue size based on the timeout and the
    /// inference performance.
    /// This estimation is based on the timeout and the expected number of requests per second.
    #[arg(long)]
    pub worker_queue_size: Option<usize>,
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
    /// Intra thread parallelism max is CPU cores - 1.
    /// On Windows, you can use high thread counts, but if you use too high
    /// thread count on Linux, you will get a BIG performance hit.
    /// So default is 1, then you can increase it if you want to test the
    /// performance.
    #[cfg(target_os = "windows")]
    #[clap(long, default_value_t = 192)]
    pub intra_threads: usize,
    #[cfg(not(target_os = "windows"))]
    #[clap(long, default_value_t = 2)]
    pub intra_threads: usize,
    /// Inter thread parallelism max is CPU cores - 1.
    /// On Windows, you can use high thread counts, but if you use too high
    /// thread count on Linux, you will get a BIG performance hit.
    /// So default is 2, then you can increase it if you want to test the
    /// performance.
    #[cfg(target_os = "windows")]
    #[clap(long, default_value_t = 192)]
    pub inter_threads: usize,
    #[cfg(not(target_os = "windows"))]
    #[clap(long, default_value_t = 2)]
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
    /// Save inference stats to file    #[clap(long)]
    pub save_stats_path: Option<PathBuf>,
    /// Path to download all models to
    /// This command will download models to the specified path and then exit.
    /// Use --download-rt-detr2 or --download-yolo5 to download specific model types,
    /// otherwise all models will be downloaded.
    #[clap(long)]
    #[serde(skip)]
    pub download_model_path: Option<PathBuf>,
    /// Download only RT-DETR v2 models (use with --download-model-path)
    /// RT-DETR v2 models include: rt-detrv2-s, rt-detrv2-ms, rt-detrv2-m, rt-detrv2-l, rt-detrv2-x
    #[clap(long)]
    #[serde(skip)]
    pub download_rt_detr2: bool,
    /// Download only YOLO5 models (use with --download-model-path)
    /// YOLO5 models include specialized models for delivery, animals, birds, etc.
    #[clap(long)]
    #[serde(skip)]
    pub download_yolo5: bool,
    /// Download all models of all types (use with --download-model-path)
    /// This will download both RT-DETR v2 and YOLO5 models
    #[clap(long)]
    #[serde(skip)]
    pub download_all_models: bool,
    /// List all available models that can be downloaded
    #[clap(long)]
    #[serde(skip)]
    pub list_models: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            config: None,
            port: 32168,
            request_timeout: Duration::from_secs(15),
            worker_queue_size: None,
            model: None,
            object_detection_model_type: ObjectDetectionModel::default(),
            object_classes: None,
            object_filter: Vec::new(),
            log_level: LogLevel::Info,
            log_path: None,
            confidence_threshold: 0.5,
            force_cpu: false,
            #[cfg(target_os = "windows")]
            intra_threads: 192,
            #[cfg(not(target_os = "windows"))]
            intra_threads: 2,
            #[cfg(target_os = "windows")]
            inter_threads: 192,
            #[cfg(not(target_os = "windows"))]
            inter_threads: 2,
            save_image_path: None,
            save_ref_image: false,
            gpu_index: 0,
            save_stats_path: None,
            download_model_path: None,
            download_rt_detr2: false,
            download_yolo5: false,
            download_all_models: false,
            list_models: false,
        }
    }
}

impl Cli {
    /// Create a new Cli from a combination of config file and command line arguments
    /// If a config file is specified, it completely overrides CLI defaults
    /// If no config file is specified, create one from CLI args and make it the ground truth
    pub fn from_config_and_args() -> anyhow::Result<Self> {
        // First parse CLI to get the config file path
        let cli_args = Self::parse();

        if let Some(config_path) = &cli_args.config {
            // If config file is specified, use it entirely (ignore other CLI args)
            Self::load_config(config_path)
        } else {
            // No config file specified, create one from CLI arguments and save it
            let default_config_path = Self::get_default_config_path()?;

            // Save the current CLI args as the new config file
            cli_args.save_config(&default_config_path)?;
            tracing::info!(
                "Created config file from CLI arguments: {}",
                default_config_path.display()
            );

            // Now load it back with the config path set
            let mut config = cli_args;
            config.config = Some(default_config_path);
            Ok(config)
        }
    }
    /// Create a Cli from provided arguments with config file support
    /// If no config file is specified, create one from CLI args and make it the ground truth
    pub fn from_args_with_config(args: Vec<std::ffi::OsString>) -> anyhow::Result<Self> {
        let cli_args = Self::try_parse_from(args)?;

        if let Some(config_path) = &cli_args.config {
            // If config file is specified, use it entirely (ignore other CLI args)
            Self::load_config(config_path)
        } else {
            // No config file specified, create one from CLI arguments and save it
            let default_config_path = Self::get_default_config_path()?;

            // Save the current CLI args as the new config file
            cli_args.save_config(&default_config_path)?;
            tracing::info!(
                "Created config file from CLI arguments: {}",
                default_config_path.display()
            );

            // Now load it back with the config path set
            let mut config = cli_args;
            config.config = Some(default_config_path);
            Ok(config)
        }
    }

    /// Load configuration from a JSON file
    pub fn load_config(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read config file {}: {}", path.display(), e))?;

        let config: Self = serde_json::from_str(&content).map_err(|e| {
            anyhow::anyhow!("Failed to parse config file {}: {}", path.display(), e)
        })?;

        Ok(config)
    }

    /// Save current configuration to a JSON file
    pub fn save_config(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize config: {}", e))?;

        std::fs::write(path, content).map_err(|e| {
            anyhow::anyhow!("Failed to write config file {}: {}", path.display(), e)
        })?;
        Ok(())
    }

    /// Get the default config file path next to the executable
    pub fn get_default_config_path() -> anyhow::Result<PathBuf> {
        let exe_path = std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("Failed to get executable path: {}", e))?;

        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to get executable directory"))?;

        Ok(exe_dir.join("blue_onyx_config.json"))
    }
    /// Get the current config file path (either specified or default)
    pub fn get_current_config_path(&self) -> anyhow::Result<PathBuf> {
        if let Some(config_path) = &self.config {
            Ok(config_path.clone())
        } else {
            Self::get_default_config_path()
        }
    }

    /// Auto-save current configuration if no config file was used
    pub fn auto_save_if_no_config(&self) -> anyhow::Result<()> {
        // Only auto-save if no config file was specified
        if self.config.is_none() {
            let config_path = Self::get_default_config_path()?;

            // Don't overwrite if the file already exists (user might have customized it)
            if !config_path.exists() {
                self.save_config(&config_path)?;
                tracing::info!("Saved current configuration to: {}", config_path.display());
            }
        }
        Ok(())
    }
    /// Load configuration for service - uses blue_onyx_config_service.json
    /// Creates default config if file doesn't exist
    pub fn for_service() -> anyhow::Result<Self> {
        let exe_dir = std::env::current_exe()?
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Could not determine executable directory"))?
            .to_path_buf();

        let config_path = exe_dir.join("blue_onyx_config_service.json");

        if config_path.exists() {
            let mut config = Self::load_config(&config_path)?;
            config.config = Some(config_path);
            Ok(config)
        } else {
            // Create default config for service with Debug log level
            let default_config = Self {
                log_level: LogLevel::Debug,
                ..Default::default()
            };
            default_config.save_config(&config_path)?;
            tracing::info!(
                "Created default service config at: {}",
                config_path.display()
            );
            let mut config = default_config;
            config.config = Some(config_path);
            Ok(config)
        }
    }

    /// Print the current configuration in a human-readable format
    pub fn print_config(&self) {
        tracing::info!("=== Blue Onyx Configuration ===");
        tracing::info!("Server Configuration:");
        tracing::info!("  Port: {}", self.port);
        tracing::info!(
            "  Request timeout: {} seconds",
            self.request_timeout.as_secs()
        );

        if let Some(queue_size) = self.worker_queue_size {
            tracing::info!("  Worker queue size: {}", queue_size);
        } else {
            tracing::info!("  Worker queue size: auto-determined");
        }

        tracing::info!("Model Configuration:");
        tracing::info!(
            "  Detection model type: {}",
            self.object_detection_model_type
        );

        if let Some(model_path) = &self.model {
            tracing::info!("  Custom model path: {}", model_path.display());
        } else {
            tracing::info!("  Model: default (rt-detrv2-s.onnx)");
        }

        if let Some(classes_path) = &self.object_classes {
            tracing::info!("  Object classes: {}", classes_path.display());
        } else {
            tracing::info!("  Object classes: default (coco_classes.yaml)");
        }

        tracing::info!("Detection Configuration:");
        tracing::info!("  Confidence threshold: {:.2}", self.confidence_threshold);

        if !self.object_filter.is_empty() {
            tracing::info!("  Object filter: [{}]", self.object_filter.join(", "));
        } else {
            tracing::info!("  Object filter: none (all objects)");
        }

        tracing::info!("Performance Configuration:");
        tracing::info!("  Force CPU: {}", if self.force_cpu { "yes" } else { "no" });
        tracing::info!("  GPU index: {}", self.gpu_index);
        tracing::info!("  Intra threads: {}", self.intra_threads);
        tracing::info!("  Inter threads: {}", self.inter_threads);

        tracing::info!("Logging Configuration:");
        tracing::info!("  Log level: {:?}", self.log_level);

        if let Some(log_path) = &self.log_path {
            tracing::info!("  Log path: {}", log_path.display());
        } else {
            tracing::info!("  Log path: stdout");
        }

        tracing::info!("Output Configuration:");

        if let Some(save_path) = &self.save_image_path {
            tracing::info!("  Save processed images: {}", save_path.display());
            tracing::info!(
                "  Save reference images: {}",
                if self.save_ref_image { "yes" } else { "no" }
            );
        } else {
            tracing::info!("  Save processed images: disabled");
        }

        if let Some(stats_path) = &self.save_stats_path {
            tracing::info!("  Save statistics: {}", stats_path.display());
        } else {
            tracing::info!("  Save statistics: disabled");
        }

        if let Some(download_path) = &self.download_model_path {
            tracing::info!("  Download models to: {}", download_path.display());
        }

        tracing::info!("=== Configuration Complete ===");
    }
}

// Custom serde functions for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let secs: u64 = s.parse()?;
    Ok(Duration::from_secs(secs))
}
