use crate::{
    api::{
        StatusUpdateResponse, VersionInfo, VisionCustomListResponse, VisionDetectionRequest,
        VisionDetectionResponse,
    },
    detector::ExecutionProvider,
    image::draw_boundary_boxes_on_encoded_image,
    startup_coordinator::{DetectorInfo, InitResult},
};
use askama::Template;
use axum::{
    Json, Router,
    body::{self, Body},
    extract::{DefaultBodyLimit, Multipart, State},
    http::{Request, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use chrono::Utc;
use crossbeam::channel::Sender;
use mime::IMAGE_JPEG;
use reqwest;
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use tokio::{
    sync::{Mutex, oneshot},
    time::{Duration, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

const MEGABYTE: usize = 1024 * 1024; // 1 MB = 1024 * 1024 bytes
const THIRTY_MEGABYTES: usize = 30 * MEGABYTE; // 30 MB in bytes

enum DetectorReady {
    NotReady,
    Ready {
        sender: Sender<(
            VisionDetectionRequest,
            oneshot::Sender<VisionDetectionResponse>,
            Instant,
        )>,
        #[allow(dead_code)]
        detector_info: DetectorInfo,
        worker_thread_handle: Option<std::thread::JoinHandle<()>>,
    },
    Failed(String),
}

struct ServerState {
    detector_ready: Mutex<DetectorReady>,
    metrics: Mutex<Metrics>,
    restart_token: CancellationToken,
    config_path: PathBuf,
}

pub async fn run_server(
    port: u16,
    cancellation_token: CancellationToken,
    restart_token: CancellationToken,
    detector_init_receiver: tokio::sync::oneshot::Receiver<InitResult>,
    metrics: Metrics,
    config_path: PathBuf,
) -> anyhow::Result<(bool, Option<std::thread::JoinHandle<()>>)> {
    // Return bool to indicate if restart was requested
    let server_state = Arc::new(ServerState {
        detector_ready: Mutex::new(DetectorReady::NotReady),
        metrics: Mutex::new(metrics),
        restart_token: restart_token.clone(),
        config_path,
    });

    // Spawn a task to wait for detector initialization and update the server state
    let state_clone = server_state.clone();
    tokio::spawn(async move {
        match detector_init_receiver.await {
            Ok(InitResult::Success {
                sender,
                detector_info,
                worker_thread_handle,
            }) => {
                info!(
                    model_name = %detector_info.model_name,
                    execution_provider = ?detector_info.execution_provider,
                    "Detector ready - server can now handle requests"
                );

                // Update metrics with real detector info
                {
                    let mut metrics = state_clone.metrics.lock().await;
                    metrics.update_detector_info(&detector_info);
                }

                // Update detector ready state
                {
                    let mut detector_ready = state_clone.detector_ready.lock().await;
                    *detector_ready = DetectorReady::Ready {
                        sender,
                        detector_info,
                        worker_thread_handle: Some(worker_thread_handle),
                    };
                }
            }
            Ok(InitResult::Failed(error)) => {
                error!(error = %error, "Detector initialization failed");
                let mut detector_ready = state_clone.detector_ready.lock().await;
                *detector_ready = DetectorReady::Failed(error);
            }
            Err(_) => {
                error!("Detector initialization channel was dropped");
                let mut detector_ready = state_clone.detector_ready.lock().await;
                *detector_ready =
                    DetectorReady::Failed("Initialization channel dropped".to_string());
            }
        }
    });
    let blue_onyx = Router::new()
        .route("/", get(welcome_handler))
        .route(
            "/v1/status/updateavailable",
            get(v1_status_update_available),
        )
        .route("/v1/vision/detection", post(v1_vision_detection))
        .route("/v1/vision/custom/list", post(v1_vision_custom_list))
        .route("/stats", get(stats_handler))
        .route("/test", get(show_form).post(handle_upload))
        .route("/config", get(config_get_handler).post(config_post_handler))
        .route("/config/restart", post(config_restart_handler))
        .route("/config/loglevel", post(config_loglevel_handler))
        .route("/favicon.ico", get(favicon_handler))
        .route(
            "/static/css/bootstrap-icons.css",
            get(bootstrap_icons_css_handler),
        )
        .fallback(fallback_handler)
        .with_state(server_state.clone())
        .layer(DefaultBodyLimit::max(THIRTY_MEGABYTES));

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);
    info!("Starting server, listening on {}", addr);
    info!("Welcome page, http://127.0.0.1:{}", port);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            error!(
                "Looks like {port} is already in use either by Blue Onyx, CPAI or another application, please turn off the other application or pick another port with --port"
            );
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    };

    let restart_check = restart_token.clone();
    axum::serve(listener, blue_onyx.into_make_service())
        .with_graceful_shutdown(async move {
            tokio::select! {
                _ = cancellation_token.cancelled() => {},
                _ = restart_check.cancelled() => {},
            }
        })
        .await?; // Return true if restart was requested, false if normal shutdown
    // Also return the worker thread handle if available for clean shutdown
    let worker_handle = server_state.take_worker_thread_handle().await;
    Ok((restart_token.is_cancelled(), worker_handle))
}

#[derive(Template)]
#[template(path = "welcome.html")]
struct WelcomeTemplate {
    logo_data: String,
    metrics: Metrics,
}

async fn welcome_handler(State(server_state): State<Arc<ServerState>>) -> impl IntoResponse {
    const LOGO: &[u8] = include_bytes!("../assets/logo_large.png");
    let encoded_logo = general_purpose::STANDARD.encode(LOGO);
    let logo_data = format!("data:image/png;base64,{encoded_logo}");
    let metrics = {
        let metrics_guard = server_state.metrics.lock().await;
        metrics_guard.clone()
    };
    let template = WelcomeTemplate { logo_data, metrics };
    match template.render() {
        Ok(body) => (
            [
                (CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
            ],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template error: {e}"),
        )
            .into_response(),
    }
}

async fn v1_vision_detection(
    State(server_state): State<Arc<ServerState>>,
    mut multipart: Multipart, // Note multipart needs to be last
) -> Result<Json<VisionDetectionResponse>, BlueOnyxError> {
    let request_start_time = Instant::now();
    let mut vision_request = VisionDetectionRequest::default();

    while let Some(field) = multipart.next_field().await? {
        match field.name() {
            Some("min_confidence") => {
                vision_request.min_confidence = field.text().await?.parse::<f32>()?;
            }
            Some("image") => {
                if let Some(image_name) = field.file_name().map(|s| s.to_string()) {
                    vision_request.image_name = image_name;
                }
                vision_request.image_data = field.bytes().await?;
            }
            Some(&_) => {}
            None => {}
        }
    }

    // Check detector state first
    let detector_ready = server_state.detector_ready.lock().await;
    match &*detector_ready {
        DetectorReady::NotReady => {
            // Detector is still initializing, return not ready
            Err(BlueOnyxError(anyhow::anyhow!(
                "Server not ready yet, detector is still initializing"
            )))
        }
        DetectorReady::Failed(error_msg) => {
            // Detector initialization failed
            Err(BlueOnyxError(anyhow::anyhow!(
                "Detector initialization failed: {}",
                error_msg
            )))
        }
        DetectorReady::Ready {
            sender,
            detector_info: _,
            worker_thread_handle: _,
        } => {
            // Detector is ready, proceed with request
            let (response_sender, receiver) = tokio::sync::oneshot::channel();

            if sender.is_full() {
                warn!("Worker queue is full server is overloaded, rejecting request");
                drop(detector_ready); // Release the lock
                update_dropped_requests(server_state).await;
                return Err(BlueOnyxError(anyhow::anyhow!("Worker queue is full")));
            }

            if let Err(err) = sender.send((vision_request, response_sender, request_start_time)) {
                warn!(?err, "Failed to send request to detection worker");
                drop(detector_ready); // Release the lock
                update_dropped_requests(server_state).await;
                return Err(BlueOnyxError(anyhow::anyhow!("Worker queue is full")));
            }

            drop(detector_ready); // Release the lock before waiting
            let result = timeout(Duration::from_secs(30), receiver).await;

            let mut vision_response = match result {
                Ok(Ok(response)) => response,
                Ok(Err(err)) => {
                    warn!("Failed to receive vision detection response: {:?}", err);
                    update_dropped_requests(server_state).await;
                    return Err(BlueOnyxError::from(err));
                }
                Err(_) => {
                    warn!("Timeout while waiting for vision detection response");
                    update_dropped_requests(server_state).await;
                    return Err(BlueOnyxError::from(anyhow::anyhow!("Operation timed out")));
                }
            };
            vision_response.analysisRoundTripMs = request_start_time.elapsed().as_millis() as i32;

            {
                let mut metrics = server_state.metrics.lock().await;
                metrics.update_metrics(&vision_response);
            }

            Ok(Json(vision_response))
        }
    }
}

async fn v1_status_update_available() -> Result<Json<StatusUpdateResponse>, BlueOnyxError> {
    let (latest_release_version_str, release_notes_url) = get_latest_release_info().await?;
    let latest = VersionInfo::parse(latest_release_version_str.as_str(), Some(release_notes_url))?;
    let current = VersionInfo::parse(env!("CARGO_PKG_VERSION"), None)?;
    let updates_available = latest > current;
    let response = StatusUpdateResponse {
        success: true,
        message: "".to_string(),
        version: None, // Deprecated field
        current,
        latest,
        updateAvailable: updates_available,
    };
    Ok(Json(response))
}

async fn v1_vision_custom_list() -> Result<Json<VisionCustomListResponse>, BlueOnyxError> {
    let response = VisionCustomListResponse {
        success: true,
        models: vec![],
        moduleId: "".to_string(),
        moduleName: "".to_string(),
        command: "list".to_string(),
        statusData: None,
        inferenceDevice: "CPU".to_string(),
        analysisRoundTripMs: 0,
        processedBy: "BlueOnyx".to_string(),
        timestampUTC: Utc::now().to_rfc3339(),
    };

    Ok(Json(response))
}

#[derive(Template)]
#[template(path = "stats.html")]
struct StatsTemplate {
    metrics: Metrics,
}

async fn stats_handler(State(server_state): State<Arc<ServerState>>) -> impl IntoResponse {
    let metrics = {
        let metrics_guard = server_state.metrics.lock().await;
        metrics_guard.clone()
    };
    let template = StatsTemplate { metrics };
    match template.render() {
        Ok(body) => (
            [
                (CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
            ],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template error: {e}"),
        )
            .into_response(),
    }
}

async fn show_form() -> impl IntoResponse {
    let template = TestTemplate { image_data: None };
    match template.render() {
        Ok(body) => (
            [
                (CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
            ],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template error: {e}"),
        )
            .into_response(),
    }
}

async fn favicon_handler() -> impl IntoResponse {
    const FAVICON: &[u8] = include_bytes!("../assets/favicon.ico");
    (
        [(axum::http::header::CONTENT_TYPE, "image/x-icon")],
        FAVICON,
    )
        .into_response()
}

#[derive(Template)]
#[template(path = "config.html")]
struct ConfigTemplate {
    logo_data: String,
    config: ConfigTemplateData,
    config_path: String,
    success_message: String,
    error_message: String,
}

#[derive(Debug)]
struct ConfigTemplateData {
    port: u16,
    request_timeout: u64,
    worker_queue_size: String,
    model_selection_type: String,
    builtin_model: String,
    custom_model_path: String,
    custom_model_type: String,
    custom_object_classes: String,
    object_filter_str: String,
    confidence_threshold: f32,
    log_level: String,
    log_path: String,
    force_cpu: bool,
    gpu_index: i32,
    intra_threads: usize,
    inter_threads: usize,
    save_image_path: String,
    save_ref_image: bool,
    save_stats_path: String,
    is_windows: bool,
}

async fn config_get_handler(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    show_config_form("".to_string(), "".to_string(), &state.config_path).await
}

async fn config_post_handler(
    State(state): State<Arc<ServerState>>,
    mut multipart: Multipart,
) -> impl IntoResponse + use<> {
    // Parse form data
    let mut form_data = std::collections::HashMap::new();

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        if let Some(name) = field.name() {
            let name = name.to_string(); // Clone the name first
            if let Ok(value) = field.text().await {
                form_data.insert(name, value);
            }
        }
    }

    // Use the config path from server state
    let current_config_path = &state.config_path;

    let mut config = crate::cli::Cli::load_config(current_config_path).unwrap_or_default();

    // Update configuration from form data
    update_config_from_form_data(&mut config, &form_data);

    // Save the updated configuration
    match config.save_config(current_config_path) {
        Ok(()) => {
            show_config_form(
                "Configuration saved successfully!".to_string(),
                "".to_string(),
                current_config_path,
            )
            .await
        }
        Err(e) => {
            show_config_form(
                "".to_string(),
                format!("Failed to save configuration: {e}"),
                current_config_path,
            )
            .await
        }
    }
}

async fn config_restart_handler(
    State(state): State<Arc<ServerState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    // First, save the configuration (same logic as config_post_handler)
    let mut form_data = std::collections::HashMap::new();

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        if let Some(name) = field.name() {
            let name = name.to_string();
            if let Ok(value) = field.text().await {
                form_data.insert(name, value);
            }
        }
    }

    // Load and update configuration
    let current_config_path = &state.config_path;
    let mut config = crate::cli::Cli::load_config(current_config_path).unwrap_or_default();

    // Apply all form updates (complete parsing logic from config_post_handler)
    update_config_from_form_data(&mut config, &form_data); // Save configuration
    match config.save_config(current_config_path) {
        Ok(()) => {
            info!("Configuration saved, triggering server restart...");

            // Check if detector is ready before restarting
            let detector_ready = state.detector_ready.lock().await;
            match &*detector_ready {
                DetectorReady::Ready { .. } => {
                    // Detector is ready, we can restart immediately
                    drop(detector_ready); // Release the lock
                    state.restart_token.cancel();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "success": true,
                            "message": "Configuration saved and server restart initiated. Please wait...",
                        })),
                    ).into_response()
                }
                DetectorReady::NotReady => {
                    // Detector is still initializing, wait for it to complete then restart
                    drop(detector_ready); // Release the lock

                    // Spawn a task to wait for detector initialization and then restart
                    let restart_token = state.restart_token.clone();
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        info!(
                            "Detector still initializing, waiting for completion before restart..."
                        );

                        // Poll until detector is ready
                        loop {
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            let detector_ready = state_clone.detector_ready.lock().await;
                            match &*detector_ready {
                                DetectorReady::Ready { .. } | DetectorReady::Failed(_) => {
                                    // Detector initialization completed (success or failure)
                                    drop(detector_ready);
                                    info!(
                                        "Detector initialization completed, triggering restart now"
                                    );
                                    restart_token.cancel();
                                    break;
                                }
                                DetectorReady::NotReady => {
                                    // Still not ready, continue waiting
                                    continue;
                                }
                            }
                        }
                    });

                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "success": true,
                            "message": "Configuration saved. Waiting for detector initialization to complete before restart...",
                        })),
                    ).into_response()
                }
                DetectorReady::Failed(_) => {
                    // Detector failed, restart immediately to try again
                    drop(detector_ready);
                    state.restart_token.cancel();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "success": true,
                            "message": "Configuration saved and server restart initiated (detector failed)...",
                        })),
                    ).into_response()
                }
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save configuration: {e}"),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct LogLevelRequest {
    log_level: crate::LogLevel,
}

