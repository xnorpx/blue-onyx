use crate::{
    api::Prediction,
    direct_ml_available, get_object_classes,
    image::{
        Image, Resizer, create_od_image_name, decode_jpeg,
        encode_maybe_draw_boundary_boxes_and_save_jpeg,
    },
};
use anyhow::{anyhow, bail};
use bytes::Bytes;
use ndarray::{Array, ArrayView, Axis, s};
use ort::{
    execution_providers::DirectMLExecutionProvider,
    inputs,
    session::{Session, SessionInputs, SessionOutputs},
    value::Value,
};
use smallvec::SmallVec;
use std::{
    fmt::Debug,
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, info, warn};

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
    object_detection_model: ObjectDetectionModel,
}

#[derive(Debug, Clone)]
pub struct OnnxConfig {
    pub intra_threads: usize,
    pub inter_threads: usize,
    pub gpu_index: i32,
    pub force_cpu: bool,
    pub model: Option<PathBuf>,
}

#[derive(
    Debug, Clone, Default, PartialEq, clap::ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum ObjectDetectionModel {
    #[default]
    RtDetrv2,
    Yolo5,
}

impl std::fmt::Display for ObjectDetectionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectDetectionModel::RtDetrv2 => write!(f, "rt-detrv2"),
            ObjectDetectionModel::Yolo5 => write!(f, "yolo5"),
        }
    }
}

impl ObjectDetectionModel {
    pub fn pre_process<'a>(
        &self,
        input: &'a mut Array<f32, ndarray::Dim<[usize; 4]>>,
        orig_size: &'a Array<i64, ndarray::Dim<[usize; 2]>>,
    ) -> anyhow::Result<SessionInputs<'a, 'a>> {
        match self {
            Self::RtDetrv2 => rt_detrv2_pre_process(input, orig_size),
            Self::Yolo5 => yolo5_pre_process(input),
        }
    }
    pub fn post_process(
        &self,
        outputs: SessionOutputs<'_>,
        confidence_threshold: f32,
        resize_factor_x: f32,
        resize_factor_y: f32,
        object_filter: &Option<Vec<bool>>,
        object_classes: &[String],
    ) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
        match self {
            Self::RtDetrv2 => rt_detrv2_post_process(
                outputs,
                confidence_threshold,
                resize_factor_x,
                resize_factor_y,
                object_filter,
                object_classes,
            ),

            Self::Yolo5 => yolo5_post_process(
                outputs,
                confidence_threshold,
                resize_factor_x,
                resize_factor_y,
                object_filter,
                object_classes,
            ),
        }
    }
}

fn rt_detrv2_pre_process<'a>(
    input: &'a mut Array<f32, ndarray::Dim<[usize; 4]>>,
    orig_size: &'a Array<i64, ndarray::Dim<[usize; 2]>>,
) -> anyhow::Result<SessionInputs<'a, 'a>> {
    Ok(inputs![
        "images" => Value::from_array(input.clone())?,
        "orig_target_sizes" => Value::from_array(orig_size.clone())?,
    ]
    .into())
}

fn yolo5_pre_process<'a>(
    input: &'a mut Array<f32, ndarray::Dim<[usize; 4]>>,
) -> anyhow::Result<SessionInputs<'a, 'a>> {
    Ok(inputs![
        "images" => Value::from_array(input.clone())?,
    ]
    .into())
}

