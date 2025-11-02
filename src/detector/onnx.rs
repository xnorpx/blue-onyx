use super::common::{
    BackendInit, DetectResult, DetectorConfig, DeviceType, EndpointProvider, InferenceCommon,
    ObjectDetectionModel, OnnxConfig, PostProcessParams, rf_detr_post_process_from_arrays,
    rt_detrv2_post_process_from_arrays, yolo5_post_process_from_arrays,
};
#[cfg(windows)]
use crate::direct_ml_available;
use crate::{
    api::Prediction,
    image::{create_od_image_name, decode_jpeg, encode_maybe_draw_boundary_boxes_and_save_jpeg},
};
use anyhow::{Context, anyhow, bail};
use bytes::Bytes;
use ndarray::{Array, Ix1, Ix2, Ix3};
#[cfg(windows)]
use ort::execution_providers::DirectMLExecutionProvider;
use ort::{
    inputs,
    session::{Session, SessionInputs, SessionOutputs},
    value::{Value, ValueType},
};
use smallvec::SmallVec;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use tracing::{debug, info, warn};

pub struct Inference {
    session: Session,
    common: InferenceCommon,
    object_detection_model: ObjectDetectionModel,
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
            Self::Yolo5 => yolo5_pre_process(input, orig_size),
        }
    }

    pub fn post_process(
        &self,
        outputs: SessionOutputs<'_>,
        params: &PostProcessParams,
    ) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
        type PostProcessFn = for<'a> fn(
            SessionOutputs<'a>,
            &PostProcessParams,
        ) -> anyhow::Result<SmallVec<[Prediction; 10]>>;

        let handler: PostProcessFn = match self {
            Self::RtDetrv2 => rt_detrv2_post_process,
            Self::RfDetr => rf_detr_post_process,
            Self::Yolo5 => yolo5_post_process,
        };

        handler(outputs, params)
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
    _orig_size: &'a Array<i64, ndarray::Dim<[usize; 2]>>,
) -> anyhow::Result<SessionInputs<'a, 'a>> {
    Ok(inputs![
        "images" => Value::from_array(input.clone())?,
    ]
    .into())
}

impl Inference {
    pub fn new(detector_config: DetectorConfig) -> anyhow::Result<Self> {
        let (device_type, model_name, session, endpoint_provider, model_yaml_path, input_size) =
            initialize_onnx(&detector_config.object_detection_onnx_config)?;

        let backend = BackendInit {
            device_type,
            endpoint_provider,
            model_name,
            model_yaml_path,
            input_size,
        };

        let object_detection_model = detector_config.object_detection_model.clone();
        let common = InferenceCommon::new(&detector_config, backend)?;

        let mut inference = Self {
            session,
            common,
            object_detection_model,
        };

        info!("Warming up the detector");
        let detector_warmup_start_time = Instant::now();
        inference.detect(Bytes::from(crate::DOG_BIKE_CAR_BYTES), None, None)?;
        info!(
            "Detector warmed up in: {:?}",
            detector_warmup_start_time.elapsed()
        );

        Ok(inference)
    }

    pub fn detect(
        &mut self,
        image_bytes: Bytes,
        image_name: Option<String>,
        min_confidence: Option<f32>,
    ) -> anyhow::Result<DetectResult> {
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

        let orig_size = Array::from_shape_vec(
            (1, 2),
            vec![common.input_height as i64, common.input_width as i64],
        )?;
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
        let pre_process_model_input_start = Instant::now();
        let session_inputs = self
            .object_detection_model
            .pre_process(&mut common.input, &orig_size)?;

        debug!(
            "Pre-process model input time: {:?}",
            pre_process_model_input_start.elapsed()
        );

        let pre_processing_time = processing_time_start.elapsed();
        debug!("Pre-process time: {:?}", pre_processing_time);
        let start_inference_time = Instant::now();
        let outputs: SessionOutputs = self.session.run(session_inputs)?;
        let inference_time = start_inference_time.elapsed();
        debug!("Inference time: {:?}", inference_time);
        let post_processing_time_start = Instant::now();
        let confidence_threshold = min_confidence.unwrap_or(common.confidence_threshold);
        let params =
            common.make_post_process_params(confidence_threshold, resize_factor_x, resize_factor_y);
        let predictions = self.object_detection_model.post_process(outputs, &params)?;

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
        let num_intra_threads = onnx_config
            .intra_threads
            .min(num_cpus::get_physical() - 1)
            .min(16);
        let num_inter_threads = onnx_config
            .inter_threads
            .min(num_cpus::get_physical() - 1)
            .min(16);
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
            let num_intra_threads = onnx_config
                .intra_threads
                .min(num_cpus::get_physical() - 1)
                .min(16);
            let num_inter_threads = onnx_config
                .inter_threads
                .min(num_cpus::get_physical() - 1)
                .min(16);
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
            (num_intra_threads, num_inter_threads)
        }

