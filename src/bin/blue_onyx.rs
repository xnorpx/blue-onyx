use blue_onyx::download_models::Model;
use blue_onyx::{
    blue_onyx_service as create_blue_onyx_service, cli::Cli, init_logging,
    system_info::system_info, update_log_level,
};
use tracing::{error, info, warn};

fn main() -> anyhow::Result<()> {
    let mut args = Cli::from_config_and_args()?;
    let _guard = init_logging(args.log_level, &mut args.log_path)?;
    system_info()?; // Print the configuration being used
    args.print_config();

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
    } // Run the tokio runtime on the main thread
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    // Main server loop - restart if requested
    let mut current_args = args;

    // Set up Ctrl+C handler once, outside the restart loop
    let global_shutdown = tokio_util::sync::CancellationToken::new();
    let ctrl_c_shutdown = global_shutdown.clone();

    rt.spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for Ctrl+C");
        info!("Ctrl+C received, shutting down server");
        ctrl_c_shutdown.cancel();
    });

    loop {
        let (blue_onyx_service_future, cancellation_token, restart_token) =
            create_blue_onyx_service(current_args.clone())?;

        let should_restart = rt.block_on(async {
            // Wait for either the service to complete, restart to be requested, or global shutdown
            tokio::select! {
                result = blue_onyx_service_future => {
                    match result {
                        Ok((restart_requested, worker_handle)) => {
                            // Wait for worker thread to complete if available
                            if let Some(handle) = worker_handle {
                                info!("Waiting for worker thread to complete...");
                                if let Err(e) = handle.join() {
                                    error!("Worker thread panicked: {:?}", e);
                                }
                            }
                            restart_requested
                        },
                        Err(e) => {
                            error!("Service failed: {}", e);
                            false // Don't restart on error
                        }
                    }
                }
                _ = restart_token.cancelled() => {
                    info!("Restart requested via API");
                    true // Restart requested
                }                _ = global_shutdown.cancelled() => {
                    info!("Global shutdown requested");
                    cancellation_token.cancel(); // Cancel the current service
                    false // Don't restart, just exit
                }
            }
        });
        if should_restart {
            info!("Restarting server with updated configuration...");

            // Reload configuration for restart
            let new_args = Cli::from_config_and_args()?;

            // Check if log level changed and update dynamically
            if new_args.log_level != current_args.log_level {
                info!(
                    old_level = ?current_args.log_level,
                    new_level = ?new_args.log_level,
                    "Log level change detected, applying dynamically"
                );

                if let Err(e) = update_log_level(new_args.log_level) {
                    warn!("Failed to update log level dynamically: {}", e);
                }
            }

            current_args = new_args;
            continue;
        } else {
            break; // Normal shutdown
        }
    }

    Ok(())
}
