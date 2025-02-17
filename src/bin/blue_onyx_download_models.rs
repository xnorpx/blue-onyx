use blue_onyx::init_logging;
use std::{env, path::PathBuf};

fn print_help() {
    println!("Usage: blue_onyx_download_models [OPTIONS] [custom-model]");
    println!("If no path is specified, it will download in the same folder as this binary.");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--help" {
        print_help();
        return;
    }

    init_logging(blue_onyx::LogLevel::Info, &mut None);

    let download_model_path: PathBuf = if args.len() > 1 && args[1] != "custom-model" {
        PathBuf::from(&args[1])
    } else {
        env::current_exe().unwrap().parent().unwrap().to_path_buf()
    };

    let custom_model_yolo5 = args.iter().any(|arg| arg == "custom-model");

    if let Err(e) =
        blue_onyx::download_models::download_models(download_model_path, custom_model_yolo5)
    {
        eprintln!("Error downloading models: {}", e);
    }
}
