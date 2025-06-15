use blue_onyx::download_models::{download_model, list_models, Model};
use blue_onyx::init_logging;
use clap::{CommandFactory, Parser};
use std::env;
use std::path::PathBuf;

#[derive(Parser)]
#[clap(
    name = "blue_onyx_download_models",
    about = "A tool to download models for Blue Onyx"
)]
struct Cli {
    /// The name of the model to download (optional)
    #[clap(short, long, conflicts_with_all = &["all", "yolo5", "rt_detrv2", "list_models"])]
    model: Option<String>,

    /// Download all models
    #[clap(short, long, conflicts_with_all = &["model", "yolo5", "rt_detrv2", "list_models"])]
    all: bool,

    /// Download the YOLO5 model
    #[clap(long, conflicts_with_all = &["model", "all", "rt_detrv2", "list_models"])]
    yolo5: bool,

    /// Download the RT-DETRv2 model
    #[clap(long, conflicts_with_all = &["model", "all", "yolo5", "list_models"])]
    rt_detrv2: bool,

    /// List all available models
    #[clap(long, conflicts_with_all = &["model", "all", "yolo5", "rt_detrv2", "destination"])]
    list_models: bool,

    /// Optional destination path where the models will be downloaded
    #[clap(short, long)]
    destination: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(blue_onyx::LogLevel::Info, &mut None);

    let model_path = cli
        .destination
        .unwrap_or_else(|| env::current_exe().unwrap().parent().unwrap().to_path_buf());

    if cli.list_models {
        list_models();
    } else if cli.all {
        download_model(model_path, Model::All)?;
    } else if cli.yolo5 {
        download_model(model_path, Model::AllYolo5)?;
    } else if cli.rt_detrv2 {
        download_model(model_path, Model::AllRtDetr2)?;
    } else if let Some(name) = cli.model {
        download_model(model_path, Model::Model(name))?;
    } else {
        Cli::command().print_help().unwrap();
    }
    Ok(())
}