fn rt_detrv2_post_process(
    outputs: SessionOutputs<'_>,
    confidence_threshold: f32,
    resize_factor_x: f32,
    resize_factor_y: f32,
    object_filter: &Option<Vec<bool>>,
    object_classes: &[String],
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let (labels_shape, labels_data) = outputs["labels"].try_extract_tensor::<i64>()?;
    let (bboxes_shape, bboxes_data) = outputs["boxes"].try_extract_tensor::<f32>()?;
    let (scores_shape, scores_data) = outputs["scores"].try_extract_tensor::<f32>()?; // Convert shapes to ndarray dimensions
    let labels_dims: Vec<usize> = labels_shape.iter().map(|&dim| dim as usize).collect();
    let labels = ArrayView::from_shape(labels_dims.as_slice(), labels_data)
        .map_err(|e| anyhow!("Failed to create labels array view: {}", e))?;
    let bboxes_dims: Vec<usize> = bboxes_shape.iter().map(|&dim| dim as usize).collect();
    let bboxes = ArrayView::from_shape(bboxes_dims.as_slice(), bboxes_data)
        .map_err(|e| anyhow!("Failed to create bboxes array view: {}", e))?;
    let scores_dims: Vec<usize> = scores_shape.iter().map(|&dim| dim as usize).collect();
    let scores = ArrayView::from_shape(scores_dims.as_slice(), scores_data)
        .map_err(|e| anyhow!("Failed to create scores array view: {}", e))?;

    // Get the first batch (assuming batch size is 1)
    let labels = labels.index_axis(Axis(0), 0);
    let bboxes = bboxes.index_axis(Axis(0), 0);
    let scores = scores.index_axis(Axis(0), 0);
    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    for (i, bbox) in bboxes.outer_iter().enumerate() {
        if scores[i] > confidence_threshold {
            // If object filter is set, skip objects that are not in the filter
            if let Some(object_filter) = object_filter.as_ref()
                && !object_filter[labels[i] as usize]
            {
                continue;
            }

            let prediction = Prediction {
                x_min: (bbox[0] * resize_factor_x) as usize,
                x_max: (bbox[2] * resize_factor_x) as usize,
                y_min: (bbox[1] * resize_factor_y) as usize,
                y_max: (bbox[3] * resize_factor_y) as usize,
                confidence: scores[i],
                label: object_classes[labels[i] as usize].clone(),
            };

            debug!("Prediction - {}: {:?}", predictions.len() + 1, prediction);

            predictions.push(prediction);
        }
    }

    Ok(predictions)
}

fn yolo5_post_process(
    outputs: SessionOutputs<'_>,
    confidence_threshold: f32,
    resize_factor_x: f32,
    resize_factor_y: f32,
    object_filter: &Option<Vec<bool>>,
    object_classes: &[String],
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let output = outputs.values().next().ok_or(anyhow!("No outputs"))?;
    let (shape, data) = output.try_extract_tensor::<f32>()?;
    let shape_dims: Vec<usize> = shape.iter().map(|&dim| dim as usize).collect();
    let yolo_output = ArrayView::from_shape(shape_dims.as_slice(), data)
        .map_err(|e| anyhow!("Failed to create output array view: {}", e))?;

    // Debug: Print the actual tensor shape
    debug!("YOLO output tensor shape: {:?}", yolo_output.shape());
    debug!("Expected classes + 5: {}", 5 + object_classes.len());

    // The YOLO5 output is typically [batch_size, num_detections, num_classes + 5]
    // So we need to check the last dimension, not the second dimension
    let expected_features = 5 + object_classes.len();
    let actual_shape = yolo_output.shape();

    if actual_shape.len() == 3 && actual_shape[2] == expected_features {
        // Shape is [batch_size, num_detections, features] - this is correct
        debug!(
            "Tensor shape is correct: [batch_size={}, num_detections={}, features={}]",
            actual_shape[0], actual_shape[1], actual_shape[2]
        );
    } else if actual_shape.len() == 2 && actual_shape[1] == expected_features {
        // Shape is [num_detections, features] - also valid
        debug!(
            "Tensor shape is 2D: [num_detections={}, features={}]",
            actual_shape[0], actual_shape[1]
        );
    } else {
        bail!(
            "Unexpected YOLO output shape: {:?}. Expected last dimension to be {} (5 + {} classes). This probably means that your classes YAML file does not match the model.",
            actual_shape,
            expected_features,
            object_classes.len()
        );
    }
    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    // Handle different tensor shapes
    let detections_view = if yolo_output.shape().len() == 3 {
        // Shape is [batch_size, num_detections, features] - get the first batch
        yolo_output.index_axis(Axis(0), 0)
    } else {
        // Shape is [num_detections, features] - use directly
        yolo_output.view()
    };

    for iter in detections_view.outer_iter() {
        if iter[4] > confidence_threshold {
            let class_idx = iter
                .slice(s![5..])
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            if let Some(object_filter) = object_filter
                && !object_filter[class_idx]
            {
                continue;
            }

            let x_center = iter[0] * resize_factor_x;
            let y_center = iter[1] * resize_factor_y;
            let width = iter[2] * resize_factor_x;
            let height = iter[3] * resize_factor_y;
            let prediction = Prediction {
                x_min: (x_center - width / 2.0) as usize,
                y_min: (y_center - height / 2.0) as usize,
                x_max: (x_center + width / 2.0) as usize,
                y_max: (y_center + height / 2.0) as usize,
                confidence: iter[4],
                label: object_classes[class_idx].clone(),
            };
            predictions.push(prediction);
        }
    }

    // Apply non-maximum suppression (aka remove overlapping boxes)
    let predictions = non_maximum_suppression(predictions, 0.5)?;

    for (i, prediction) in predictions.iter().enumerate() {
        debug!("Prediction - {}: {:?}", i + 1, prediction);
    }

    Ok(predictions)
}