        #[cfg(not(windows))]
        {
            let num_intra_threads = onnx_config
                .intra_threads
                .min(num_cpus::get_physical() - 1)
                .min(16);
            let num_inter_threads = onnx_config
                .inter_threads
                .min(num_cpus::get_physical() - 1)
                .min(16);
            warn!(
                "GPU acceleration not available on this platform, using CPU for inference with {} intra and {} inter threads",
                num_intra_threads, num_inter_threads
            );
            (num_intra_threads, num_inter_threads)
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

fn query_image_input_size(session: &Session) -> anyhow::Result<(usize, usize)> {
    let input = session
        .inputs
        .iter()
        .find(|candidate| matches!(candidate.input_type, ValueType::Tensor { .. }))
        .ok_or_else(|| anyhow!("Model does not expose a tensor input"))?;

    let ValueType::Tensor { shape, .. } = &input.input_type else {
        unreachable!("Filtered to tensor inputs only");
    };

    if shape.len() != 4 {
        bail!(
            "Expected tensor input shape [batch, channels, height, width]; got rank {}",
            shape.len()
        );
    }

    let batch = shape[0];
    if batch != 1 {
        warn!("Unexpected batch dimension {batch}; continuing with assumption batch size is 1");
    }

    let channels = shape[1];
    if channels != 3 {
        warn!("Unexpected channel count {channels}; detector expects 3 channels (RGB)");
    }

    let height = shape[2];
    let width = shape[3];

    if height <= 0 || width <= 0 {
        bail!("Input tensor height/width must be positive; got height={height}, width={width}");
    }

    Ok((width as usize, height as usize))
}

fn rt_detrv2_post_process(
    outputs: SessionOutputs<'_>,
    params: &PostProcessParams,
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let mut labels_tensor = None;
    let mut boxes_tensor = None;
    let mut scores_tensor = None;

    for (name, value) in outputs {
        match name {
            "labels" => labels_tensor = Some(value),
            "boxes" | "bboxes" => boxes_tensor = Some(value),
            "scores" => scores_tensor = Some(value),
            unexpected => {
                debug!(
                    unexpected_output = unexpected,
                    "Ignoring unexpected output from RT-DETRv2 session"
                );
            }
        }
    }

    let labels = labels_tensor.context("RT-DETRv2 output 'labels' missing")?;
    let boxes = boxes_tensor.context("RT-DETRv2 output 'boxes' missing")?;
    let scores = scores_tensor.context("RT-DETRv2 output 'scores' missing")?;

    let labels_view = labels
        .try_extract_array::<i64>()
        .context("Extracting RT-DETRv2 labels array")?
        .into_dimensionality::<Ix1>()
        .context("RT-DETRv2 labels tensor must be 1D")?;
    let boxes_view = boxes
        .try_extract_array::<f32>()
        .context("Extracting RT-DETRv2 boxes array")?
        .into_dimensionality::<Ix2>()
        .context("RT-DETRv2 boxes tensor must be 2D")?;
    let scores_view = scores
        .try_extract_array::<f32>()
        .context("Extracting RT-DETRv2 scores array")?
        .into_dimensionality::<Ix1>()
        .context("RT-DETRv2 scores tensor must be 1D")?;

    rt_detrv2_post_process_from_arrays(labels_view, boxes_view, scores_view, params)
}

fn rf_detr_post_process(
    outputs: SessionOutputs<'_>,
    params: &PostProcessParams,
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let mut dets_tensor = None;
    let mut logits_tensor = None;

    for (name, value) in outputs {
        match name {
            "dets" | "boxes" => dets_tensor = Some(value),
            "logits" | "scores" => logits_tensor = Some(value),
            unexpected => debug!(
                unexpected_output = unexpected,
                "Ignoring unexpected RF-DETR output"
            ),
        }
    }

    let dets = dets_tensor.context("RF-DETR output 'dets' missing")?;
    let logits = logits_tensor.context("RF-DETR output 'logits' missing")?;

    let dets_view = dets
        .try_extract_array::<f32>()
        .context("Extracting RF-DETR dets array")?
        .into_dimensionality::<Ix3>()
        .context("RF-DETR detections tensor must be 3D")?;
    let logits_view = logits
        .try_extract_array::<f32>()
        .context("Extracting RF-DETR logits array")?
        .into_dimensionality::<Ix3>()
        .context("RF-DETR logits tensor must be 3D")?;

    rf_detr_post_process_from_arrays(dets_view, logits_view, params)
}

fn yolo5_post_process(
    outputs: SessionOutputs<'_>,
    params: &PostProcessParams,
) -> anyhow::Result<SmallVec<[Prediction; 10]>> {
    let mut detections_tensor = None;

    for (name, value) in outputs {
        match name {
            "detections" | "output" => detections_tensor = Some(value),
            unexpected => debug!(
                unexpected_output = unexpected,
                "Ignoring unexpected YOLOv5 output"
            ),
        }
    }

    let detections = detections_tensor.context("YOLOv5 output 'detections' missing")?;

    let detections_view = detections
        .try_extract_array::<f32>()
        .context("Extracting YOLOv5 detections array")?
        .into_dimensionality::<Ix2>()
        .context("YOLOv5 detections tensor must be 2D")?;

    yolo5_post_process_from_arrays(detections_view, params)
}
