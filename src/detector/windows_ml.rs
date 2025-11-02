use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use bytes::Bytes;
use ndarray::{ArrayView1, ArrayView2, ArrayView3};
use smallvec::SmallVec;
use tracing::{debug, info, warn};
use windows::AI::MachineLearning::{
    ILearningModelFeatureDescriptor, ImageFeatureDescriptor, LearningModel, LearningModelBinding,
    LearningModelDevice, LearningModelDeviceKind, LearningModelEvaluationResult,
    LearningModelFeatureKind, LearningModelSession, TensorFeatureDescriptor, TensorFloat,
    TensorInt64Bit, TensorKind,
};
use windows::core::{HSTRING, Interface};

use crate::api::Prediction;

use crate::image::{
    create_od_image_name, decode_jpeg, encode_maybe_draw_boundary_boxes_and_save_jpeg,
};

use super::common::*;

#[derive(Clone)]
struct ModelBindingNames {
    image_input: HSTRING,
    orig_target_sizes: Option<HSTRING>,
    outputs: ModelOutputNames,
}

#[derive(Clone)]
enum ModelOutputNames {
    RtDetr {
        boxes: HSTRING,
        labels: HSTRING,
        scores: HSTRING,
    },
    RfDetr {
        dets: HSTRING,
        logits: HSTRING,
    },
    Yolo5 {
        detections: HSTRING,
    },
}

pub struct Inference {
    session: LearningModelSession,
    common: InferenceCommon,
    bindings: ModelBindingNames,
}

impl Inference {
    pub fn new(detector_config: DetectorConfig) -> Result<Self> {
        let (backend, session, bindings) =
            initialize_windows_ml(&detector_config).context("initialize Windows ML backend")?;
        let common = InferenceCommon::new(&detector_config, backend)?;

        info!("Windows ML backend initialized; warmup will occur on first detect() call");

        Ok(Self {
            session,
            common,
            bindings,
        })
    }

