use crate::{
    api::Prediction,
    direct_ml_available, get_object_classes,
    image::{
        create_od_image_name, decode_jpeg, encode_maybe_draw_boundary_boxes_and_save_jpeg, Image,
        Resizer,
    },
};
use anyhow::bail;
use bytes::Bytes;
use ndarray::{Array, ArrayView, Axis};
use ort::{
    execution_providers::DirectMLExecutionProvider,
    inputs,
    session::{Session, SessionOutputs},
};
use smallvec::SmallVec;
use std::{
    fmt::Debug,
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, error, info, warn};

pub struct DetectResult {
    pub predictions: SmallVec<[Prediction; 10]>,
    pub processing_time: std::time::Duration,
    pub decode_image_time: std::time::Duration,
    pub resize_image_time: std::time::Duration,
    pub pre_processing_time: std::time::Duration,
    pub inference_time: std::time::Duration,
    pub post_processing_time: std::time::Duration,
    pub device_type: DeviceType,
    pub endpoint_provider: EndpointProvider,
}

impl Debug for DetectResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetectResult")
            .field("# predictions", &self.predictions)
            .field("processing_time", &self.processing_time)
            .field("decode_image_time", &self.decode_image_time)
            .field("resize_image_time", &self.resize_image_time)
            .field("pre_processing_time", &self.pre_processing_time)
            .field("inference_time", &self.inference_time)
            .field("post_processing_time", &self.post_processing_time)
            .field("device_type", &self.device_type)
            .finish()
    }
}

pub struct Detector {
    session: Session,
    resizer: Resizer,
    decoded_image: Image,
    resized_image: Image,
    object_classes: Vec<String>,
    object_filter: Option<Vec<bool>>,
    input: ndarray::ArrayBase<ndarray::OwnedRepr<f32>, ndarray::Dim<[usize; 4]>>,
    confidence_threshold: f32,
    device_type: DeviceType,
    endpoint_provider: EndpointProvider,
    save_image_path: Option<PathBuf>,
    save_ref_image: bool,
    model_name: String,
}

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub model: Option<PathBuf>,
    pub object_classes: Option<PathBuf>,
    pub object_filter: Vec<String>,
    pub confidence_threshold: f32,
    pub force_cpu: bool,
    pub save_image_path: Option<PathBuf>,
    pub save_ref_image: bool,
    pub gpu_index: i32,
    pub intra_threads: usize,
    pub inter_threads: usize,
    pub timeout: Duration,
}

