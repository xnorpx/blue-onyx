use crate::{detector::ObjectDetectionModel, LogLevel};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

#[derive(Parser, Serialize, Deserialize)]
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
    /// Save inference stats to file
    #[clap(long)]
    pub save_stats_path: Option<PathBuf>,
    /// Path to download all models to
    /// This command will only download the models to the specified path
    /// and then exit
    #[clap(long)]
    pub download_model_path: Option<PathBuf>,
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
        }
    }
}

impl Cli {
    /// Create a new Cli from a combination of config file and command line arguments
    /// Command line arguments take precedence over config file values
    pub fn from_config_and_args() -> anyhow::Result<Self> {
        // First parse CLI to get the config file path
        let mut cli_args = Self::parse();

        if let Some(config_path) = &cli_args.config {
            let mut config = Self::load_config(config_path)?;

            // Override config values with CLI values that were explicitly set
            // This is a simplified approach - in practice you might want to detect
            // which CLI values were actually provided vs using defaults
            config.merge_with_cli(&cli_args);
            cli_args = config;
        }

        Ok(cli_args)
    }

    /// Create a Cli from provided arguments with config file support
    pub fn from_args_with_config(args: Vec<std::ffi::OsString>) -> anyhow::Result<Self> {
        let mut cli_args = Self::try_parse_from(args)?;

        if let Some(config_path) = &cli_args.config {
            let mut config = Self::load_config(config_path)?;
            config.merge_with_cli(&cli_args);
            cli_args = config;
        }

        Ok(cli_args)
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

    /// Merge CLI arguments with config values (CLI takes precedence)
    fn merge_with_cli(&mut self, cli: &Self) {
        // Override config with CLI values
        // Note: This is a simple merge - you might want more sophisticated logic
        // to only override when CLI values are explicitly set (not defaults)

        self.config = cli.config.clone();
        self.port = cli.port;
        self.request_timeout = cli.request_timeout;
        if cli.worker_queue_size.is_some() {
            self.worker_queue_size = cli.worker_queue_size;
        }
        if cli.model.is_some() {
            self.model = cli.model.clone();
        }
        self.object_detection_model_type = cli.object_detection_model_type.clone();
        if cli.object_classes.is_some() {
            self.object_classes = cli.object_classes.clone();
        }
        if !cli.object_filter.is_empty() {
            self.object_filter = cli.object_filter.clone();
        }
        self.log_level = cli.log_level;
        if cli.log_path.is_some() {
            self.log_path = cli.log_path.clone();
        }
        self.confidence_threshold = cli.confidence_threshold;
        self.force_cpu = cli.force_cpu;
        self.intra_threads = cli.intra_threads;
        self.inter_threads = cli.inter_threads;
        if cli.save_image_path.is_some() {
            self.save_image_path = cli.save_image_path.clone();
        }
        self.save_ref_image = cli.save_ref_image;
        self.gpu_index = cli.gpu_index;
        if cli.save_stats_path.is_some() {
            self.save_stats_path = cli.save_stats_path.clone();
        }
        if cli.download_model_path.is_some() {
            self.download_model_path = cli.download_model_path.clone();
        }
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
            Self::load_config(&config_path)
        } else {
            // Create default config
            let default_config = Self::default();
            default_config.save_config(&config_path)?;
            tracing::info!(
                "Created default service config at: {}",
                config_path.display()
            );
            Ok(default_config)
        }
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