    pub fn detect(
        &mut self,
        image_bytes: Bytes,
        image_name: Option<String>,
        min_confidence: Option<f32>,
    ) -> Result<DetectResult> {
        let common = &mut self.common;

        if let Some(ref image_name_str) = image_name {
            debug!("Detecting objects in image: {}", image_name_str);
            if let Some(ref save_image_path) = common.save_image_path
                && common.save_ref_image
            {
                let save_image_path = save_image_path.to_path_buf();
                let image_path_buf = PathBuf::from(image_name_str);
                let image_name_ref = image_path_buf
                    .file_name()
                    .ok_or_else(|| anyhow!("Failed to get file name from path"))?;
                let save_image_path = save_image_path.join(image_name_ref);
                std::fs::write(save_image_path, &image_bytes)?;
            }
        }

        let processing_time_start = Instant::now();
        decode_jpeg(image_name.clone(), image_bytes, &mut common.decoded_image)?;
        let decode_image_time = processing_time_start.elapsed();

        debug!(
            "Decode image time: {:?}, resolution {}x{}",
            decode_image_time, common.decoded_image.width, common.decoded_image.height
        );

        let resize_factor_x = common.decoded_image.width as f32 / common.input_width as f32;
        let resize_factor_y = common.decoded_image.height as f32 / common.input_height as f32;
        debug!(
            "Image resize factors: width_factor={:.3} ({}->{}), height_factor={:.3} ({}->{})",
            resize_factor_x,
            common.input_width,
            common.decoded_image.width,
            resize_factor_y,
            common.input_height,
            common.decoded_image.height
        );

        let resize_image_start_time = Instant::now();
        common
            .resizer
            .resize_image(&mut common.decoded_image, &mut common.resized_image)?;
        let resize_image_time = resize_image_start_time.elapsed();
        debug!("Resize image time: {:#?}", resize_image_time);

        if common.resized_image.width != common.input_width
            || common.resized_image.height != common.input_height
        {
            bail!(
                "Resized image dimensions ({}x{}) don't match input tensor dimensions ({}x{})",
                common.resized_image.width,
                common.resized_image.height,
                common.input_width,
                common.input_height
            );
        }

        debug!(
            "Resized image dimensions: {}x{}, Input tensor dimensions: {}x{}",
            common.resized_image.width,
            common.resized_image.height,
            common.input_width,
            common.input_height
        );

        let copy_pixels_to_input_start = Instant::now();
        let expected_pixels = common.input_width * common.input_height;
        let actual_pixels = common.resized_image.pixels.len() / 3;

        debug!(
            "Expected pixels: {}, Actual pixels: {}",
            expected_pixels, actual_pixels
        );
        debug!(
            "Input tensor shape: [1, 3, {}, {}]",
            common.input_height, common.input_width
        );

        if actual_pixels != expected_pixels {
            bail!(
                "Pixel count mismatch: expected {} pixels but got {} pixels",
                expected_pixels,
                actual_pixels
            );
        }

        for (index, chunk) in common.resized_image.pixels.chunks_exact(3).enumerate() {
            let y = index / common.input_width;
            let x = index % common.input_width;

            if y >= common.input_height || x >= common.input_width {
                bail!(
                    "Index out of bounds: trying to access ({}, {}) but tensor is {}x{}",
                    x,
                    y,
                    common.input_width,
                    common.input_height
                );
            }

            common.input[[0, 0, y, x]] = chunk[0] as f32 / 255.0;
            common.input[[0, 1, y, x]] = chunk[1] as f32 / 255.0;
            common.input[[0, 2, y, x]] = chunk[2] as f32 / 255.0;
        }

        debug!(
            "Copy pixels to input time: {:?}",
            copy_pixels_to_input_start.elapsed()
        );

        let input_slice = common
            .input
            .as_slice()
            .ok_or_else(|| anyhow!("Input tensor is not contiguous"))?;
        let input_shape = [
            1_i64,
            3,
            common.input_height as i64,
            common.input_width as i64,
        ];
        let pre_processing_time = processing_time_start.elapsed();
        debug!("Pre-process time: {:?}", pre_processing_time);

        let start_inference_time = Instant::now();
        let input_tensor = TensorFloat::CreateFromShapeArrayAndDataArray(&input_shape, input_slice)
            .map_err(|err| {
                anyhow!("TensorFloat::CreateFromShapeArrayAndDataArray failed: {err:?}")
            })?;

        let binding = LearningModelBinding::CreateFromSession(&self.session)
            .map_err(|err| anyhow!("LearningModelBinding::CreateFromSession failed: {err:?}"))?;
        binding
            .Bind(&self.bindings.image_input, &input_tensor)
            .map_err(|err| anyhow!("Binding image input failed: {err:?}"))?;

        if let Some(orig_name) = &self.bindings.orig_target_sizes {
            let orig_data = [common.input_height as i64, common.input_width as i64];
            let orig_tensor = TensorInt64Bit::CreateFromShapeArrayAndDataArray(&[1, 2], &orig_data)
                .map_err(|err| {
                    anyhow!("TensorInt64Bit::CreateFromShapeArrayAndDataArray failed: {err:?}")
                })?;
            binding
                .Bind(orig_name, &orig_tensor)
                .map_err(|err| anyhow!("Binding orig_target_sizes failed: {err:?}"))?;
        }

        let evaluation = self
            .session
            .Evaluate(&binding, &HSTRING::new())
            .map_err(|err| anyhow!("LearningModelSession::Evaluate failed: {err:?}"))?;
        let inference_time = start_inference_time.elapsed();
        debug!("Inference time: {:?}", inference_time);

        let post_processing_time_start = Instant::now();
        let confidence_threshold = min_confidence.unwrap_or(common.confidence_threshold);
        let params =
            common.make_post_process_params(confidence_threshold, resize_factor_x, resize_factor_y);

        let predictions = parse_predictions(&self.bindings.outputs, &evaluation, &params)?;

        let now = Instant::now();
        let post_processing_time = now.duration_since(post_processing_time_start);
        debug!("Post-processing time: {:?}", post_processing_time);
        let processing_time = now.duration_since(processing_time_start);
        debug!("Processing time: {:?}", processing_time);

        if let Some(ref image_name) = image_name
            && let Some(ref save_image_path) = common.save_image_path
        {
            info!(
                "Saving detection result with {} predictions to disk",
                predictions.len()
            );
            let save_image_start_time = Instant::now();
            let save_image_path = save_image_path.to_path_buf();
            let image_name_od = create_od_image_name(image_name, true)?;
            let output_path = save_image_path
                .join(&image_name_od)
                .to_string_lossy()
                .to_string();
            info!("Output path: {}", output_path);

            encode_maybe_draw_boundary_boxes_and_save_jpeg(
                &common.decoded_image,
                &output_path,
                Some(predictions.as_slice()),
                common.input_width as u32,
                common.input_height as u32,
            )?;
            debug!("Save image time: {:?}", save_image_start_time.elapsed());
        } else {
            if image_name.is_none() {
                debug!("No image name provided, skipping image save");
            }
            if common.save_image_path.is_none() {
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
            device_type: common.device_type,
            endpoint_provider: common.endpoint_provider,
        })
    }