async fn config_loglevel_handler(Json(payload): Json<LogLevelRequest>) -> impl IntoResponse {
    match crate::update_log_level(payload.log_level) {
        Ok(()) => {
            info!(?payload.log_level, "Log level updated successfully via API");
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "message": format!("Log level updated to {:?}", payload.log_level),
                    "new_level": format!("{:?}", payload.log_level)
                })),
            )
                .into_response()
        }
        Err(e) => {
            error!("Failed to update log level: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "success": false,
                    "message": format!("Failed to update log level: {}", e)
                })),
            )
                .into_response()
        }
    }
}

async fn show_config_form(
    success_message: String,
    error_message: String,
    config_path: &Path,
) -> impl IntoResponse + use<> {
    const LOGO: &[u8] = include_bytes!("../assets/logo_large.png");
    let encoded_logo = general_purpose::STANDARD.encode(LOGO);
    let logo_data = format!("data:image/png;base64,{encoded_logo}");

    // Use the provided config path instead of trying to get the default
    let current_config_path = config_path.to_path_buf();
    let config = crate::cli::Cli::load_config(&current_config_path).unwrap_or_default();

    // Determine if using builtin or custom model
    let (
        model_selection_type,
        builtin_model,
        custom_model_path,
        custom_model_type,
        custom_object_classes,
    ) = if let Some(model_path) = &config.model {
        if let Some(model_filename) = model_path.file_name().and_then(|name| name.to_str()) {
            // Check if this is a builtin model (just filename, no path, and matches known models)
            let is_builtin = !model_filename.contains('\\')
                && !model_filename.contains('/')
                && (model_filename.starts_with("rt-detr")
                    || model_filename == "delivery.onnx"
                    || model_filename.starts_with("IPcam-")
                    || model_filename.starts_with("ipcam-")
                    || model_filename == "package.onnx");

            if is_builtin {
                (
                    "builtin".to_string(),
                    model_filename.to_string(),
                    String::new(),
                    String::new(),
                    String::new(),
                )
            } else {
                (
                    "custom".to_string(),
                    String::new(),
                    model_path.to_string_lossy().to_string(),
                    config.object_detection_model_type.to_string(),
                    config
                        .object_classes
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                )
            }
        } else {
            // Invalid filename, default to builtin
            (
                "builtin".to_string(),
                "rt-detrv2-s.onnx".to_string(),
                String::new(),
                String::new(),
                String::new(),
            )
        }
    } else {
        // No model specified, default to builtin with rt-detrv2-s.onnx
        (
            "builtin".to_string(),
            "rt-detrv2-s.onnx".to_string(),
            String::new(),
            String::new(),
            String::new(),
        )
    };
    let config_data = ConfigTemplateData {
        port: config.port,
        request_timeout: config.request_timeout.as_secs(),
        worker_queue_size: config
            .worker_queue_size
            .map(|v| v.to_string())
            .unwrap_or_default(),
        model_selection_type,
        builtin_model,
        custom_model_path,
        custom_model_type,
        custom_object_classes,
        object_filter_str: config.object_filter.join(", "),
        confidence_threshold: config.confidence_threshold,
        log_level: format!("{:?}", config.log_level),
        log_path: config
            .log_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        force_cpu: config.force_cpu,
        gpu_index: config.gpu_index,
        intra_threads: config.intra_threads,
        inter_threads: config.inter_threads,
        save_image_path: config
            .save_image_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        save_ref_image: config.save_ref_image,
        save_stats_path: config
            .save_stats_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        is_windows: cfg!(target_os = "windows"),
    };

    let template = ConfigTemplate {
        logo_data,
        config: config_data,
        config_path: current_config_path.to_string_lossy().to_string(),
        success_message,
        error_message,
    };

    render_config_template(template)
}