impl Detector {
    pub fn new(detector_config: DetectorConfig) -> anyhow::Result<Self> {
        let object_classes = get_object_classes(detector_config.object_classes)?;
        let mut providers = Vec::new();
        let mut device_type = DeviceType::CPU;
        let (num_intra_threads, num_inter_threads) = if detector_config.force_cpu {
            let num_intra_threads = detector_config
                .intra_threads
                .min(num_cpus::get_physical() - 1);
            let num_inter_threads = detector_config
                .inter_threads
                .min(num_cpus::get_physical() - 1);
            info!(
                "Forcing CPU for inference with {} intra and {} inter threads",
                num_intra_threads, num_inter_threads
            );
            (num_intra_threads, num_inter_threads)
        } else if direct_ml_available() {
            providers.push(
                DirectMLExecutionProvider::default()
                    .with_device_id(detector_config.gpu_index)
                    .build()
                    .error_on_failure(),
            );
            device_type = DeviceType::GPU;
            (1, 1) // For GPU we just hardcode to 1 thread
        } else {
            let num_intra_threads = detector_config
                .intra_threads
                .min(num_cpus::get_physical() - 1);
            let num_inter_threads = detector_config
                .inter_threads
                .min(num_cpus::get_physical() - 1);
            warn!("DirectML not available, falling back to CPU for inference with {} intra and {} inter threads", num_intra_threads, num_inter_threads);
            (detector_config.intra_threads, detector_config.inter_threads)
        };

        let mut object_filter = None;
        if !detector_config.object_filter.is_empty() {
            let mut object_filter_vector = vec![false; object_classes.len()];
            for object in detector_config.object_filter.iter() {
                if let Some(index) = object_classes
                    .iter()
                    .position(|x| x.to_lowercase() == object.to_lowercase())
                {
                    object_filter_vector[index] = true;
                }
            }
            object_filter = Some(object_filter_vector);
        }

        let (model_bytes, model_name) = if let Some(model) = detector_config.model {
            let model_str = model
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Failed to convert model path to string"))?
                .to_string();
            (std::fs::read(&model)?, model_str)
        } else {
            let exe_path: PathBuf = std::env::current_exe()?;
            let model_path = exe_path
                .parent()
                .ok_or_else(|| {
                    anyhow::anyhow!("Failed to get parent directory of executable path")
                })?
                .join(crate::SMALL_RT_DETR_V2_MODEL_FILE_NAME);
            let Ok(model_bytes) = std::fs::read(&model_path) else {
                error!("Failed to read model file: {:?} ensure you either specify a model or that {} is in the same directory as binary", model_path, crate::SMALL_RT_DETR_V2_MODEL_FILE_NAME.to_string());
                bail!("Failed to read model file");
            };
            (
                model_bytes,
                crate::SMALL_RT_DETR_V2_MODEL_FILE_NAME.to_string(),
            )
        };

        info!(
            "Initializing detector with model: {:?}, #{} classes and inference running on {}",
            model_name,
            object_classes.len(),
            device_type,
        );

        let session = Session::builder()?
            .with_execution_providers(providers)?
            .with_intra_threads(num_intra_threads)?
            .with_inter_threads(num_inter_threads)?
            .commit_from_memory(model_bytes.as_slice())?;

        let endpoint_provider = match device_type {
            DeviceType::GPU => EndpointProvider::DirectML,
            DeviceType::CPU => EndpointProvider::CPU,
        };

        let mut detector = Self {
            model_name,
            endpoint_provider,
            session,
            resizer: Resizer::default(),
            decoded_image: Image::default(),
            resized_image: Image::default(),
            input: Array::zeros((1, 3, 640, 640)),
            object_classes,
            object_filter,
            confidence_threshold: detector_config.confidence_threshold,
            device_type,
            save_image_path: detector_config.save_image_path,
            save_ref_image: detector_config.save_ref_image,
        };

        // Warmup
        info!("Warming up the detector");
        let detector_warmup_start_time = Instant::now();
        detector.detect(Bytes::from(crate::DOG_BIKE_CAR_BYTES), None, None)?;
        info!(
            "Detector warmed up in: {:?}",
            detector_warmup_start_time.elapsed()
        );
        Ok(detector)
    }