    pub fn get_min_processing_time(&mut self) -> Result<std::time::Duration> {
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
        self.common.model_name()
    }

    pub fn get_endpoint_provider_name(&self) -> String {
        self.common.endpoint_provider().to_string()
    }

    pub fn is_using_gpu(&self) -> bool {
        self.common.device_type() == DeviceType::GPU
    }

    pub fn get_input_size(&self) -> (usize, usize) {
        self.common.input_size()
    }
}

fn initialize_windows_ml(
    detector_config: &DetectorConfig,
) -> Result<(BackendInit, LearningModelSession, ModelBindingNames)> {
    let (model_path, model_yaml_path) = resolve_model_files(detector_config)?;
    let model_name = model_path
        .file_name()
        .and_then(|os| os.to_str())
        .unwrap_or("unknown")
        .to_string();

    info!("Loading Windows ML model: {}", model_name);
    let model = LearningModel::LoadFromFilePath(&path_to_hstring(&model_path)?)
        .map_err(|err| anyhow::anyhow!("LearningModel::LoadFromFilePath failed: {err:?}"))?;

    let (device, device_type, endpoint_provider) =
        create_device(detector_config).context("create Windows ML device")?;
    let session =
        LearningModelSession::CreateFromModelOnDevice(&model, &device).map_err(|err| {
            anyhow::anyhow!("LearningModelSession::CreateFromModelOnDevice failed: {err:?}")
        })?;

    let (bindings, input_size) =
        discover_model_bindings(&model, &detector_config.object_detection_model)?;
    info!(
        input_width = input_size.0,
        input_height = input_size.1,
        "Model '{}' configured with input size: {}x{}",
        model_name,
        input_size.0,
        input_size.1
    );

    let backend = BackendInit {
        device_type,
        endpoint_provider,
        model_name,
        model_yaml_path,
        input_size,
    };

    Ok((backend, session, bindings))
}

fn create_device(
    detector_config: &DetectorConfig,
) -> Result<(LearningModelDevice, DeviceType, EndpointProvider)> {
    if detector_config.object_detection_onnx_config.force_cpu {
        let device = LearningModelDevice::Create(LearningModelDeviceKind::Cpu)
            .map_err(|err| anyhow::anyhow!("LearningModelDevice::Create(Cpu) failed: {err:?}"))?;
        return Ok((device, DeviceType::CPU, EndpointProvider::CPU));
    }

    match LearningModelDevice::Create(LearningModelDeviceKind::DirectX) {
        Ok(device) => Ok((device, DeviceType::GPU, EndpointProvider::WindowsMl)),
        Err(err) => {
            warn!(?err, "DirectX device creation failed, falling back to CPU");
            let device =
                LearningModelDevice::Create(LearningModelDeviceKind::Cpu).map_err(|cpu_err| {
                    anyhow::anyhow!(
                        "LearningModelDevice::Create(Cpu) failed after GPU fallback: {cpu_err:?}"
                    )
                })?;
            Ok((device, DeviceType::CPU, EndpointProvider::CPU))
        }
    }
}

