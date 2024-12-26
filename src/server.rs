use crate::api::{
    StatusUpdateResponse, VersionInfo, VisionCustomListResponse, VisionDetectionRequest,
    VisionDetectionResponse,
};
use askama::Template;
use axum::{
    body::{self, Body},
    extract::{DefaultBodyLimit, Multipart, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
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
use tracing::{error, info, warn};

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
struct StatsTemplate<'a> {
    message: &'a str,
}

async fn stats_handler(State(_server_state): State<Arc<ServerState>>) -> StatsTemplate<'static> {
    StatsTemplate {
        message: "Hello World",
    }
}

async fn favicon_handler() -> impl IntoResponse {
    StatusCode::NO_CONTENT
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

async fn fallback_handler(req: Request<Body>) -> impl IntoResponse {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    let body_bytes = body::to_bytes(req.into_body(), usize::MAX)
        .await
        .unwrap_or_else(|_| body::Bytes::new());

    warn!(
        "Unimplemented endpoint called: Method: {}, URI: {}, Headers: {:?}, Body: {:?}",
        method, uri, headers, body_bytes
    );

    (StatusCode::NOT_FOUND, "Endpoint not implemented")
}

impl<E> From<E> for BlueOnyxError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
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

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Metrics {
    start_time: Instant,
    model_name: String,
    device_name: String,
    execution_provider_name: String,
    using_gpu: bool,
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
    pub fn new(
        model_name: String,
        device_name: String,
        execution_provider: String,
        using_gpu: bool,
    ) -> Self {
        Self {
            start_time: Instant::now(),
            model_name,
            device_name,
            execution_provider_name: execution_provider,
            using_gpu,
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

    #[allow(unused)]
    fn avg_ms(&self, total_ms: u128) -> i32 {
        if self.number_of_requests == 0 {
            0
        } else {
            (total_ms as f64 / self.number_of_requests as f64).round() as i32
        }
    }

    #[allow(unused)]
    fn avg_inference_ms(&self) -> i32 {
        self.avg_ms(self.total_inference_ms)
    }

    #[allow(unused)]
    fn avg_processing_ms(&self) -> i32 {
        self.avg_ms(self.total_processing_ms)
    }

    #[allow(unused)]
    fn avg_analysis_round_trip_ms(&self) -> i32 {
        self.avg_ms(self.total_analysis_round_trip_ms)
    }
}