fn render_config_template(template: ConfigTemplate) -> impl IntoResponse {
    match template.render() {
        Ok(body) => (
            [
                (CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
            ],
            body,
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Template error: {e}"),
        )
            .into_response(),
    }
}

async fn fallback_handler(req: Request<Body>) -> impl IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    let body_bytes = body::to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap_or_else(|_| body::Bytes::new());

    debug!(
        "Unimplemented endpoint called: Method: {}, URI: {}, Headers: {:?}, Body: {:?}",
        method, uri, headers, body_bytes
    );

    (StatusCode::NOT_FOUND, "Endpoint not implemented")
}
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct VersionJson {
    version: String,
    windows: String,
    windows_sha256: String,
}

pub async fn get_latest_release_info() -> anyhow::Result<(String, String)> {
    let response =
        reqwest::get("https://github.com/xnorpx/blue-onyx/releases/latest/download/version.json")
            .await?;
    let version_info: VersionJson = response.json().await?;
    let latest_release_version_str = version_info.version;
    let release_notes_url =
        format!("https://github.com/xnorpx/blue-onyx/releases/{latest_release_version_str}");
    Ok((latest_release_version_str, release_notes_url))
}

#[derive(Debug, Clone)]
pub struct Metrics {
    version: String,
    log_path: String,
    start_time: Instant,
    model_name: String,
    execution_provider_name: String,
    number_of_requests: u128,
    dropped_requests: u128,
    total_inference_ms: u128,
    min_inference_ms: i32,
    max_inference_ms: i32,
    total_processing_ms: u128,
    min_processing_ms: i32,
    max_processing_ms: i32,
    total_analysis_round_trip_ms: u128,
    min_analysis_round_trip_ms: i32,
    max_analysis_round_trip_ms: i32,
}