fn resolve_model_files(detector_config: &DetectorConfig) -> Result<(PathBuf, Option<PathBuf>)> {
    let model_filename = detector_config
        .object_detection_onnx_config
        .model
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.to_string());

    let (model_path, yaml_path) =
        crate::ensure_model_files(model_filename).context("ensure Windows ML model files")?;
    Ok((model_path, Some(yaml_path)))
}

fn discover_model_bindings(
    model: &LearningModel,
    object_model: &ObjectDetectionModel,
) -> Result<(ModelBindingNames, (usize, usize))> {
    let input_features = model
        .InputFeatures()
        .map_err(|err| anyhow!("LearningModel::InputFeatures failed: {err:?}"))?;
    let input_feature_count = input_features
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size failed: {err:?}"))?;

    let mut image_input: Option<HSTRING> = None;
    let mut input_size: Option<(usize, usize)> = None;
    let mut orig_target_sizes: Option<HSTRING> = None;

    for index in 0..input_feature_count {
        let feature: ILearningModelFeatureDescriptor = input_features
            .GetAt(index)
            .map_err(|err| anyhow!("IVectorView::GetAt failed: {err:?}"))?;

        match feature
            .Kind()
            .map_err(|err| anyhow!("LearningModelFeatureDescriptor::Kind failed: {err:?}"))?
        {
            LearningModelFeatureKind::Image => {
                let descriptor: ImageFeatureDescriptor = feature.cast().map_err(|err| {
                    anyhow!("Failed to cast feature to ImageFeatureDescriptor: {err:?}")
                })?;
                let width = descriptor
                    .Width()
                    .map_err(|err| anyhow!("ImageFeatureDescriptor::Width failed: {err:?}"))?
                    as usize;
                let height = descriptor
                    .Height()
                    .map_err(|err| anyhow!("ImageFeatureDescriptor::Height failed: {err:?}"))?
                    as usize;
                let name = descriptor
                    .Name()
                    .map_err(|err| anyhow!("ImageFeatureDescriptor::Name failed: {err:?}"))?;
                image_input = Some(name);
                input_size = Some((width, height));
            }
            LearningModelFeatureKind::Tensor => {
                let descriptor: TensorFeatureDescriptor = feature.cast().map_err(|err| {
                    anyhow!("Failed to cast feature to TensorFeatureDescriptor: {err:?}")
                })?;
                let name = descriptor
                    .Name()
                    .map_err(|err| anyhow!("TensorFeatureDescriptor::Name failed: {err:?}"))?;
                let tensor_kind = descriptor.TensorKind().map_err(|err| {
                    anyhow!("TensorFeatureDescriptor::TensorKind failed: {err:?}")
                })?;
                let name_string = name.to_string();
                let shape_vec = tensor_descriptor_shape(&descriptor, &name_string)?;

                if image_input.is_none()
                    && tensor_kind == TensorKind::Float
                    && let Some((width, height)) = extract_hw_from_shape(&shape_vec)
                {
                    image_input = Some(name.clone());
                    input_size = Some((width, height));
                }

                if orig_target_sizes.is_none()
                    && tensor_kind == TensorKind::Int64
                    && shape_vec.as_slice() == [1, 2]
                {
                    orig_target_sizes = Some(name);
                }
            }
            _ => continue,
        }
    }

    if image_input.is_none() {
        warn!("Falling back to default input binding name 'input'");
    }
    let image_input = image_input.unwrap_or_else(|| HSTRING::from("input"));
    let input_size = input_size.unwrap_or((640, 640));

    let outputs = model
        .OutputFeatures()
        .map_err(|err| anyhow!("LearningModel::OutputFeatures failed: {err:?}"))?;
    let output_feature_count = outputs
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size failed: {err:?}"))?;

    let mut candidates = Vec::new();
    for index in 0..output_feature_count {
        let feature: ILearningModelFeatureDescriptor = outputs
            .GetAt(index)
            .map_err(|err| anyhow!("IVectorView::GetAt failed: {err:?}"))?;

        if feature
            .Kind()
            .map_err(|err| anyhow!("LearningModelFeatureDescriptor::Kind failed: {err:?}"))?
            != LearningModelFeatureKind::Tensor
        {
            continue;
        }

        let descriptor: TensorFeatureDescriptor = feature
            .cast()
            .map_err(|err| anyhow!("Failed to cast feature to TensorFeatureDescriptor: {err:?}"))?;
        let name = descriptor
            .Name()
            .map_err(|err| anyhow!("TensorFeatureDescriptor::Name failed: {err:?}"))?;
        let tensor_kind = descriptor
            .TensorKind()
            .map_err(|err| anyhow!("TensorFeatureDescriptor::TensorKind failed: {err:?}"))?;
        let name_string = name.to_string();
        let shape = tensor_descriptor_shape(&descriptor, &name_string)?;
        candidates.push((name, tensor_kind, shape));
    }

    let outputs = classify_outputs(candidates, object_model)?;

    Ok((
        ModelBindingNames {
            image_input,
            orig_target_sizes,
            outputs,
        },
        input_size,
    ))
}

