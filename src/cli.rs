use crate::{LogLevel, detector::ObjectDetectionModel, download_models::Model, init_logging};
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
    /// CLI arguments always override config file values
    pub fn from_config_and_args() -> anyhow::Result<Option<Self>> {
        // First parse CLI to get the config file path and all CLI arguments
        let mut args = Self::parse();

        if args.list_models {
            let _guard = init_logging(args.log_level, &mut args.log_path)?;
            crate::download_models::list_models();
            return Ok(None);
        }
        // Check if any download flags are set
        if args.download_all_models || args.download_rt_detr2 || args.download_yolo5 {
            let _guard = init_logging(args.log_level, &mut args.log_path)?;
            // Use specified path or default to current directory
            let download_path = args.download_model_path.unwrap_or_else(|| {
                if let Ok(exe) = std::env::current_exe() && let Some(parent) = exe.parent() {
                    parent.to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_else(|_| ".".into())
                }
            }); // Determine what to download based on flags
            let model_type = match (
                args.download_all_models,
                args.download_rt_detr2,
                args.download_yolo5,
            ) {
                (true, _, _) => Model::All,
                (false, true, true) => Model::All,
                (false, true, false) => Model::AllRtDetr2,
                (false, false, true) => Model::AllYolo5,
                (false, false, false) => unreachable!("No download flags set"),
            };

            // Create async runtime for download operation
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;

            rt.block_on(async {
                crate::download_models::download_model(download_path, model_type).await
            })?;
            return Ok(None);
        } // Run the tokio runtime on the main thread

        if let Some(config_path) = args.config.clone() {
            let config_file = Self::load_config(&config_path)?;
            Ok(Some(Self::merge_config_with_cli_args(
                config_file,
                args,
                config_path,
            )))
        } else {
            // No config file specified, check if default config exists
            let default_config_path = Self::get_default_config_path()?;

            if default_config_path.exists() {
                // Load existing default config file and merge with CLI args
                tracing::info!(
                    "Loading existing config file and merging with CLI arguments: {}",
                    default_config_path.display()
                );
                let config_file = Self::load_config(&default_config_path)?;
                Ok(Some(Self::merge_config_with_cli_args(
                    config_file,
                    args,
                    default_config_path,
                )))
            } else {
                // Create new config file from CLI arguments
                args.save_config(&default_config_path)?;
                tracing::info!(
                    "Created config file from CLI arguments: {}",
                    default_config_path.display()
                );

                // Now load it back with the config path set
                let mut config = args;
                config.config = Some(default_config_path);
                Ok(Some(config))
            }
        }
    }
    /// Create a Cli from provided arguments with config file support
    /// CLI arguments always override config file values
    pub fn from_args_with_config(args: Vec<std::ffi::OsString>) -> anyhow::Result<Self> {
        let cli_args = Self::try_parse_from(args)?;
        if let Some(config_path) = cli_args.config.clone() {
            // If config file is specified, load it and merge with CLI args
            let config_file = Self::load_config(&config_path)?;
            Ok(Self::merge_config_with_cli_args(
                config_file,
                cli_args,
                config_path,
            ))
        } else {
            // No config file specified, check if default config exists
            let default_config_path = Self::get_default_config_path()?;

            if default_config_path.exists() {
                // Load existing default config file and merge with CLI args
                tracing::info!(
                    "Loading existing config file and merging with CLI arguments: {}",
                    default_config_path.display()
                );
                let config_file = Self::load_config(&default_config_path)?;
                Ok(Self::merge_config_with_cli_args(
                    config_file,
                    cli_args,
                    default_config_path,
                ))
            } else {
                // Create new config file from CLI arguments
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

    /// Merge config file values with CLI arguments, where CLI arguments take precedence
    /// CLI arguments override config file values when they are explicitly provided
    fn merge_config_with_cli_args(
        mut config_file: Self,
        cli_args: Self,
        config_path: PathBuf,
    ) -> Self {
        // Use clap's built-in logic to determine which values were explicitly set
        // We'll create default CLI args and compare with the parsed CLI args
        let defaults = Self::default();

        // Override config file values with CLI arguments when they differ from defaults
        // This approach assumes that if a CLI arg differs from its default, it was explicitly set

        if cli_args.port != defaults.port {
            config_file.port = cli_args.port;
        }
        if cli_args.request_timeout != defaults.request_timeout {
            config_file.request_timeout = cli_args.request_timeout;
        }
        if cli_args.worker_queue_size != defaults.worker_queue_size {
            config_file.worker_queue_size = cli_args.worker_queue_size;
        }
        if cli_args.model != defaults.model {
            config_file.model = cli_args.model;
        }
        if cli_args.object_detection_model_type != defaults.object_detection_model_type {
            config_file.object_detection_model_type = cli_args.object_detection_model_type;
        }
        if cli_args.object_classes != defaults.object_classes {
            config_file.object_classes = cli_args.object_classes;
        }
        if cli_args.object_filter != defaults.object_filter {
            config_file.object_filter = cli_args.object_filter;
        }
        if cli_args.log_level != defaults.log_level {
            config_file.log_level = cli_args.log_level;
        }
        if cli_args.log_path != defaults.log_path {
            config_file.log_path = cli_args.log_path;
        }
        if cli_args.confidence_threshold != defaults.confidence_threshold {
            config_file.confidence_threshold = cli_args.confidence_threshold;
        }
        if cli_args.save_image_path != defaults.save_image_path {
            config_file.save_image_path = cli_args.save_image_path;
        }
        if cli_args.save_ref_image != defaults.save_ref_image {
            config_file.save_ref_image = cli_args.save_ref_image;
        }
        if cli_args.save_stats_path != defaults.save_stats_path {
            config_file.save_stats_path = cli_args.save_stats_path;
        }
        if cli_args.force_cpu != defaults.force_cpu {
            config_file.force_cpu = cli_args.force_cpu;
        }
        if cli_args.gpu_index != defaults.gpu_index {
            config_file.gpu_index = cli_args.gpu_index;
        }
        if cli_args.intra_threads != defaults.intra_threads {
            config_file.intra_threads = cli_args.intra_threads;
        }
        if cli_args.inter_threads != defaults.inter_threads {
            config_file.inter_threads = cli_args.inter_threads;
        }

        // Set the config path
        config_file.config = Some(config_path.clone());

        // Save the merged configuration back to the config file
        if let Err(e) = config_file.save_config(&config_path) {
            tracing::warn!("Failed to save merged configuration: {}", e);
        } else {
            tracing::info!("Saved merged configuration to: {}", config_path.display());
        }

        config_file
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
