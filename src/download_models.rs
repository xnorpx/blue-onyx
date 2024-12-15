use std::path::PathBuf;
use tokio::fs;
use tracing::info;

const RT_DETR2_MODELS: &[(&str, &[&str])] = &[(
    // TODO: Update the model repository name to rt-detrv2-onnx
    "xnorpx/rt-detr2-onnx",
    &[
        "rt-detrv2-s.onnx",
        "rt-detrv2-ms.onnx",
        "rt-detrv2-m.onnx",
        "rt-detrv2-l.onnx",
        "rt-detrv2-x.onnx",
    ],
)];

pub fn download_models(model_path: PathBuf) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if !model_path.exists() {
            fs::create_dir_all(model_path.clone()).await?;
        }
        let mut downloaded_models = Vec::new();
        for (api_name, models) in RT_DETR2_MODELS.iter() {
            let api = hf_hub::api::tokio::Api::new()?;
            let api = api.model(api_name.to_string());
            for filename in models.iter() {
                let cached_file_path = api.get(filename).await?;
                tokio::fs::copy(cached_file_path, model_path.join(filename)).await?;
                info!(
                    "Copied {} to {}",
                    filename,
                    model_path.join(filename).display()
                );
                downloaded_models.push(filename);
            }
        }
        info!("Succesfully downloaded models: {:?}", downloaded_models);
        Ok(())
    })
}
