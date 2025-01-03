use blue_onyx::{blue_onyx_service, cli::Cli, init_logging, system_info::system_info};
use clap::Parser;
use tracing::info;

fn main() -> anyhow::Result<()> {
    let parse = Cli::parse();
    let args = parse;
    init_logging(args.log_level, args.log_path.clone());
    system_info()?;

    if args.download_model_path.is_some() {
        blue_onyx::download_models::download_models(args.download_model_path.unwrap(), false)?;
        return Ok(());
    }

    let (blue_onyx_service, cancellation_token, thread_handle) = blue_onyx_service(args)?;

    // Run the tokio runtime on the main thread
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    rt.block_on(async {
        let ctrl_c_token = cancellation_token.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");
            info!("Ctrl+C received, shutting down server");
            ctrl_c_token.cancel();
        });
        blue_onyx_service
            .await
            .expect("Failed to run blue onyx service");
    });
    thread_handle.join().expect("Failed to join worker thread");
    Ok(())
}