impl Metrics {
    pub fn new(model_name: String, execution_provider: String, log_path: Option<PathBuf>) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            log_path: log_path
                .unwrap_or_else(|| PathBuf::from("stdout"))
                .to_string_lossy()
                .to_string(),
            start_time: Instant::now(),
            model_name,
            execution_provider_name: execution_provider,
            number_of_requests: 0,
            dropped_requests: 0,
            total_inference_ms: 0,
            min_inference_ms: i32::MAX,
            max_inference_ms: i32::MIN,
            total_processing_ms: 0,
            min_processing_ms: i32::MAX,
            max_processing_ms: i32::MIN,
            total_analysis_round_trip_ms: 0,
            min_analysis_round_trip_ms: i32::MAX,
            max_analysis_round_trip_ms: i32::MIN,
        }
    }

    fn uptime(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let days = elapsed.as_secs() / 86400;
        let hours = (elapsed.as_secs() % 86400) / 3600;
        let minutes = (elapsed.as_secs() % 3600) / 60;
        format!("{days} days, {hours} hours and {minutes} minutes")
    }

    fn update_metrics(&mut self, response: &VisionDetectionResponse) {
        self.number_of_requests = self.number_of_requests.wrapping_add(1);
        self.total_inference_ms = self
            .total_inference_ms
            .wrapping_add(response.inferenceMs as u128);
        self.min_inference_ms = self.min_inference_ms.min(response.inferenceMs);
        self.max_inference_ms = self.max_inference_ms.max(response.inferenceMs);
        self.total_processing_ms = self
            .total_processing_ms
            .wrapping_add(response.processMs as u128);
        self.min_processing_ms = self.min_processing_ms.min(response.processMs);
        self.max_processing_ms = self.max_processing_ms.max(response.processMs);
        self.total_analysis_round_trip_ms = self
            .total_analysis_round_trip_ms
            .wrapping_add(response.analysisRoundTripMs as u128);
        self.min_analysis_round_trip_ms = self
            .min_analysis_round_trip_ms
            .min(response.analysisRoundTripMs);
        self.max_analysis_round_trip_ms = self
            .max_analysis_round_trip_ms
            .max(response.analysisRoundTripMs);
    }

    fn update_dropped_requests(&mut self) {
        self.dropped_requests = self.dropped_requests.wrapping_add(1);
    }

    fn avg_ms(&self, total_ms: u128) -> i32 {
        if self.number_of_requests == 0 {
            0
        } else {
            (total_ms as f64 / self.number_of_requests as f64).round() as i32
        }
    }

    fn avg_inference_ms(&self) -> i32 {
        self.avg_ms(self.total_inference_ms)
    }

    fn avg_processing_ms(&self) -> i32 {
        self.avg_ms(self.total_processing_ms)
    }

    fn avg_analysis_round_trip_ms(&self) -> i32 {
        self.avg_ms(self.total_analysis_round_trip_ms)
    }
    pub fn update_detector_info(&mut self, detector_info: &DetectorInfo) {
        self.model_name = detector_info.model_name.clone();
        self.execution_provider_name = match &detector_info.execution_provider {
            ExecutionProvider::CPU => "CPU".to_string(),
            ExecutionProvider::DirectML(index) => format!("DirectML(GPU {index})"),
        };
    }
}

