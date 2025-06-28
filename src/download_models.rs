use hf_hub::api::tokio::Api;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::info;

pub enum Model {
    Model(String),
    AllRtDetr2,
    AllYolo5,
    All,
}

pub const RT_DETR2_MODELS: (&str, &[&str]) = (
    "xnorpx/rt-detr2-onnx",
    &[
        "rt-detrv2-s.onnx",
        "rt-detrv2-s.yaml",
        "rt-detrv2-ms.onnx",
        "rt-detrv2-ms.yaml",
        "rt-detrv2-m.onnx",
        "rt-detrv2-m.yaml",
        "rt-detrv2-l.onnx",
        "rt-detrv2-l.yaml",
        "rt-detrv2-x.onnx",
        "rt-detrv2-x.yaml",
    ],
);

pub const YOLO5_MODELS: (&str, &[&str]) = (
    "xnorpx/blue-onyx-yolo5",
    &[
        "delivery.onnx",
        "delivery.yaml",
        "IPcam-animal.onnx",
        "IPcam-animal.yaml",
        "ipcam-bird.onnx",
        "ipcam-bird.yaml",
        "IPcam-combined.onnx",
        "IPcam-combined.yaml",
        "IPcam-dark.onnx",
        "IPcam-dark.yaml",
        "IPcam-general.onnx",
        "IPcam-general.yaml",
        "package.onnx",
        "package.yaml",
    ],
);

pub fn get_all_models() -> [(&'static str, &'static [&'static str]); 2] {
    [RT_DETR2_MODELS, YOLO5_MODELS]
}

pub fn get_all_model_names() -> Vec<String> {
    let all_models = get_all_models();
    let mut models = Vec::new();
    for model_set in all_models.iter() {
        for file in model_set.1.iter() {
            if file.ends_with(".onnx") {
                models.push(file.to_string());
            }
        }
    }
    models
}

pub fn list_models() {
    let model_names = get_all_model_names();
    for model_name in model_names {
        info!("{}", model_name);
    }
}

/// Download models based on the Model enum
pub async fn download_model(model_path: PathBuf, model: Model) -> anyhow::Result<()> {
    if !model_path.exists() {
        fs::create_dir_all(model_path.clone()).await?;
    }

    let mut downloaded_models: Vec<String> = Vec::new();

    match model {
        Model::Model(model_name) => {
            // Download specific model and its yaml
            download_file_to_dir(&model_name, &model_path).await?;
            downloaded_models.push(model_name.clone());

            let yaml_name = model_name.replace(".onnx", ".yaml");
            match download_file_to_dir(&yaml_name, &model_path).await {
                Ok(_) => {
                    downloaded_models.push(yaml_name);
                }
                Err(e) => {
                    info!("Warning: Failed to download YAML file {}: {}", yaml_name, e);
                    info!("The model will still work but may use default object classes");
                }
            }
        }
        Model::AllRtDetr2 => {
            download_repository_files(RT_DETR2_MODELS, &model_path, &mut downloaded_models).await?;
        }
        Model::AllYolo5 => {
            download_repository_files(YOLO5_MODELS, &model_path, &mut downloaded_models).await?;
        }
        Model::All => {
            let all_models = get_all_models();
            for model_repo in all_models.iter() {
                download_repository_files(*model_repo, &model_path, &mut downloaded_models).await?;
            }
        }
    }

    info!("Successfully downloaded models: {:?}", downloaded_models);
    Ok(())
}

/// Download all files from a specific repository
async fn download_repository_files(
    models: (&str, &[&str]),
    target_dir: &Path,
    downloaded_models: &mut Vec<String>,
) -> anyhow::Result<()> {
    let (repo_name, files) = models;
    let api = Api::new()?;
    let api_repo = api.model(repo_name.to_string());

    let mut errors = Vec::new();

    for filename in files.iter() {
        match api_repo.get(filename).await {
            Ok(cached_file) => {
                let target_path = target_dir.join(filename);

                match fs::copy(&cached_file, &target_path).await {
                    Ok(_) => {
                        info!("Downloaded {} to {}", filename, target_path.display());
                        downloaded_models.push(filename.to_string());
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to copy {} to {}: {}",
                            filename,
                            target_path.display(),
                            e
                        );
                        info!("Warning: {}", error_msg);
                        errors.push(error_msg);
                    }
                }
            }
            Err(e) => {
                let error_msg =
                    format!("Failed to download {filename} from {repo_name}: {e}");
                info!("Warning: {}", error_msg);
                errors.push(error_msg);
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        info!("Some files failed to download but continuing: {:?}", errors);
        Ok(()) // Don't fail the entire operation for missing individual files
    }
}

/// Download a specific file from any of the available repositories
pub async fn download_file_to_dir(filename: &str, target_dir: &Path) -> anyhow::Result<()> {
    let all_models = get_all_models();

    for (repo_name, files) in all_models.iter() {
        if files.contains(&filename) {
            info!("Found {} in repository {}", filename, repo_name);

            // Check if target directory exists
            if !target_dir.exists() {
                fs::create_dir_all(target_dir).await?;
            }

            let api = Api::new()?;
            let api_repo = api.model(repo_name.to_string());

            // Download the file
            let cached_file = api_repo.get(filename).await?;
            let target_path = target_dir.join(filename);

            fs::copy(&cached_file, &target_path).await?;
            info!("Downloaded {} to {}", filename, target_path.display());
            return Ok(());
        }
    }
    Err(anyhow::anyhow!(
        "File {} not found in any repository",
        filename
    ))
}