fn non_maximum_suppression(
    mut predictions: SmallVec<[Prediction; 10]>,
    iou_threshold: f32,
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let mut filtered_predictions = SmallVec::new();

    predictions.sort_by(|a, b| {
        a.label
            .cmp(&b.label)
            .then(b.confidence.partial_cmp(&a.confidence).unwrap())
    });

    let mut current_class = None;
    let mut kept: SmallVec<[Prediction; 10]> = SmallVec::new();

    for pred in predictions.iter() {
        if Some(&pred.label) != current_class {
            for kept_pred in kept.iter() {
                filtered_predictions.push(kept_pred.clone());
            }
            kept.clear();
            current_class = Some(&pred.label);
        }
        let mut should_keep = true;
        for kept_pred in kept.iter() {
            if calculate_iou(pred, kept_pred) >= iou_threshold {
                should_keep = false;
                break;
            }
        }

        if should_keep {
            kept.push(pred.clone());
        }
    }

    for kept_pred in kept.iter() {
        filtered_predictions.push(kept_pred.clone());
    }

    Ok(filtered_predictions)
}

fn calculate_iou(a: &Prediction, b: &Prediction) -> f32 {
    let x_min = a.x_min.max(b.x_min) as f32;
    let y_min = a.y_min.max(b.y_min) as f32;
    let x_max = a.x_max.min(b.x_max) as f32;
    let y_max = a.y_max.min(b.y_max) as f32;
    let intersection = (x_max - x_min).max(0.0) * (y_max - y_min).max(0.0);
    let area_a = (a.x_max - a.x_min) as f32 * (a.y_max - a.y_min) as f32;
    let area_b = (b.x_max - b.x_min) as f32 * (b.y_max - b.y_min) as f32;
    let union = area_a + area_b - intersection;

    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub object_classes: Option<PathBuf>,
    pub object_filter: Vec<String>,
    pub confidence_threshold: f32,
    pub save_image_path: Option<PathBuf>,
    pub save_ref_image: bool,
    pub timeout: Duration,
    pub object_detection_onnx_config: OnnxConfig,
    pub object_detection_model: ObjectDetectionModel,
}