fn classify_outputs(
    candidates: Vec<(HSTRING, TensorKind, Vec<i64>)>,
    object_model: &ObjectDetectionModel,
) -> Result<ModelOutputNames> {
    match object_model {
        ObjectDetectionModel::RtDetrv2 => classify_rt_detr_outputs(candidates),
        ObjectDetectionModel::RfDetr => classify_rf_detr_outputs(candidates),
        ObjectDetectionModel::Yolo5 => classify_yolo5_outputs(candidates),
    }
}

fn classify_rt_detr_outputs(
    candidates: Vec<(HSTRING, TensorKind, Vec<i64>)>,
) -> Result<ModelOutputNames> {
    let mut boxes = None;
    let mut labels = None;
    let mut scores = None;

    for (name, tensor_kind, shape) in candidates.into_iter() {
        match tensor_kind {
            TensorKind::Int64 if labels.is_none() => {
                labels = Some(name);
            }
            TensorKind::Float => {
                let last_dim = shape.last().copied().unwrap_or_default();
                if boxes.is_none() && last_dim == 4 && shape.len() >= 3 {
                    boxes = Some(name);
                } else if scores.is_none() {
                    scores = Some(name);
                }
            }
            _ => continue,
        }
    }

    let boxes = boxes.ok_or_else(|| anyhow!("Failed to locate RT-DETR boxes output"))?;
    let labels = labels.ok_or_else(|| anyhow!("Failed to locate RT-DETR labels output"))?;
    let scores = scores.ok_or_else(|| anyhow!("Failed to locate RT-DETR scores output"))?;

    Ok(ModelOutputNames::RtDetr {
        boxes,
        labels,
        scores,
    })
}

fn classify_rf_detr_outputs(
    candidates: Vec<(HSTRING, TensorKind, Vec<i64>)>,
) -> Result<ModelOutputNames> {
    let mut dets = None;
    let mut logits = None;

    for (name, tensor_kind, shape) in candidates.into_iter() {
        if tensor_kind != TensorKind::Float {
            continue;
        }

        let last_dim = shape.last().copied().unwrap_or_default();
        if dets.is_none() && last_dim == 4 && shape.len() >= 3 {
            dets = Some(name);
        } else if logits.is_none() {
            logits = Some(name);
        }
    }

    let dets = dets.ok_or_else(|| anyhow!("Failed to locate RF-DETR dets output"))?;
    let logits = logits.ok_or_else(|| anyhow!("Failed to locate RF-DETR logits output"))?;

    Ok(ModelOutputNames::RfDetr { dets, logits })
}

fn classify_yolo5_outputs(
    candidates: Vec<(HSTRING, TensorKind, Vec<i64>)>,
) -> Result<ModelOutputNames> {
    let detections = candidates
        .into_iter()
        .find(|(_, kind, _)| *kind == TensorKind::Float)
        .map(|(name, _, _)| name)
        .ok_or_else(|| anyhow!("Failed to locate YOLOv5 detections output"))?;

    Ok(ModelOutputNames::Yolo5 { detections })
}

