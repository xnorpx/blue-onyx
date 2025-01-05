use anyhow::{anyhow, Context};
use axum::body::Bytes;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt::Debug;

#[derive(Default)]
pub struct VisionDetectionRequest {
    pub min_confidence: f32,
    pub image_data: Bytes,
    pub image_name: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", default)]
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
    pub moduleId: String,
    /// The name of the device or package handling the inference. eg CPU, GPU
    pub executionProvider: String,
    /// True if this module can use the current GPU if one is present.
    pub canUseGPU: bool,
    // The time (ms) to perform the AI inference.
    pub inferenceMs: i32,
    // The time (ms) to process the image (includes inference and image manipulation operations).
    pub processMs: i32,
    // The time (ms) for the round trip to the analysis module and back.
    pub analysisRoundTripMs: i32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
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

impl VersionInfo {
    pub fn parse(version_str: &str, release_notes: Option<String>) -> anyhow::Result<Self> {
        let parts: Vec<_> = version_str.trim().split('.').collect();
        let major: u8 = parts
            .first()
            .ok_or_else(|| anyhow!("Missing major version segment"))?
            .parse()
            .context("Failed to parse major version")?;
        let minor: u8 = parts
            .get(1)
            .ok_or_else(|| anyhow!("Missing minor version segment"))?
            .parse()
            .context("Failed to parse minor version")?;
        let patch: u8 = parts
            .get(2)
            .ok_or_else(|| anyhow!("Missing patch version segment"))?
            .parse()
            .context("Failed to parse patch version")?;

        Ok(Self {
            major,
            minor,
            patch,
            releaseNotes: release_notes.unwrap_or_default(),
            ..Default::default()
        })
    }
}
impl PartialEq for VersionInfo {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major && self.minor == other.minor && self.patch == other.patch
    }
}

impl Eq for VersionInfo {}

impl PartialOrd for VersionInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for VersionInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other_order => other_order,
            },
            other_order => other_order,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VersionInfo;
    use std::cmp::Ordering;

    #[test]
    fn test_eq_and_ne() {
        let v1 = VersionInfo {
            major: 1,
            minor: 2,
            patch: 3,
            ..Default::default()
        };
        let v2 = VersionInfo {
            major: 1,
            minor: 2,
            patch: 3,
            ..Default::default()
        };
        let v3 = VersionInfo {
            major: 1,
            minor: 3,
            patch: 3,
            ..Default::default()
        };
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
        assert!(v3 > v2);
        assert!(v2 <= v1);
    }

    #[test]
    fn test_partial_ord_and_ord() {
        let v1 = VersionInfo {
            major: 1,
            minor: 2,
            patch: 3,
            ..Default::default()
        };
        let v2 = VersionInfo {
            major: 1,
            minor: 2,
            patch: 4,
            ..Default::default()
        };
        let v3 = VersionInfo {
            major: 2,
            minor: 0,
            patch: 0,
            ..Default::default()
        };

        // Check ordering with v1 and v2
        assert_eq!(v1.cmp(&v2), Ordering::Less);
        assert!(v1 < v2);

        // Check ordering with v3 and v2
        assert_eq!(v3.cmp(&v2), Ordering::Greater);
        assert!(v3 > v2);
    }
}
