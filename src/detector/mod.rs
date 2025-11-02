#[cfg(any(not(windows), all(windows, not(feature = "windows_ml"))))]
mod onnx;
#[cfg(any(not(windows), all(windows, not(feature = "windows_ml"))))]
pub use onnx::*;

#[cfg(all(windows, feature = "windows_ml"))]
mod windows_ml;
#[cfg(all(windows, feature = "windows_ml"))]
pub use windows_ml::*;

mod common;
pub use common::*;