    pub fn detect(
        &mut self,
        image_bytes: Bytes,
        image_name: Option<String>,
        min_confidence: Option<f32>,
    ) -> anyhow::Result<DetectResult> {
        // Process from here
        if let Some(image_name) = image_name.clone() {
            debug!("Detecting objects in image: {}", image_name);
            if let Some(save_image_path) = self.save_image_path.clone() {
                if self.save_ref_image {
                    let save_image_path = save_image_path.to_path_buf();
                    let image_path_buf = PathBuf::from(image_name.clone());
                    let image_name_ref = image_path_buf
                        .file_name()
                        .ok_or_else(|| anyhow::anyhow!("Failed to get file name from path"))?;
                    let save_image_path = save_image_path.join(image_name_ref);
                    std::fs::write(save_image_path, &image_bytes)?;
                }
            }
        }

        let processing_time_start = Instant::now();
        decode_jpeg(image_name.clone(), image_bytes, &mut self.decoded_image)?;
        let decode_image_time = processing_time_start.elapsed();
        debug!(
            "Decode image time: {:?}, resolution {}x{}",
            decode_image_time, self.decoded_image.width, self.decoded_image.height
        );
        let resize_factor_x = self.decoded_image.width as f32 / 640.0;
        let resize_factor_y = self.decoded_image.height as f32 / 640.0;
        let orig_size = Array::from_shape_vec(
            (1, 2),
            vec![
                self.resized_image.width as i64,
                self.resized_image.height as i64,
            ],
        )?;

        let resize_image_start_time = Instant::now();
        self.resizer
            .resize_image(&mut self.decoded_image, &mut self.resized_image)?;
        let resize_image_time = resize_image_start_time.elapsed();
        debug!("Resize image time: {:#?}", resize_image_time);

        for (i, pixel) in self.resized_image.pixels.iter().enumerate() {
            let channel = i % 3;
            let value = *pixel as f32 / 255.0;
            let index = i / 3;
            let y = index / 640;
            let x = index % 640;
            self.input[[0, channel, y, x]] = value;
        }
        let pre_processing_time = processing_time_start.elapsed();
        debug!("Pre-process time: {:?}", pre_processing_time);

        let start_inference_time = std::time::Instant::now();
        let outputs: SessionOutputs = self.session.run(
            inputs!["images" => self.input.view(), "orig_target_sizes" => orig_size.view()]?,
        )?;
        let inference_time = start_inference_time.elapsed();
        debug!("Inference time: {:?}", inference_time);

        let post_processing_time_start = Instant::now();

        let labels: ArrayView<i64, _> = outputs["labels"].try_extract_tensor::<i64>()?;
        let bboxes: ArrayView<f32, _> = outputs["boxes"].try_extract_tensor::<f32>()?;
        let scores: ArrayView<f32, _> = outputs["scores"].try_extract_tensor::<f32>()?;

        let labels = labels.index_axis(Axis(0), 0);
        let bboxes = bboxes.index_axis(Axis(0), 0);
        let scores = scores.index_axis(Axis(0), 0);

        let mut predictions = SmallVec::<[Prediction; 10]>::new();

        let confidence_threshold = min_confidence.unwrap_or(self.confidence_threshold);

        for (i, bbox) in bboxes.outer_iter().enumerate() {
            if scores[i] > confidence_threshold {
                // If object filter is set, skip objects that are not in the filter
                if let Some(object_filter) = self.object_filter.as_ref() {
                    // If the object is not in the filter, skip it
                    if !object_filter[labels[i] as usize] {
                        continue;
                    }
                }
                let prediction = Prediction {
                    x_min: (bbox[0] * resize_factor_x) as usize,
                    x_max: (bbox[2] * resize_factor_x) as usize,
                    y_min: (bbox[1] * resize_factor_y) as usize,
                    y_max: (bbox[3] * resize_factor_y) as usize,
                    confidence: scores[i],
                    label: self.object_classes[labels[i] as usize].clone(),
                };
                debug!("Prediction - {}: {:?}", predictions.len() + 1, prediction);
                predictions.push(prediction);
            }
        }

        let now = Instant::now();
        let post_processing_time = now.duration_since(post_processing_time_start);
        debug!("Post-processing time: {:?}", post_processing_time);
        let processing_time = now.duration_since(processing_time_start);
        // Processing time is mainly composed of:
        //  1. Image decoding time
        //  2. Image resizing time
        //  3. Inference time
        debug!("Processing time: {:?}", processing_time);

        if !predictions.is_empty() && image_name.is_some() {
            debug!(
                "Detected {} objects in image {:?}",
                predictions.len(),
                image_name
            );
        }

        if let Some(image_name) = image_name.clone() {
            if let Some(save_image_path) = self.save_image_path.clone() {
                let save_image_start_time = Instant::now();
                let save_image_path = save_image_path.to_path_buf();
                let image_name_od = create_od_image_name(&image_name, true)?;
                encode_maybe_draw_boundary_boxes_and_save_jpeg(
                    &self.decoded_image,
                    &save_image_path
                        .join(image_name_od)
                        .to_string_lossy()
                        .to_string(),
                    Some(predictions.as_slice()),
                )?;
                debug!("Save image time: {:?}", save_image_start_time.elapsed());
            }
        }

        Ok(DetectResult {
            predictions,
            processing_time,
            decode_image_time,
            resize_image_time,
            pre_processing_time,
            inference_time,
            post_processing_time,
            device_type: self.device_type,
            endpoint_provider: self.endpoint_provider,
        })
    }

    pub fn get_min_processing_time(&mut self) -> anyhow::Result<Duration> {
        const TUNE_RUNS: usize = 10;
        info!("Running detector {TUNE_RUNS} times to get min processing time");
        let mut min_processing_time = Duration::MAX;

        for _ in 0..TUNE_RUNS {
            let detector_warmup_start_time = Instant::now();
            self.detect(Bytes::from(crate::DOG_BIKE_CAR_BYTES), None, None)?;
            let processing_time = detector_warmup_start_time.elapsed();
            min_processing_time = min_processing_time.min(processing_time);
        }
        info!(
            ?min_processing_time,
            "Done running detector {TUNE_RUNS} times"
        );
        Ok(min_processing_time)
    }

    pub fn get_model_name(&self) -> &String {
        &self.model_name
    }

    pub fn get_endpoint_provider_name(&self) -> String {
        self.endpoint_provider.to_string()
    }

    pub fn is_using_gpu(&self) -> bool {
        self.device_type == DeviceType::GPU
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EndpointProvider {
    CPU,
    DirectML,
}

impl std::fmt::Display for EndpointProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointProvider::CPU => write!(f, "CPU"),
            EndpointProvider::DirectML => write!(f, "DirectML"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceType {
    CPU,
    GPU,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::CPU => write!(f, "CPU"),
            DeviceType::GPU => write!(f, "GPU"),
        }
    }
}
