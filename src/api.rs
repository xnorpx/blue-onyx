use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

#[derive(Default)]
pub struct VisionDetectionRequest {
    pub min_confidence: f32,
    pub image_data: Bytes,
    pub image_name: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VisionDetectionResponse {
    /// True if successful.
    pub success: bool,
    /// A summary of the inference operation.
    pub message: String,
    /// An description of the error if success was false.
    pub error: Option<String>,
    /// An array of objects with the x_max, x_min, max, y_min, label and confidence.
    pub predictions: Vec<Prediction>,
    /// The number of objects found.
    pub count: i32,
    /// The command that was sent as part of this request. Can be detect, list, status.
    pub command: String,
    /// The Id of the module that processed this request.
    pub module_id: String,
    /// The name of the device or package handling the inference. eg CPU, GPU
    pub execution_provider: String,
    /// True if this module can use the current GPU if one is present.
    pub can_useGPU: bool,
    // The time (ms) to perform the AI inference.
    pub inference_ms: i32,
    // The time (ms) to process the image (includes inference and image manipulation operations).
    pub process_ms: i32,
    // The time (ms) for the round trip to the analysis module and back.
    pub analysis_round_trip_ms: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Prediction {
    pub x_max: usize,
    pub x_min: usize,
    pub y_max: usize,
    pub y_min: usize,
    pub confidence: f32,
    pub label: String,
}

impl Debug for Prediction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prediction")
            .field("label", &self.label)
            .field("confidence", &self.confidence)
            .finish()
    }
}

#[allow(non_snake_case)]
#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VisionCustomListResponse {
    pub success: bool,
    pub models: Vec<String>,
    pub moduleId: String,
    pub moduleName: String,
    pub command: String,
    pub statusData: Option<String>,
    pub inferenceDevice: String,
    pub analysisRoundTripMs: i32,
    pub processedBy: String,
    pub timestampUTC: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatusUpdateResponse {
    pub success: bool,
    pub message: String,
    pub version: Option<VersionInfo>, // Deprecated field
    pub current: VersionInfo,
    pub latest: VersionInfo,
    pub updateAvailable: bool,
}

#[allow(non_snake_case)]
#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub preRelease: Option<String>,
    pub securityUpdate: bool,
    pub build: u32,
    pub file: String,
    pub releaseNotes: String,
}
