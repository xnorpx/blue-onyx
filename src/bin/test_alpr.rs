use blue_onyx::{alpr::LicensePlateRecognition, image::decode_jpeg, init_logging};
use bytes::Bytes;
use std::path::PathBuf;
use tracing::info;

fn main() -> anyhow::Result<()> {
    init_logging(blue_onyx::LogLevel::Info, None);
    info!("Let's f send it");
    let model_path = PathBuf::from("c:\\git\\blue-onyx\\wpod_net.onnx");

    let test_car_image = PathBuf::from("c:\\git\\blue-onyx\\assets\\Cars450.jpeg");
    // let test_car_image = PathBuf::from("c:\\git\\blue-onyx\\assets\\Cars450.jpeg");
    let image_bytes = Bytes::from(std::fs::read(test_car_image)?);
    let mut decoded_image = blue_onyx::image::Image::default();
    decode_jpeg(
        Some("Cars450.jpeg".to_string()),
        image_bytes,
        &mut decoded_image,
    )?;

    let mut lpr = LicensePlateRecognition::new(model_path, 0.9)?;
    lpr.detect(&mut decoded_image)?;

    info!("It's f done");

    Ok(())
}