fn extract_hw_from_shape(dims: &[i64]) -> Option<(usize, usize)> {
    if dims.len() >= 4 {
        let height_raw = dims[dims.len() - 2];
        let width_raw = dims[dims.len() - 1];
        if height_raw > 0 && width_raw > 0 {
            let height = usize::try_from(height_raw).ok()?;
            let width = usize::try_from(width_raw).ok()?;
            Some((width, height))
        } else {
            None
        }
    } else {
        None
    }
}

fn tensor_descriptor_shape(
    descriptor: &TensorFeatureDescriptor,
    context: &str,
) -> Result<Vec<i64>> {
    let shape_view = descriptor
        .Shape()
        .map_err(|err| anyhow!("TensorFeatureDescriptor::Shape ({context}) failed: {err:?}"))?;
    let size = shape_view
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size ({context}) failed: {err:?}"))?
        as usize;
    let mut dims = vec![0i64; size];
    let read = shape_view
        .GetMany(0, &mut dims)
        .map_err(|err| anyhow!("IVectorView::GetMany ({context}) failed: {err:?}"))?
        as usize;
    if read != size {
        dims.truncate(read);
    }
    Ok(dims)
}

fn path_to_hstring(path: &Path) -> Result<HSTRING> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("model path contains invalid UTF-8"))?;
    Ok(HSTRING::from(path_str))
}

fn parse_predictions(
    bindings: &ModelOutputNames,
    evaluation: &LearningModelEvaluationResult,
    params: &PostProcessParams,
) -> Result<SmallVec<[Prediction; 10]>> {
    let outputs = evaluation
        .Outputs()
        .map_err(|err| anyhow!("LearningModelEvaluationResult::Outputs failed: {err:?}"))?;

    let lookup_tensor_float = |name: &HSTRING| -> Result<TensorFloat> {
        let name_str = name.to_string();
        outputs
            .Lookup(name)
            .map_err(|err| anyhow!("Lookup of output '{name_str}' failed: {err:?}"))?
            .cast()
            .map_err(|err| anyhow!("Casting output '{name_str}' to TensorFloat failed: {err:?}"))
    };

    let lookup_tensor_int64 = |name: &HSTRING| -> Result<TensorInt64Bit> {
        let name_str = name.to_string();
        outputs
            .Lookup(name)
            .map_err(|err| anyhow!("Lookup of output '{name_str}' failed: {err:?}"))?
            .cast()
            .map_err(|err| anyhow!("Casting output '{name_str}' to TensorInt64Bit failed: {err:?}"))
    };

    match bindings {
        ModelOutputNames::RtDetr {
            boxes,
            labels,
            scores,
        } => {
            let boxes_tensor = lookup_tensor_float(boxes)?;
            let labels_tensor = lookup_tensor_int64(labels)?;
            let scores_tensor = lookup_tensor_float(scores)?;

            let boxes_data = tensor_float_to_vec(&boxes_tensor)?;
            let labels_data = tensor_int64_to_vec(&labels_tensor)?;
            let scores_data = tensor_float_to_vec(&scores_tensor)?;

            if boxes_data.len() % 4 != 0 {
                bail!(
                    "Unexpected RT-DETR boxes tensor length {}; must be divisible by 4",
                    boxes_data.len()
                );
            }

            let num_queries = boxes_data.len() / 4;
            if labels_data.len() != num_queries || scores_data.len() != num_queries {
                bail!(
                    "RT-DETR output length mismatch: boxes={}, labels={}, scores={}",
                    num_queries,
                    labels_data.len(),
                    scores_data.len()
                );
            }

            let boxes_view = ArrayView2::from_shape((num_queries, 4), boxes_data.as_slice())
                .context("Creating view for RT-DETR boxes output")?;
            let labels_view = ArrayView1::from_shape(num_queries, labels_data.as_slice())
                .context("Creating view for RT-DETR labels output")?;
            let scores_view = ArrayView1::from_shape(num_queries, scores_data.as_slice())
                .context("Creating view for RT-DETR scores output")?;

            rt_detrv2_post_process_from_arrays(labels_view, boxes_view, scores_view, params)
        }
        ModelOutputNames::RfDetr { dets, logits } => {
            let dets_tensor = lookup_tensor_float(dets)?;
            let logits_tensor = lookup_tensor_float(logits)?;

            let dets_data = tensor_float_to_vec(&dets_tensor)?;
            let logits_data = tensor_float_to_vec(&logits_tensor)?;

            let dets_shape = tensor_shape_to_usizes(&dets_tensor, "dets")?;
            let logits_shape = tensor_shape_to_usizes(&logits_tensor, "logits")?;

            if dets_shape.len() != 3 || logits_shape.len() != 3 {
                bail!(
                    "RF-DETR outputs must be 3D; dets={:?}, logits={:?}",
                    dets_shape,
                    logits_shape
                );
            }

            let dets_view = ArrayView3::from_shape(
                (dets_shape[0], dets_shape[1], dets_shape[2]),
                dets_data.as_slice(),
            )
            .context("Creating view for RF-DETR dets output")?;

            let logits_view = ArrayView3::from_shape(
                (logits_shape[0], logits_shape[1], logits_shape[2]),
                logits_data.as_slice(),
            )
            .context("Creating view for RF-DETR logits output")?;

            rf_detr_post_process_from_arrays(dets_view, logits_view, params)
        }
        ModelOutputNames::Yolo5 { detections } => {
            let detections_tensor = lookup_tensor_float(detections)?;
            let detections_data = tensor_float_to_vec(&detections_tensor)?;
            let detections_shape = tensor_shape_to_usizes(&detections_tensor, "detections")?;

            if detections_shape.len() != 3 {
                bail!(
                    "YOLOv5 detections output must be 3D; shape={:?}",
                    detections_shape
                );
            }

            let rows = detections_shape[1];
            let cols = detections_shape[2];

            let detections_view = ArrayView2::from_shape((rows, cols), detections_data.as_slice())
                .context("Creating view for YOLOv5 detections output")?;

            yolo5_post_process_from_arrays(detections_view, params)
        }
    }
}

