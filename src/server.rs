use crate::api::{
    StatusUpdateResponse, VersionInfo, VisionCustomListResponse, VisionDetectionRequest,
    VisionDetectionResponse,
};
use axum::{
    body::{self, Body},
    extract::{DefaultBodyLimit, Multipart, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use std::{
    net::{Ipv4Addr, SocketAddr},
    sync::{mpsc::Sender, Arc},
    time::Instant,
};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

const MEGABYTE: usize = 1024 * 1024; // 1 MB = 1024 * 1024 bytes
const THIRTY_MEGABYTES: usize = 30 * MEGABYTE; // 30 MB in bytes

struct Metrics {}

struct ServerState {
    sender: Sender<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
    )>,
    _metrics: Metrics, // TODO: Implement metrics
}

pub async fn run_server(
    port: u16,
    cancellation_token: CancellationToken,
    sender: Sender<(
        VisionDetectionRequest,
        oneshot::Sender<VisionDetectionResponse>,
    )>,
) -> anyhow::Result<()> {
    let server_state = Arc::new(ServerState {
        sender,
        _metrics: Metrics {}, // TODO: Implement metrics
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
    Ok(Json(vision_response))
}

async fn v1_status_update_available() -> Result<Json<StatusUpdateResponse>, BlueOnyxError> {
    let response = StatusUpdateResponse {
        success: true,
        message: "".to_string(),
        version: None,
        current: VersionInfo::default(),
        latest: VersionInfo::default(),
        updateAvailable: false, // Always respond that no update is available
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
