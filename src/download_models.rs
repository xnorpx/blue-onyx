use hf_hub::api::tokio::Api;
use std::path::PathBuf;
use tokio::fs;
use tracing::info;

pub enum Model {
    Model(String),
    AllRtDetr2,
    AllYolo5,
    All,
}

pub const RT_DETR2_MODELS: &[(&str, &[&str])] = &[(
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
)];

pub const YOLO5_MODELS: &[(&str, &[&str])] = &[(
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

pub fn get_all_models() -> [&'static [(&'static str, &'static [&'static str])]; 2] {
    [RT_DETR2_MODELS, YOLO5_MODELS]
}

pub fn get_all_model_names() -> Vec<String> {
    let all_models = get_all_models();
    let mut models = Vec::new();
    for model_set in all_models.iter() {
        for (_, files) in model_set.iter() {
            for file in *files {
                if file.ends_with(".onnx") {
                    models.push(file.to_string());
                }
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
                let mut found = false;
                for models in all_models.iter() {
                    for (api_name, files) in models.iter() {
                        if files.contains(&model_name.as_str()) {
                            found = true;
                            let api = api.model(api_name.to_string());
                            let cached_file_path = api.get(&model_name).await?;
                            tokio::fs::copy(cached_file_path, model_path.join(&model_name)).await?;
                            info!(
                                "Copied {} to {}",
                                model_name,
                                model_path.join(&model_name).display()
                            );
                            downloaded_models.push(model_name.clone());

                            // Download the corresponding yaml file
                            if let Some(yaml_name) = model_name
                                .strip_suffix(".onnx")
                                .map(|name| format!("{}.yaml", name))
                            {
                                if files.contains(&yaml_name.as_str()) {
                                    let cached_file_path = api.get(&yaml_name).await?;
                                    tokio::fs::copy(cached_file_path, model_path.join(&yaml_name))
                                        .await?;
                                    info!(
                                        "Copied {} to {}",
                                        yaml_name,
                                        model_path.join(&yaml_name).display()
                                    );
                                    downloaded_models.push(yaml_name);
                                }
                            }
                            break;
                        }
                    }
                }
                if !found {
                    return Err(anyhow::anyhow!("Model {} not found", model_name));
                }
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
                        models,
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