impl ServerState {
    /// Extract the worker thread handle for clean shutdown
    /// Returns the handle if the detector is ready, None otherwise
    pub async fn take_worker_thread_handle(&self) -> Option<std::thread::JoinHandle<()>> {
        let mut detector_ready = self.detector_ready.lock().await;
        match &mut *detector_ready {
            DetectorReady::Ready {
                worker_thread_handle,
                ..
            } => worker_thread_handle.take(),
            _ => None,
        }
    }
}

async fn update_dropped_requests(server_state: Arc<ServerState>) {
    warn!(
        "If you see this message spamming you should reduce the number of requests or upgrade your service to be faster."
    );
    let mut metrics = server_state.metrics.lock().await;
    metrics.update_dropped_requests();
}

#[derive(Template)]
#[template(path = "test.html")]
struct TestTemplate<'a> {
    image_data: Option<&'a str>,
}

async fn handle_upload(
    State(server_state): State<Arc<ServerState>>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let request_start_time = Instant::now();
    loop {
        let field_result = multipart.next_field().await;
        let field = match field_result {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(_) => return (StatusCode::BAD_REQUEST, "Invalid multipart field").into_response(),
        };
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            let content_type = field
                .content_type()
                .map(|ct| ct.to_string())
                .unwrap_or_default();
            if content_type != IMAGE_JPEG.to_string() {
                return (StatusCode::BAD_REQUEST, "Invalid content type").into_response();
            }

            let data: Bytes = match field.bytes().await {
                Ok(d) => d,
                Err(_) => {
                    return (StatusCode::BAD_REQUEST, "Failed to read image bytes").into_response();
                }
            };
            let vision_request = VisionDetectionRequest {
                min_confidence: 0., // This will be set to None and will use server default
                image_data: data.clone(),
                image_name: "image.jpg".to_string(),
            };

            // Check detector state first
            let detector_ready = server_state.detector_ready.lock().await;

            match &*detector_ready {
                DetectorReady::NotReady => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Server not ready yet, detector is still initializing",
                    )
                        .into_response();
                }
                DetectorReady::Failed(error_msg) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Detector initialization failed: {error_msg}"),
                    )
                        .into_response();
                }
                DetectorReady::Ready {
                    sender,
                    detector_info: _,
                    worker_thread_handle: _,
                } => {
                    let (response_sender, receiver) = tokio::sync::oneshot::channel();
                    if let Err(err) =
                        sender.send((vision_request, response_sender, request_start_time))
                    {
                        error!(?err, "Failed to send request to detection worker");
                        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send request")
                            .into_response();
                    }

                    drop(detector_ready); // Release the lock before waiting
                    let result = timeout(Duration::from_secs(30), receiver).await;

                    let mut vision_response = match result {
                        Ok(Ok(response)) => response,
                        Ok(Err(err)) => {
                            error!("Failed to receive vision detection response: {:?}", err);
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to receive response",
                            )
                                .into_response();
                        }
                        Err(_) => {
                            error!("Timeout while waiting for vision detection response");
                            return (StatusCode::INTERNAL_SERVER_ERROR, "Timeout").into_response();
                        }
                    };

                    let data = match draw_boundary_boxes_on_encoded_image(
                        data,
                        vision_response.predictions.as_slice(),
                    ) {
                        Ok(d) => d,
                        Err(_) => {
                            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to draw boxes")
                                .into_response();
                        }
                    };

                    let encoded = general_purpose::STANDARD.encode(&data);
                    let data_url = format!("data:image/jpeg;base64,{encoded}");

                    let template = TestTemplate {
                        image_data: Some(&data_url),
                    };

                    vision_response.analysisRoundTripMs =
                        request_start_time.elapsed().as_millis() as i32;

                    {
                        let mut metrics = server_state.metrics.lock().await;
                        metrics.update_metrics(&vision_response);
                    }
                    match template.render() {
                        Ok(body) => {
                            return (
                                [
                                    (CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
                                    (axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8"),
                                ],
                                body,
                            )
                                .into_response();
                        }
                        Err(e) => {
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Template error: {e}"),
                            )
                                .into_response();
                        }
                    }
                }
            }
        }
    }
    (StatusCode::BAD_REQUEST, "No image field found").into_response()
}

