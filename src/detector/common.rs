use crate::{
    api::Prediction,
    get_object_classes,
    image::{Image, Resizer},
};
use anyhow::{Result, bail};
use ndarray::{Array, ArrayView1, ArrayView2, ArrayView3, Axis, s};
use smallvec::SmallVec;
use std::{fmt::Debug, path::PathBuf, time::Duration};

#[derive(
    Debug, Clone, Default, PartialEq, clap::ValueEnum, serde::Serialize, serde::Deserialize,
)]
pub enum ObjectDetectionModel {
    RtDetrv2,
    #[default]
    RfDetr,
    Yolo5,
}

impl std::fmt::Display for ObjectDetectionModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectDetectionModel::RtDetrv2 => write!(f, "rt-detrv2"),
            ObjectDetectionModel::RfDetr => write!(f, "rf-detr"),
            ObjectDetectionModel::Yolo5 => write!(f, "yolo5"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostProcessParams<'a> {
    pub confidence_threshold: f32,
    pub resize_factor_x: f32,
    pub resize_factor_y: f32,
    pub object_filter: &'a Option<Vec<bool>>,
    pub object_classes: &'a [String],
    pub input_width: u32,
    pub input_height: u32,
}

pub struct DetectResult {
    pub predictions: SmallVec<[Prediction; 10]>,
    pub processing_time: Duration,
    pub decode_image_time: Duration,
    pub resize_image_time: Duration,
    pub pre_processing_time: Duration,
    pub inference_time: Duration,
    pub post_processing_time: Duration,
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

#[derive(Debug, Clone)]
pub struct OnnxConfig {
    pub intra_threads: usize,
    pub inter_threads: usize,
    pub gpu_index: i32,
    pub force_cpu: bool,
    pub model: Option<PathBuf>,
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

#[derive(Debug, Clone)]
pub struct BackendInit {
    pub(crate) device_type: DeviceType,
    pub(crate) endpoint_provider: EndpointProvider,
    pub(crate) model_name: String,
    pub(crate) model_yaml_path: Option<PathBuf>,
    pub(crate) input_size: (usize, usize),
}

pub struct InferenceCommon {
    pub(crate) resizer: Resizer,
    pub(crate) decoded_image: Image,
    pub(crate) resized_image: Image,
    pub(crate) input: Array<f32, ndarray::Dim<[usize; 4]>>,
    pub(crate) object_classes: Vec<String>,
    pub(crate) object_filter: Option<Vec<bool>>,
    pub(crate) confidence_threshold: f32,
    pub(crate) save_image_path: Option<PathBuf>,
    pub(crate) save_ref_image: bool,
    pub(crate) input_width: usize,
    pub(crate) input_height: usize,
    pub(crate) model_name: String,
    pub(crate) device_type: DeviceType,
    pub(crate) endpoint_provider: EndpointProvider,
}

impl InferenceCommon {
    pub fn new(detector_config: &DetectorConfig, backend: BackendInit) -> Result<Self> {
        let (input_width, input_height) = backend.input_size;
        let yaml_path_to_use = backend
            .model_yaml_path
            .clone()
            .or(detector_config.object_classes.clone());

        let object_classes = if let Some(yaml_path) = yaml_path_to_use {
            get_object_classes(Some(yaml_path))?
        } else {
            bail!(
                "No YAML file found with model. A YAML file containing object classes is required for the model."
            );
        };

        let object_filter = if !detector_config.object_filter.is_empty() {
            let mut object_filter_vector = vec![false; object_classes.len()];
            for object in detector_config.object_filter.iter() {
                if let Some(index) = object_classes
                    .iter()
                    .position(|class_name| class_name.to_lowercase() == object.to_lowercase())
                {
                    object_filter_vector[index] = true;
                }
            }
            Some(object_filter_vector)
        } else {
            None
        };

        Ok(Self {
            resizer: Resizer::new(input_width, input_height)?,
            decoded_image: Image::default(),
            resized_image: Image::default(),
            input: Array::zeros((1, 3, input_height, input_width)),
            object_classes,
            object_filter,
            confidence_threshold: detector_config.confidence_threshold,
            save_image_path: detector_config.save_image_path.clone(),
            save_ref_image: detector_config.save_ref_image,
            input_width,
            input_height,
            model_name: backend.model_name,
            device_type: backend.device_type,
            endpoint_provider: backend.endpoint_provider,
        })
    }

    pub fn make_post_process_params<'a>(
        &'a self,
        confidence_threshold: f32,
        resize_factor_x: f32,
        resize_factor_y: f32,
    ) -> PostProcessParams<'a> {
        PostProcessParams {
            confidence_threshold,
            resize_factor_x,
            resize_factor_y,
            object_filter: &self.object_filter,
            object_classes: &self.object_classes,
            input_width: self.input_width as u32,
            input_height: self.input_height as u32,
        }
    }

    pub fn model_name(&self) -> &String {
        &self.model_name
    }

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn endpoint_provider(&self) -> EndpointProvider {
        self.endpoint_provider
    }

    pub fn set_execution_context(
        &mut self,
        device_type: DeviceType,
        endpoint_provider: EndpointProvider,
    ) {
        self.device_type = device_type;
        self.endpoint_provider = endpoint_provider;
    }

    pub fn input_size(&self) -> (usize, usize) {
        (self.input_width, self.input_height)
    }
}