fn tensor_float_to_vec(tensor: &TensorFloat) -> Result<Vec<f32>> {
    let view = tensor
        .GetAsVectorView()
        .map_err(|err| anyhow!("TensorFloat::GetAsVectorView failed: {err:?}"))?;
    let size = view
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size failed: {err:?}"))? as usize;
    let mut data = vec![0f32; size];
    let read = view
        .GetMany(0, &mut data)
        .map_err(|err| anyhow!("IVectorView::GetMany failed: {err:?}"))? as usize;
    if read != size {
        data.truncate(read);
    }
    Ok(data)
}

fn tensor_int64_to_vec(tensor: &TensorInt64Bit) -> Result<Vec<i64>> {
    let view = tensor
        .GetAsVectorView()
        .map_err(|err| anyhow!("TensorInt64Bit::GetAsVectorView failed: {err:?}"))?;
    let size = view
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size failed: {err:?}"))? as usize;
    let mut data = vec![0i64; size];
    let read = view
        .GetMany(0, &mut data)
        .map_err(|err| anyhow!("IVectorView::GetMany failed: {err:?}"))? as usize;
    if read != size {
        data.truncate(read);
    }
    Ok(data)
}

fn tensor_shape_to_usizes(tensor: &TensorFloat, label: &str) -> Result<Vec<usize>> {
    let shape_view = tensor
        .Shape()
        .map_err(|err| anyhow!("TensorFloat::Shape ({label}) failed: {err:?}"))?;
    let size = shape_view
        .Size()
        .map_err(|err| anyhow!("IVectorView::Size ({label}) failed: {err:?}"))?
        as usize;
    let mut raw = vec![0i64; size];
    let read = shape_view
        .GetMany(0, &mut raw)
        .map_err(|err| anyhow!("IVectorView::GetMany ({label}) failed: {err:?}"))?
        as usize;
    if read != size {
        raw.truncate(read);
    }
    raw.iter()
        .map(|dim| {
            usize::try_from(*dim)
                .map_err(|_| anyhow!("Negative or overflowing dimension {dim} in {label}"))
        })
        .collect()
}
