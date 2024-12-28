use crate::{
    api::{
        StatusUpdateResponse, VersionInfo, VisionCustomListResponse, VisionDetectionRequest,
        VisionDetectionResponse,
    },
    image::draw_boundary_boxes_on_encoded_image,
};
use askama::Template;
use axum::{
    body::{self, Body},
    extract::{DefaultBodyLimit, Multipart, State},
    http::{header::CACHE_CONTROL, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use chrono::Utc;
use mime::IMAGE_JPEG;
use reqwest;
use serde::Deserialize;
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{mpsc::Sender, Arc},
    time::Instant,
};
use tokio::{
    sync::{oneshot, Mutex},
    time::{timeout, Duration},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

const MEGABYTE: usize = 1024 * 1024; // 1 MB = 1024 * 1024 bytes
const THIRTY_MEGABYTES: usize = 30 * MEGABYTE; // 30 MB in bytes

struct ServerState {
    sender: Sender<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
    )>,
    metrics: Mutex<Metrics>,
}

pub async fn run_server(
    port: u16,
    cancellation_token: CancellationToken,
    sender: Sender<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
    )>,
    metrics: Metrics,
) -> anyhow::Result<()> {
    let server_state = Arc::new(ServerState {
        sender,
        metrics: Mutex::new(metrics), // TODO: Implement metrics
    });

    let blue_onyx = Router::new()
        .route(
            "/",
            get(|| async { (StatusCode::OK, "Blue Onyx is alive and healthy") }),
        )
        .route(
            "/v1/status/updateavailable",
            get(v1_status_update_available),
        )
        .route("/v1/vision/detection", post(v1_vision_detection))
        .with_state(server_state.clone())
        .route("/v1/vision/custom/list", post(v1_vision_custom_list))
        .route("/stats", get(stats_handler))
        .with_state(server_state.clone())
        .route("/test", get(show_form).post(handle_upload))
        .with_state(server_state.clone())
        .route("/favicon.ico", get(favicon_handler))
        .fallback(fallback_handler)
        .layer(DefaultBodyLimit::max(THIRTY_MEGABYTES));

    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);
    info!("Starting server, listening on {}", addr);
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            error!("Looks like {port} is already in use either by Blue Onyx, CPAI or another application, please turn off the other application or pick another port with --port");
            return Err(e.into());
        }
        Err(e) => return Err(e.into()),
    };

    axum::serve(listener, blue_onyx.into_make_service())
        .with_graceful_shutdown(async move {
            cancellation_token.cancelled().await;
        })
        .await?;

    Ok(())
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

    let (sender, receiver) = tokio::sync::oneshot::channel();
    if let Err(err) = server_state.sender.send((vision_request, sender)) {
        error!(?err, "Failed to send request to detection worker");
    }
    let result = timeout(Duration::from_secs(30), receiver).await;

    let mut vision_response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(err)) => {
            error!("Failed to receive vision detection response: {:?}", err);
            return Err(BlueOnyxError::from(err));
        }
        Err(_) => {
            error!("Timeout while waiting for vision detection response");
            return Err(BlueOnyxError::from(anyhow::anyhow!("Operation timed out")));
        }
    };
    vision_response.analysis_round_trip_ms = request_start_time.elapsed().as_millis() as i32;

    {
        let mut metrics = server_state.metrics.lock().await;
        metrics.update_metrics(&vision_response);
    }

    Ok(Json(vision_response))
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
    (
        [(CACHE_CONTROL, "no-store, no-cache, must-revalidate")],
        template.into_response(),
    )
}

async fn show_form() -> impl IntoResponse {
    let template = TestTemplate { image_data: None };
    (
        [(CACHE_CONTROL, "no-store, no-cache, must-revalidate")],
        template.into_response(),
    )
}

async fn favicon_handler() -> impl IntoResponse {
    StatusCode::NO_CONTENT
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
    let release_notes_url = format!(
        "https://github.com/xnorpx/blue-onyx/releases/{}",
        latest_release_version_str
    );
    Ok((latest_release_version_str, release_notes_url))
}

#[derive(Debug, Clone)]
pub struct Metrics {
    start_time: Instant,
    model_name: String,
    device_name: String,
    execution_provider_name: String,
    number_of_requests: u128,
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
    pub fn new(model_name: String, device_name: String, execution_provider: String) -> Self {
        Self {
            start_time: Instant::now(),
            model_name,
            device_name,
            execution_provider_name: execution_provider,
            number_of_requests: 0,
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
        format!("{} days, {} hours and {} minutes", days, hours, minutes)
    }

    fn update_metrics(&mut self, response: &VisionDetectionResponse) {
        self.number_of_requests = self.number_of_requests.wrapping_add(1);
        self.total_inference_ms = self
            .total_inference_ms
            .wrapping_add(response.inference_ms as u128);
        self.min_inference_ms = self.min_inference_ms.min(response.inference_ms);
        self.max_inference_ms = self.max_inference_ms.max(response.inference_ms);
        self.total_processing_ms = self
            .total_processing_ms
            .wrapping_add(response.process_ms as u128);
        self.min_processing_ms = self.min_processing_ms.min(response.process_ms);
        self.max_processing_ms = self.max_processing_ms.max(response.process_ms);
        self.total_analysis_round_trip_ms = self
            .total_analysis_round_trip_ms
            .wrapping_add(response.analysis_round_trip_ms as u128);
        self.min_analysis_round_trip_ms = self
            .min_analysis_round_trip_ms
            .min(response.analysis_round_trip_ms);
        self.max_analysis_round_trip_ms = self
            .max_analysis_round_trip_ms
            .max(response.analysis_round_trip_ms);
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
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "image" {
            let content_type = field
                .content_type()
                .map(|ct| ct.to_string())
                .unwrap_or_default();
            if content_type != IMAGE_JPEG.to_string() {
                return Err(StatusCode::BAD_REQUEST);
            }

            let data: Bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            let vision_request = VisionDetectionRequest {
                min_confidence: 0., // This will be set to None and will use server default
                image_data: data.clone(),
                image_name: "image.jpg".to_string(),
            };

            let (sender, receiver) = tokio::sync::oneshot::channel();
            if let Err(err) = server_state.sender.send((vision_request, sender)) {
                error!(?err, "Failed to send request to detection worker");
            }
            let result = timeout(Duration::from_secs(30), receiver).await;

            let mut vision_response = match result {
                Ok(Ok(response)) => response,
                Ok(Err(err)) => {
                    error!("Failed to receive vision detection response: {:?}", err);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
                Err(_) => {
                    error!("Timeout while waiting for vision detection response");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

            let data =
                draw_boundary_boxes_on_encoded_image(data, vision_response.predictions.as_slice())
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let encoded = general_purpose::STANDARD.encode(&data);
            let data_url = format!("data:image/jpeg;base64,{}", encoded);

            let template = TestTemplate {
                image_data: Some(&data_url),
            };

            vision_response.analysis_round_trip_ms =
                request_start_time.elapsed().as_millis() as i32;

            {
                let mut metrics = server_state.metrics.lock().await;
                metrics.update_metrics(&vision_response);
            }
            return Ok((
                [(CACHE_CONTROL, "no-store, no-cache, must-revalidate")],
                template.into_response(),
            ));
        }
    }
    Err(StatusCode::BAD_REQUEST)
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
                module_id: "".into(),
                execution_provider: "".into(),
                can_useGPU: false,
                inference_ms: 0_i32,
                process_ms: 0_i32,
                analysis_round_trip_ms: 0_i32,
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