impl Detector {
    pub fn new(detector_config: DetectorConfig) -> anyhow::Result<Self> {
        let (device_type, model_name, session, endpoint_provider, model_yaml_path) =
            initialize_onnx(&detector_config.object_detection_onnx_config)?; // Prioritize the YAML file that comes with the model over the configured one
        let yaml_path_to_use = model_yaml_path.or(detector_config.object_classes);

        let object_classes = if let Some(yaml_path) = &yaml_path_to_use {
            info!("Using object classes from model YAML: {:?}", yaml_path);
            get_object_classes(Some(yaml_path.clone()))?
        } else {
            bail!(
                "No YAML file found with model. A YAML file containing object classes is required for the model."
            );
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
            object_detection_model: detector_config.object_detection_model,
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
        // Save the image if save_ref_image is set
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

        // Process from here
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
        let copy_pixels_to_input_start = Instant::now();
        for (index, chunk) in self.resized_image.pixels.chunks_exact(3).enumerate() {
            let y = index / 640;
            let x = index % 640;
            self.input[[0, 0, y, x]] = chunk[0] as f32 / 255.0;
            self.input[[0, 1, y, x]] = chunk[1] as f32 / 255.0;
            self.input[[0, 2, y, x]] = chunk[2] as f32 / 255.0;
        }

        debug!(
            "Copy pixels to input time: {:?}",
            copy_pixels_to_input_start.elapsed()
        );
        let pre_process_model_input_start = Instant::now();
        let session_inputs = self
            .object_detection_model
            .pre_process(&mut self.input, &orig_size)?;

        debug!(
            "Pre-process model input time: {:?}",
            pre_process_model_input_start.elapsed()
        );

        let pre_processing_time = processing_time_start.elapsed();
        debug!("Pre-process time: {:?}", pre_processing_time);
        let start_inference_time = std::time::Instant::now();
        let outputs: SessionOutputs = self.session.run(session_inputs)?;
        let inference_time = start_inference_time.elapsed();
        debug!("Inference time: {:?}", inference_time);
        let post_processing_time_start = Instant::now();
        let confidence_threshold = min_confidence.unwrap_or(self.confidence_threshold);
        let predictions = self.object_detection_model.post_process(
            outputs,
            confidence_threshold,
            resize_factor_x,
            resize_factor_y,
            &self.object_filter,
            &self.object_classes,
        )?;

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

        if let Some(image_name) = image_name.clone()
            && let Some(save_image_path) = self.save_image_path.clone()
        {
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

fn initialize_onnx(
    onnx_config: &OnnxConfig,
) -> Result<
    (
        DeviceType,
        String,
        Session,
        EndpointProvider,
        Option<PathBuf>,
    ),
    anyhow::Error,
> {
    let mut providers = Vec::new();
    let mut device_type = DeviceType::CPU;

    let (num_intra_threads, num_inter_threads) = if onnx_config.force_cpu {
        let num_intra_threads = onnx_config.intra_threads.min(num_cpus::get_physical() - 1);
        let num_inter_threads = onnx_config.inter_threads.min(num_cpus::get_physical() - 1);
        info!(
            "Forcing CPU for inference with {} intra and {} inter threads",
            num_intra_threads, num_inter_threads
        );
        (num_intra_threads, num_inter_threads)
    } else if direct_ml_available() {
        info!(
            gpu_index = onnx_config.gpu_index,
            "DirectML available, using DirectML for inference"
        );
        providers.push(
            DirectMLExecutionProvider::default()
                .with_device_id(onnx_config.gpu_index)
                .build()
                .error_on_failure(),
        );

        device_type = DeviceType::GPU;
        (1, 1) // For GPU we just hardcode to 1 thread
    } else {
        let num_intra_threads = onnx_config.intra_threads.min(num_cpus::get_physical() - 1);
        let num_inter_threads = onnx_config.inter_threads.min(num_cpus::get_physical() - 1);
        warn!(
            "DirectML not available, falling back to CPU for inference with {} intra and {} inter threads",
            num_intra_threads, num_inter_threads
        );
        (onnx_config.intra_threads, onnx_config.inter_threads)
    };

    // Simple model and yaml file handling
    let model_filename = onnx_config
        .model
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let (model_path, yaml_path) = crate::ensure_model_files(model_filename)?;
    let model_bytes = std::fs::read(&model_path)?;
    let model_name = model_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    info!(
        "Initializing detector with model: {:?} and inference running on {}",
        model_name, device_type,
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
    Ok((
        device_type,
        model_name,
        session,
        endpoint_provider,
        Some(yaml_path),
    ))
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

/// Execution provider for the detector
#[derive(Debug, Clone)]
pub enum ExecutionProvider {
    CPU,
    DirectML(usize), // GPU index
}
