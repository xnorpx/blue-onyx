#[cfg(windows)]
use crate::direct_ml_available;
use crate::{
    api::Prediction,
    get_object_classes,
    image::{
        Image, Resizer, create_od_image_name, decode_jpeg,
        encode_maybe_draw_boundary_boxes_and_save_jpeg,
    },
};
use anyhow::{anyhow, bail};
use bytes::Bytes;
use ndarray::{Array, ArrayView, Axis, s};
#[cfg(windows)]
use ort::execution_providers::DirectMLExecutionProvider;
use ort::{
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
    input_width: usize,
    input_height: usize,
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
pub struct PostProcessParams<'a> {
    pub confidence_threshold: f32,
    pub resize_factor_x: f32,
    pub resize_factor_y: f32,
    pub object_filter: &'a Option<Vec<bool>>,
    pub object_classes: &'a [String],
    pub input_width: u32,
    pub input_height: u32,
}

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

impl ObjectDetectionModel {
    pub fn pre_process<'a>(
        &self,
        input: &'a mut Array<f32, ndarray::Dim<[usize; 4]>>,
        orig_size: &'a Array<i64, ndarray::Dim<[usize; 2]>>,
    ) -> anyhow::Result<SessionInputs<'a, 'a>> {
        match self {
            Self::RtDetrv2 => rt_detrv2_pre_process(input, orig_size),
            Self::RfDetr => rf_detr_pre_process(input, orig_size),
            Self::Yolo5 => yolo5_pre_process(input),
        }
    }
    pub fn post_process(
        &self,
        outputs: SessionOutputs<'_>,
        params: &PostProcessParams,
    ) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
        match self {
            Self::RtDetrv2 => rt_detrv2_post_process(
                outputs,
                params.confidence_threshold,
                params.resize_factor_x,
                params.resize_factor_y,
                params.object_filter,
                params.object_classes,
            ),

            Self::RfDetr => rf_detr_post_process(outputs, params),

            Self::Yolo5 => yolo5_post_process(
                outputs,
                params.confidence_threshold,
                params.resize_factor_x,
                params.resize_factor_y,
                params.object_filter,
                params.object_classes,
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

fn rf_detr_pre_process<'a>(
    input: &'a mut Array<f32, ndarray::Dim<[usize; 4]>>,
    _orig_size: &'a Array<i64, ndarray::Dim<[usize; 2]>>,
) -> anyhow::Result<SessionInputs<'a, 'a>> {
    Ok(inputs![
        "input" => Value::from_array(input.clone())?,
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

fn rf_detr_post_process(
    outputs: SessionOutputs<'_>,
    params: &PostProcessParams,
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let (dets_shape, dets_data) = outputs["dets"].try_extract_tensor::<f32>()?;
    let (labels_shape, labels_data) = outputs["labels"].try_extract_tensor::<f32>()?;

    // Convert shapes to ndarray dimensions
    let dets_dims: Vec<usize> = dets_shape.iter().map(|&dim| dim as usize).collect();
    let dets = ArrayView::from_shape(dets_dims.as_slice(), dets_data)
        .map_err(|e| anyhow!("Failed to create dets array view: {}", e))?;
    let labels_dims: Vec<usize> = labels_shape.iter().map(|&dim| dim as usize).collect();
    let labels = ArrayView::from_shape(labels_dims.as_slice(), labels_data)
        .map_err(|e| anyhow!("Failed to create labels array view: {}", e))?;

    // Get the first batch (assuming batch size is 1)
    let dets = dets.index_axis(Axis(0), 0); // Shape: [num_queries, 4] - boxes in cxcywh format
    let labels = labels.index_axis(Axis(0), 0); // Shape: [num_queries, num_classes] - logits

    // Apply sigmoid and flatten to find top-k predictions across all queries and classes
    let num_queries = labels.shape()[0];
    let num_classes = labels.shape()[1];
    let total_predictions = num_queries * num_classes;

    // Apply sigmoid to convert logits to probabilities and collect all scores
    let mut all_scores = Vec::with_capacity(total_predictions);
    for query_idx in 0..num_queries {
        for class_idx in 0..num_classes {
            let logit = labels[[query_idx, class_idx]];
            let prob = 1.0 / (1.0 + (-logit).exp()); // sigmoid
            all_scores.push((prob, query_idx, class_idx));
        }
    }

    // Sort by score (descending) and take top predictions
    all_scores.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut predictions = SmallVec::<[Prediction; 10]>::new();

    // Process top predictions above confidence threshold
    for (score, query_idx, class_idx) in all_scores.iter().take(300) {
        if *score <= params.confidence_threshold {
            break;
        }

        // If object filter is set, skip objects that are not in the filter
        if let Some(object_filter) = params.object_filter.as_ref()
            && *class_idx < object_filter.len()
            && !object_filter[*class_idx]
        {
            continue;
        }

        // Extract bounding box coordinates in cxcywh format (normalized 0-1)
        let det = dets.index_axis(Axis(0), *query_idx);

        // RF-DETR outputs normalized coordinates (0-1), scale to original image dimensions
        let orig_img_width = params.resize_factor_x * params.input_width as f32;
        let orig_img_height = params.resize_factor_y * params.input_height as f32;

        let center_x = det[0] * orig_img_width;
        let center_y = det[1] * orig_img_height;
        let width = det[2] * orig_img_width;
        let height = det[3] * orig_img_height;

        // Convert from center_x, center_y, width, height to x_min, y_min, x_max, y_max
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

        debug!(
            "RF-DETR Detection - {}: {:?}",
            predictions.len() + 1,
            prediction
        );
        debug!(
            "  Query {}, Class {}: {:.4} -> {:.4} (COCO ID {})",
            query_idx,
            class_idx,
            labels[[*query_idx, *class_idx]],
            score,
            class_idx
        );
        debug!(
            "  Raw bbox: center=({:.4}, {:.4}), size=({:.4}, {:.4})",
            det[0], det[1], det[2], det[3]
        );
        debug!(
            "  Model input: {}x{}, Original image: {:.0}x{:.0}",
            params.input_width, params.input_height, orig_img_width, orig_img_height
        );
        debug!(
            "  Scaled bbox: center=({:.2}, {:.2}), size=({:.2}, {:.2})",
            center_x, center_y, width, height
        );
        debug!(
            "  Float coords: ({:.2}, {:.2}) to ({:.2}, {:.2})",
            x_min, y_min, x_max, y_max
        );
        debug!(
            "  Final coords: ({}, {}) to ({}, {})",
            prediction.x_min, prediction.y_min, prediction.x_max, prediction.y_max
        );

        predictions.push(prediction);
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
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
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

fn query_image_input_size(session: &Session) -> anyhow::Result<(usize, usize)> {
    let inputs = &session.inputs;

    info!("Model inputs:");
    for (i, input) in inputs.iter().enumerate() {
        info!(
            "  Input {}: name='{}', type={:?}",
            i, input.name, input.input_type
        );
    }

    // Try to extract dimensions from the input type information
    for input in inputs.iter() {
        // Look for image input (typically named "input" or "images")
        if input.name == "input" || input.name == "images" {
            // Parse the input type string to extract dimensions
            let type_str = format!("{:?}", input.input_type);

            // Look for shape pattern like "shape: [1, 3, 384, 384]"
            if let Some(shape_start) = type_str.find("shape: [") {
                let shape_part = &type_str[shape_start + 8..];
                if let Some(shape_end) = shape_part.find(']') {
                    let shape_str = &shape_part[..shape_end];
                    let dims: Vec<&str> = shape_str.split(',').map(|s| s.trim()).collect();

                    // Expect format [batch_size, channels, height, width]
                    if dims.len() == 4 {
                        if let (Ok(height), Ok(width)) =
                            (dims[2].parse::<usize>(), dims[3].parse::<usize>())
                        {
                            info!(
                                "Extracted input size from model '{}': {}x{}",
                                input.name, width, height
                            );
                            return Ok((width, height));
                        }
                    }
                }
            }

            // Fallback: use heuristic based on input name
            if input.name == "input" {
                info!("Could not parse dimensions, using RF-DETR default: 384x384");
                return Ok((384, 384));
            } else if input.name == "images" {
                info!("Could not parse dimensions, using RT-DETR/YOLO default: 640x640");
                return Ok((640, 640));
            }
        }
    }

    // Fallback to 640x640 if we can't detect the size
    warn!("Could not detect input size from model, falling back to 640x640");
    Ok((640, 640))
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
        let (device_type, model_name, session, endpoint_provider, model_yaml_path, (width, height)) =
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
            resizer: Resizer::new(width, height)?,
            decoded_image: Image::default(),
            resized_image: Image::default(),
            input: Array::zeros((1, 3, height, width)),
            object_classes,
            object_filter,
            confidence_threshold: detector_config.confidence_threshold,
            device_type,
            save_image_path: detector_config.save_image_path,
            save_ref_image: detector_config.save_ref_image,
            object_detection_model: detector_config.object_detection_model,
            input_width: width,
            input_height: height,
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
        if let Some(ref image_name_str) = image_name {
            debug!("Detecting objects in image: {}", image_name_str);
            if let Some(ref save_image_path) = self.save_image_path {
                if self.save_ref_image {
                    let save_image_path = save_image_path.to_path_buf();
                    let image_path_buf = PathBuf::from(image_name_str);
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

        let resize_factor_x = self.decoded_image.width as f32 / self.input_width as f32;
        let resize_factor_y = self.decoded_image.height as f32 / self.input_height as f32;
        debug!(
            "Image resize factors: width_factor={:.3} ({}->{}), height_factor={:.3} ({}->{})",
            resize_factor_x,
            self.input_width,
            self.decoded_image.width,
            resize_factor_y,
            self.input_height,
            self.decoded_image.height
        );

        let orig_size = Array::from_shape_vec(
            (1, 2),
            vec![self.input_height as i64, self.input_width as i64],
        )?;
        let resize_image_start_time = Instant::now();
        self.resizer
            .resize_image(&mut self.decoded_image, &mut self.resized_image)?;
        let resize_image_time = resize_image_start_time.elapsed();
        debug!("Resize image time: {:#?}", resize_image_time);

        // Ensure resized image dimensions match input tensor dimensions
        if self.resized_image.width != self.input_width
            || self.resized_image.height != self.input_height
        {
            bail!(
                "Resized image dimensions ({}x{}) don't match input tensor dimensions ({}x{})",
                self.resized_image.width,
                self.resized_image.height,
                self.input_width,
                self.input_height
            );
        }

        debug!(
            "Resized image dimensions: {}x{}, Input tensor dimensions: {}x{}",
            self.resized_image.width,
            self.resized_image.height,
            self.input_width,
            self.input_height
        );

        let copy_pixels_to_input_start = Instant::now();
        let expected_pixels = self.input_width * self.input_height;
        let actual_pixels = self.resized_image.pixels.len() / 3; // RGB channels

        debug!(
            "Expected pixels: {}, Actual pixels: {}",
            expected_pixels, actual_pixels
        );
        debug!(
            "Input tensor shape: [1, 3, {}, {}]",
            self.input_height, self.input_width
        );

        if actual_pixels != expected_pixels {
            bail!(
                "Pixel count mismatch: expected {} pixels but got {} pixels",
                expected_pixels,
                actual_pixels
            );
        }

        for (index, chunk) in self.resized_image.pixels.chunks_exact(3).enumerate() {
            let y = index / self.input_width;
            let x = index % self.input_width;

            // Check bounds before accessing
            if y >= self.input_height || x >= self.input_width {
                bail!(
                    "Index out of bounds: trying to access ({}, {}) but tensor is {}x{}",
                    x,
                    y,
                    self.input_width,
                    self.input_height
                );
            }

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
        let params = PostProcessParams {
            confidence_threshold,
            resize_factor_x,
            resize_factor_y,
            object_filter: &self.object_filter,
            object_classes: &self.object_classes,
            input_width: self.input_width as u32,
            input_height: self.input_height as u32,
        };
        let predictions = self.object_detection_model.post_process(outputs, &params)?;

        let now = Instant::now();
        let post_processing_time = now.duration_since(post_processing_time_start);
        debug!("Post-processing time: {:?}", post_processing_time);
        let processing_time = now.duration_since(processing_time_start);

        // Processing time is mainly composed of:
        //  1. Image decoding time
        //  2. Image resizing time
        //  3. Inference time

        debug!("Processing time: {:?}", processing_time);

        if let Some(ref image_name) = image_name
            && let Some(ref save_image_path) = self.save_image_path
        {
            info!(
                "Saving detection result with {} predictions to disk",
                predictions.len()
            );
            let save_image_start_time = Instant::now();
            let save_image_path = save_image_path.to_path_buf();
            let image_name_od = create_od_image_name(&image_name, true)?;
            let output_path = save_image_path
                .join(&image_name_od)
                .to_string_lossy()
                .to_string();
            info!("Output path: {}", output_path);

            encode_maybe_draw_boundary_boxes_and_save_jpeg(
                &self.decoded_image,
                &output_path,
                Some(predictions.as_slice()),
                self.input_width as u32,
                self.input_height as u32,
            )?;
            debug!("Save image time: {:?}", save_image_start_time.elapsed());
        } else {
            if image_name.is_none() {
                debug!("No image name provided, skipping image save");
            }
            if self.save_image_path.is_none() {
                debug!("No save path configured, skipping image save");
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

    pub fn get_input_size(&self) -> (usize, usize) {
        (self.input_width, self.input_height)
    }
}

type InitializeOnnxResult = Result<
    (
        DeviceType,
        String,
        Session,
        EndpointProvider,
        Option<PathBuf>,
        (usize, usize), // (width, height)
    ),
    anyhow::Error,
>;

fn initialize_onnx(onnx_config: &OnnxConfig) -> InitializeOnnxResult {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut providers = Vec::new();
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut device_type = DeviceType::CPU;

    let (num_intra_threads, num_inter_threads) = if onnx_config.force_cpu {
        let num_intra_threads = onnx_config.intra_threads.min(num_cpus::get_physical() - 1);
        let num_inter_threads = onnx_config.inter_threads.min(num_cpus::get_physical() - 1);
        info!(
            "Forcing CPU for inference with {} intra and {} inter threads",
            num_intra_threads, num_inter_threads
        );
        // When forcing CPU, ensure no other providers are used
        // providers list will remain empty, which means CPU provider is used by default
        (num_intra_threads, num_inter_threads)
    } else {
        #[cfg(windows)]
        if direct_ml_available() {
            info!(
                gpu_index = onnx_config.gpu_index,
                "DirectML available, attempting to use DirectML for inference"
            );

            // Try to initialize DirectML provider, but handle any errors
            let provider = DirectMLExecutionProvider::default()
                .with_device_id(onnx_config.gpu_index)
                .build();
            providers.push(provider);
            device_type = DeviceType::GPU;
            info!("DirectML initialization successful");
            (1, 1) // For GPU we just hardcode to 1 thread
        } else {
            let num_intra_threads = onnx_config.intra_threads.min(num_cpus::get_physical() - 1);
            let num_inter_threads = onnx_config.inter_threads.min(num_cpus::get_physical() - 1);
            #[cfg(windows)]
            warn!(
                "DirectML not available, falling back to CPU for inference with {} intra and {} inter threads",
                num_intra_threads, num_inter_threads
            );
            #[cfg(not(windows))]
            warn!(
                "GPU acceleration not available on this platform, using CPU for inference with {} intra and {} inter threads",
                num_intra_threads, num_inter_threads
            );
            (onnx_config.intra_threads, onnx_config.inter_threads)
        }

        #[cfg(not(windows))]
        {
            let num_intra_threads = onnx_config.intra_threads.min(num_cpus::get_physical() - 1);
            let num_inter_threads = onnx_config.inter_threads.min(num_cpus::get_physical() - 1);
            warn!(
                "GPU acceleration not available on this platform, using CPU for inference with {} intra and {} inter threads",
                num_intra_threads, num_inter_threads
            );
            (onnx_config.intra_threads, onnx_config.inter_threads)
        }
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

    // Build the session with the appropriate execution providers
    // Note: When providers list is empty (which is the case when force_cpu=true),
    // ONNX Runtime will default to CPU execution provider
    let session = Session::builder()?
        .with_execution_providers(providers)?
        .with_intra_threads(num_intra_threads)?
        .with_inter_threads(num_inter_threads)?
        .commit_from_memory(model_bytes.as_slice())?;

    // Query the input size from the model
    let (width, height) = query_image_input_size(&session)?;

    info!(
        "Model '{}' configured with input size: {}x{} ({}x{} tensor)",
        model_name, width, height, height, width
    );

    let endpoint_provider = match device_type {
        #[cfg(windows)]
        DeviceType::GPU => EndpointProvider::DirectML,
        _ => EndpointProvider::CPU,
    };
    Ok((
        device_type,
        model_name,
        session,
        endpoint_provider,
        Some(yaml_path),
        (width, height),
    ))
}

#[derive(Debug, Clone, Copy)]
pub enum EndpointProvider {
    CPU,
    #[cfg(windows)]
    DirectML,
}

impl std::fmt::Display for EndpointProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndpointProvider::CPU => write!(f, "CPU"),
            #[cfg(windows)]
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
    #[cfg(windows)]
    DirectML(usize), // GPU index
}