struct BlueOnyxError(anyhow::Error);

impl IntoResponse for BlueOnyxError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(VisionDetectionResponse {
                success: false,
                message: "".into(),
                error: Some(self.0.to_string()),
                predictions: vec![],
                count: 0,
                command: "".into(),
                moduleId: "".into(),
                executionProvider: "".into(),
                canUseGPU: false,
                inferenceMs: 0_i32,
                processMs: 0_i32,
                analysisRoundTripMs: 0_i32,
            }),
        )
            .into_response()
    }
}

impl<E> From<E> for BlueOnyxError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

async fn bootstrap_icons_css_handler() -> impl IntoResponse {
    // Return minimal CSS to prevent 404 errors
    // This could be replaced with actual Bootstrap Icons CSS if needed
    const MINIMAL_CSS: &str = r#"
/* Minimal Bootstrap Icons CSS placeholder */
/* Add actual Bootstrap Icons CSS here if needed */
.icon {
    display: inline-block;
    width: 1em;
    height: 1em;
}
"#;
    (
        [(axum::http::header::CONTENT_TYPE, "text/css")],
        MINIMAL_CSS,
    )
        .into_response()
}

/// Helper function to parse form data and update configuration
fn update_config_from_form_data(
    config: &mut crate::cli::Cli,
    form_data: &std::collections::HashMap<String, String>,
) {
    // Basic server configuration
    if let Some(port_str) = form_data.get("port")
        && let Ok(port) = port_str.parse::<u16>()
    {
        config.port = port;
    }

    if let Some(timeout_str) = form_data.get("request_timeout")
        && let Ok(timeout) = timeout_str.parse::<u64>()
    {
        config.request_timeout = std::time::Duration::from_secs(timeout);
    }

    if let Some(queue_str) = form_data.get("worker_queue_size") {
        config.worker_queue_size = if queue_str.is_empty() {
            None
        } else {
            queue_str.parse::<usize>().ok()
        };
    }

    // Model configuration
    if let Some(model_selection_type) = form_data.get("model_selection_type") {
        match model_selection_type.as_str() {
            "builtin" => {
                if let Some(builtin_model) = form_data.get("builtin_model")
                    && !builtin_model.is_empty()
                {
                    // Set the model path to just the filename (will be found in the executable directory)
                    config.model = Some(PathBuf::from(builtin_model));

                    // Determine model type and set object classes based on the model
                    if builtin_model.starts_with("rt-detr") {
                        config.object_detection_model_type =
                            crate::detector::ObjectDetectionModel::RtDetrv2;
                    } else {
                        config.object_detection_model_type =
                            crate::detector::ObjectDetectionModel::Yolo5;
                    }

                    // Set corresponding YAML file
                    let yaml_file = builtin_model.replace(".onnx", ".yaml");
                    config.object_classes = Some(PathBuf::from(yaml_file));
                }
            }
            "custom" => {
                // Handle custom model configuration
                if let Some(custom_model_path) = form_data.get("custom_model_path") {
                    config.model = if custom_model_path.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(custom_model_path))
                    };
                }

                if let Some(custom_model_type) = form_data.get("custom_model_type") {
                    config.object_detection_model_type = match custom_model_type.as_str() {
                        "Yolo5" => crate::detector::ObjectDetectionModel::Yolo5,
                        _ => crate::detector::ObjectDetectionModel::RtDetrv2,
                    };
                }

                if let Some(custom_classes_str) = form_data.get("custom_object_classes") {
                    config.object_classes = if custom_classes_str.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(custom_classes_str))
                    };
                }
            }
            _ => {}
        }
    }

    // Detection configuration
    if let Some(filter_str) = form_data.get("object_filter") {
        config.object_filter = if filter_str.is_empty() {
            Vec::new()
        } else {
            filter_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        };
    }

    if let Some(confidence_str) = form_data.get("confidence_threshold")
        && let Ok(confidence) = confidence_str.parse::<f32>()
    {
        config.confidence_threshold = confidence;
    }

    // Logging configuration
    if let Some(log_level) = form_data.get("log_level") {
        config.log_level = match log_level.as_str() {
            "Trace" => crate::LogLevel::Trace,
            "Debug" => crate::LogLevel::Debug,
            "Warn" => crate::LogLevel::Warn,
            "Error" => crate::LogLevel::Error,
            _ => crate::LogLevel::Info,
        };
    }

    if let Some(log_path_str) = form_data.get("log_path") {
        config.log_path = if log_path_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(log_path_str))
        };
    }

    // Performance configuration
    config.force_cpu = form_data.contains_key("force_cpu");

    if let Some(gpu_str) = form_data.get("gpu_index")
        && let Ok(gpu_index) = gpu_str.parse::<i32>()
    {
        config.gpu_index = gpu_index;
    }

    if let Some(intra_str) = form_data.get("intra_threads")
        && let Ok(intra_threads) = intra_str.parse::<usize>()
    {
        config.intra_threads = intra_threads;
    }

    if let Some(inter_str) = form_data.get("inter_threads")
        && let Ok(inter_threads) = inter_str.parse::<usize>()
    {
        config.inter_threads = inter_threads;
    }

    // Output configuration
    if let Some(save_image_str) = form_data.get("save_image_path") {
        config.save_image_path = if save_image_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(save_image_str))
        };
    }

    config.save_ref_image = form_data.contains_key("save_ref_image");

    if let Some(save_stats_str) = form_data.get("save_stats_path") {
        config.save_stats_path = if save_stats_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(save_stats_str))
        };
    }
}
