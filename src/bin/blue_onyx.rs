use blue_onyx::download_models::Model;
use blue_onyx::{
    blue_onyx_service as create_blue_onyx_service, cli::Cli, init_logging, system_info::system_info,
};
use tracing::{info, warn};

fn main() -> anyhow::Result<()> {
    let mut args = Cli::from_config_and_args()?;
    let _guard = init_logging(args.log_level, &mut args.log_path);
    system_info()?;

    // Print the configuration being used
    args.print_config(); // Auto-save configuration if no config file was used
    args.auto_save_if_no_config()?;

    if args.list_models {
        blue_onyx::download_models::list_models();
        return Ok(());
    }
    // Check if any download flags are set
    if args.download_all_models || args.download_rt_detr2 || args.download_yolo5 {
        // Use specified path or default to current directory
        let download_path = args.download_model_path.unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()))
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
            blue_onyx::download_models::download_model(download_path, model_type).await
        })?;
        return Ok(());
    }

    // Run the tokio runtime on the main thread
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?; // Main server loop - restart if requested
    let mut current_args = args;
    #[allow(unused_assignments)] // False positive - we do use this in take() calls
    let mut current_thread_handle: Option<std::thread::JoinHandle<()>> = None;

    loop {
        let (blue_onyx_service_future, cancellation_token, _restart_token, thread_handle) =
            create_blue_onyx_service(current_args.clone())?;
        current_thread_handle = Some(thread_handle);

        let should_restart = rt.block_on(async {
            let ctrl_c_token = cancellation_token.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c()
                    .await
                    .expect("Failed to listen for Ctrl+C");
                info!("Ctrl+C received, shutting down server");
                ctrl_c_token.cancel();
            });

            blue_onyx_service_future
                .await
                .expect("Failed to run blue onyx service")
        });

        if should_restart {
            info!("Restarting server with updated configuration...");

            // Wait for the current worker thread to finish properly
            if let Some(handle) = current_thread_handle.take() {
                info!("Waiting for worker thread to shutdown...");
                if let Err(e) = handle.join() {
                    warn!("Worker thread didn't shutdown cleanly: {:?}", e);
                }
                info!("Worker thread shutdown complete");
            }
            // Reload configuration for restart
            current_args = Cli::from_config_and_args()?;
            // Note: Don't re-initialize logging during restart as the global subscriber is already set
            continue;
        } else {
            break; // Normal shutdown
        }
    }

    // Clean up the final worker thread on normal shutdown
    if let Some(handle) = current_thread_handle.take() {
        info!("Waiting for final worker thread to shutdown...");
        if let Err(e) = handle.join() {
            warn!("Final worker thread didn't shutdown cleanly: {:?}", e);
        }
    }

    Ok(())
}
