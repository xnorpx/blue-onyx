use hf_hub::api::tokio::Api;
use tracing_subscriber::fmt::writer::OptionalWriter;
use std::path::PathBuf;
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

pub fn download_model(model_path: PathBuf, model: Model) -> anyhow::Result<()> {
    let all_models = get_all_models();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if !model_path.exists() {
            fs::create_dir_all(model_path.clone()).await?;
        }
        let api = Api::new()?;
        let mut downloaded_models: Vec<String> = Vec::new();

        match model {
            Model::Model(model_name) => {

            }
            Model::AllRtDetr2 => {
                download(api, RT_DETR2_MODELS, &mut downloaded_models, model_path).await?;
            }
            Model::AllYolo5 => {
                download(api, YOLO5_MODELS, &mut downloaded_models, model_path).await?;
            }
            Model::All => {
                for models in all_models.iter() {
                    download(
                        api.clone(),
                        *models,
                        &mut downloaded_models,
                        model_path.clone(),
                    )
                    .await?;
                }
            }
        }

        info!("Succesfully downloaded models: {:?}", downloaded_models);
        Ok(())
    })
}

async fn download(
    api: Api,
    models: (&str, &[&str]),
    downloaded_models: &mut Vec<String>,
    model_path: PathBuf,
    model_name: Option<PathBuf>,
) -> anyhow::Result<()> {
    let api_name = models.0;
    let api = api.model(api_name.to_string());
    for filename in models.1.iter() {
        if let Some(model_name) = model_name {
            if let Some(stem) = path.file_stem() {
                if let Some(parent) = path.parent() {
                    // Rebuild the path without the extension
                    return parent.join(stem);
                }
            }
        }
        let cached_file_path = api.get(filename).await?;
        tokio::fs::copy(cached_file_path, model_path.join(filename)).await?;
        info!(
            "Copied {} to {}",
            filename,
            model_path.join(filename).display()
        );
        downloaded_models.push(filename.to_string());
    }
    Ok(())
}
