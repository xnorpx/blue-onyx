use blue_onyx::{
    detector::DetectorConfig,
    init_logging,
    server::{run_server, Metrics},
    system_info::{self, system_info},
    worker::DetectorWorker,
    LogLevel,
};
use clap::Parser;
use std::{path::PathBuf, sync::mpsc::channel};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use windows::Win32::System::Threading::{
    GetCurrentProcessorNumber, GetCurrentThread, SetThreadAffinityMask, SetThreadPriority,
    THREAD_PRIORITY_TIME_CRITICAL,
};

#[derive(Parser)]
#[command(author = "Marcus Asteborg", version=env!("CARGO_PKG_VERSION"), about = "TODO")]
struct Cli {
    /// The port on which the server will listen for HTTP requests.
    /// Default is 32168. Example usage: --port 1337
    //#[arg(long, default_value_t = 32168)]
    #[arg(long, default_value_t = 32168)]
    pub port: u16,
    /// Path to the ONNX rt-detrv2 onnx model file.
    /// If not given the default model small model is used.
    #[clap(long)]
    model: Option<PathBuf>,
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
    /// If log_path is set, then stdout logging will be disabled and it will log to file
    #[clap(long)]
    log_path: Option<PathBuf>,
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
    /// Optional path to save the processed images
    #[clap(long)]
    save_image_path: Option<PathBuf>,
    /// Save the reference image (only if save_image_path is provided)
    #[clap(long, default_value_t = false)]
    save_ref_image: bool,
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
    let parse = Cli::parse();
    let args = parse;
    init_logging(args.log_level, None);
    system_info()?;

    if args.download_model_path.is_some() {
        blue_onyx::download_models::download_models(args.download_model_path.unwrap())?;
        return Ok(());
    }

    let detector_config = DetectorConfig {
        model: args.model,
        object_classes: args.object_classes,
        object_filter: args.object_filter,
        confidence_threshold: args.confidence_threshold,
        force_cpu: args.force_cpu,
        save_image_path: args.save_image_path,
        save_ref_image: args.save_ref_image,
        gpu_index: args.gpu_index,
        intra_threads: args.intra_threads,
        inter_threads: args.inter_threads,
    };

    let (sender, receive) = channel();
    // Run a separate thread for the detector worker
    let mut detector_worker = DetectorWorker::new(detector_config, receive)?;

    let detector = detector_worker.get_detector();
    let model_name = detector.get_model_name();
    let using_gpu = detector.is_using_gpu();
    let execution_providers_name = detector.get_endpoint_provider_name();

    let device_name = if using_gpu {
        system_info::gpu_model(args.gpu_index as usize)
    } else {
        system_info::cpu_model()
    };
    let metrics = Metrics::new(
        model_name.clone(),
        device_name,
        execution_providers_name,
        using_gpu,
    );

    let thread_handle = std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        unsafe {
            let thread_handle = GetCurrentThread();
            if let Err(err) = SetThreadPriority(thread_handle, THREAD_PRIORITY_TIME_CRITICAL) {
                error!(?err, "Failed to set thread priority to time critical");
            }
            let processor_number = GetCurrentProcessorNumber();
            let core_mask = 1usize << processor_number;
            let previous_mask = SetThreadAffinityMask(thread_handle, core_mask);
            if previous_mask == 0 {
                error!("Failed to set thread affinity.");
            }
        }
        detector_worker.run();
    });

    // Run the tokio runtime on the main thread
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let cancellation_token = CancellationToken::new();
        let ctrl_c_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");
            info!("Ctrl+C received, shutting down server");
            ctrl_c_token.cancel();
        });

        run_server(args.port, cancellation_token, sender, metrics)
            .await
            .expect("Failed to run server");
    });

    thread_handle.join().expect("Failed to join worker thread");

    Ok(())
}