pub fn non_maximum_suppression(
    mut predictions: SmallVec<[Prediction; 10]>,
    iou_threshold: f32,
) -> Result<SmallVec<[Prediction; 10]>> {
    let mut filtered_predictions = SmallVec::new();

    predictions.sort_by(|a, b| {
        a.label.cmp(&b.label).then(
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
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

pub fn calculate_iou(a: &Prediction, b: &Prediction) -> f32 {
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

pub fn rt_detrv2_post_process_from_arrays(
    labels: ArrayView1<'_, i64>,
    bboxes: ArrayView2<'_, f32>,
    scores: ArrayView1<'_, f32>,
    params: &PostProcessParams,
) -> Result<SmallVec<[Prediction; 10]>> {
    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    for (i, bbox) in bboxes.outer_iter().enumerate() {
        if scores[i] > params.confidence_threshold {
            if let Some(object_filter) = params.object_filter.as_ref()
                && !object_filter[labels[i] as usize]
            {
                continue;
            }

            let prediction = Prediction {
                x_min: (bbox[0] * params.resize_factor_x) as usize,
                x_max: (bbox[2] * params.resize_factor_x) as usize,
                y_min: (bbox[1] * params.resize_factor_y) as usize,
                y_max: (bbox[3] * params.resize_factor_y) as usize,
                confidence: scores[i],
                label: params.object_classes[labels[i] as usize].clone(),
            };

            predictions.push(prediction);
        }
    }

    Ok(predictions)
}

pub fn rf_detr_post_process_from_arrays(
    dets: ArrayView3<'_, f32>,
    labels: ArrayView3<'_, f32>,
    params: &PostProcessParams,
) -> Result<SmallVec<[Prediction; 10]>> {
    let dets = dets.index_axis(Axis(0), 0);
    let labels = labels.index_axis(Axis(0), 0);

    let num_queries = labels.shape()[0];
    let num_classes = labels.shape()[1];
    let total_predictions = num_queries * num_classes;

    let mut all_scores = Vec::with_capacity(total_predictions);
    for query_idx in 0..num_queries {
        for class_idx in 0..num_classes {
            let logit = labels[[query_idx, class_idx]];
            let prob = 1.0 / (1.0 + (-logit).exp());
            all_scores.push((prob, query_idx, class_idx));
        }
    }

    all_scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    for (score, query_idx, class_idx) in all_scores.iter().take(300) {
        if *score <= params.confidence_threshold {
            break;
        }

        if let Some(object_filter) = params.object_filter.as_ref()
            && *class_idx < object_filter.len()
            && !object_filter[*class_idx]
        {
            continue;
        }

        let det = dets.index_axis(Axis(0), *query_idx);

        let orig_img_width = params.resize_factor_x * params.input_width as f32;
        let orig_img_height = params.resize_factor_y * params.input_height as f32;

        let center_x = det[0] * orig_img_width;
        let center_y = det[1] * orig_img_height;
        let width = det[2] * orig_img_width;
        let height = det[3] * orig_img_height;

        let x_min = (center_x - width / 2.0).max(0.0);
        let x_max = center_x + width / 2.0;
        let y_min = (center_y - height / 2.0).max(0.0);
        let y_max = center_y + height / 2.0;

        let prediction = Prediction {
            x_min: x_min.round() as usize,
            x_max: x_max.round() as usize,
            y_min: y_min.round() as usize,
            y_max: y_max.round() as usize,
            confidence: *score,
            label: if *class_idx < params.object_classes.len() {
                params.object_classes[*class_idx].clone()
            } else {
                format!("class_{class_idx}")
            },
        };

        predictions.push(prediction);
    }

    Ok(predictions)
}

pub fn yolo5_post_process_from_arrays(
    detections_view: ArrayView2<'_, f32>,
    params: &PostProcessParams,
) -> Result<SmallVec<[Prediction; 10]>> {
    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    for iter in detections_view.outer_iter() {
        if iter[4] > params.confidence_threshold {
            let class_idx = iter
                .slice(s![5..])
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            if let Some(object_filter) = params.object_filter.as_ref()
                && !object_filter[class_idx]
            {
                continue;
            }

            let x_center = iter[0] * params.resize_factor_x;
            let y_center = iter[1] * params.resize_factor_y;
            let width = iter[2] * params.resize_factor_x;
            let height = iter[3] * params.resize_factor_y;
            let prediction = Prediction {
                x_min: (x_center - width / 2.0) as usize,
                y_min: (y_center - height / 2.0) as usize,
                x_max: (x_center + width / 2.0) as usize,
                y_max: (y_center + height / 2.0) as usize,
                confidence: iter[4],
                label: params.object_classes[class_idx].clone(),
            };
            predictions.push(prediction);
        }
    }

    non_maximum_suppression(predictions, 0.5)
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

#[derive(Debug, Clone, Copy)]
pub enum EndpointProvider {
    CPU,
    #[cfg(windows)]
    DirectML,
    #[cfg(windows)]
    WindowsMl,
}

impl std::fmt::Display for EndpointProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointProvider::CPU => write!(f, "CPU"),
            #[cfg(windows)]
            EndpointProvider::DirectML => write!(f, "DirectML"),
            #[cfg(windows)]
            EndpointProvider::WindowsMl => write!(f, "WindowsML"),
        }
    }
}

/// Execution provider for the detector
#[derive(Debug, Clone)]
pub enum ExecutionProvider {
    CPU,
    #[cfg(windows)]
    DirectML(usize), // GPU index
}
