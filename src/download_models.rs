use hf_hub::api::tokio::Api;
use std::path::PathBuf;
use tokio::fs;
use tracing::info;

const RT_DETR2_MODELS: &[(&str, &[&str])] = &[(
    // Messed up the repo name, should be rt-detr2-onnx
    "xnorpx/rt-detr2-onnx",
    &[
        "rt-detrv2-s.onnx",
        "rt-detrv2-ms.onnx",
        "rt-detrv2-m.onnx",
        "rt-detrv2-l.onnx",
        "rt-detrv2-x.onnx",
    ],
)];

const CUSTOM_YOLO5_MODELS: &[(&str, &[&str])] = &[(
    // Messed up the repo name, should be rt-detr2-onnx
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
)];

pub fn download_models(model_path: PathBuf, download_custom_yolo5: bool) -> anyhow::Result<()> {
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
        if download_custom_yolo5 {
            download(api, CUSTOM_YOLO5_MODELS, &mut downloaded_models, model_path)
                .await
                .unwrap();
        } else {
            download(api, RT_DETR2_MODELS, &mut downloaded_models, model_path)
                .await
                .unwrap();
        }
        info!("Succesfully downloaded models: {:?}", downloaded_models);
        Ok(())
    })
}

async fn download(
    api: Api,
    models: &[(&str, &[&str])],
    downloaded_models: &mut Vec<String>,
    model_path: PathBuf,
) -> anyhow::Result<()> {
    for (api_name, models) in models.iter() {
        let api = api.model(api_name.to_string());
        for filename in models.iter() {
            let cached_file_path = api.get(filename).await?;
            tokio::fs::copy(cached_file_path, model_path.join(filename)).await?;
            info!(
                "Copied {} to {}",
                filename,
                model_path.join(filename).display()
            );
            downloaded_models.push(filename.to_string());
        }
    }
    Ok(())
}
