use crate::api::Prediction;
use ab_glyph::{FontArc, PxScale};
use anyhow::bail;
use bytes::Bytes;
use image::{DynamicImage, ImageBuffer};
use jpeg_encoder::{ColorType, Encoder};
use std::{fmt, path::Path, time::Instant};
use tracing::{debug, info};
use zune_core::{colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

pub struct Image {
    pub name: Option<String>,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn resize(&mut self, size: usize) {
        self.pixels.resize(size, 0);
    }
}

impl fmt::Display for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?}, Resolution: {}x{}",
            self.name, self.width, self.height
        )
    }
}

impl Default for Image {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::with_capacity(99_532_800),
            name: None,
        }
    }
}

pub fn decode_jpeg(name: Option<String>, jpeg: Bytes, image: &mut Image) -> anyhow::Result<()> {
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .set_use_unsafe(true)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(jpeg.as_ref(), options);
    // We need to decode the headers first to get the output buffer size
    decoder.decode_headers()?;
    let output_buffer_size = decoder
        .output_buffer_size()
        .ok_or_else(|| anyhow::anyhow!("Failed to get decoder output buffer size"))?;
    // Resize the output buffer to the required size
    image.resize(output_buffer_size);
    // Decode the image into the output buffer
    decoder.decode_into(&mut image.pixels)?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| anyhow::anyhow!("Failed to get image dimensions"))?;
    image.width = width;
    image.height = height;
    image.name = name;
    Ok(())
}

pub fn load_image(jpeg_file: &Path) -> anyhow::Result<Bytes> {
    if !is_jpeg(jpeg_file.to_str().expect("Failed to get image path")) {
        bail!("Image is not a JPEG file")
    }
    Ok(Bytes::from(std::fs::read(jpeg_file)?))
}

pub fn encode_maybe_draw_boundary_boxes_and_save_jpeg(
    image: &Image,
    jpeg_file: &String,
    predictions: Option<&[Prediction]>,
) -> anyhow::Result<()> {
    let encode_image_start_time = Instant::now();

    let image = create_dynamic_image_maybe_with_boundary_box(predictions, image, 20)?;

    let encoder = Encoder::new_file(jpeg_file, 100)?;
    encoder.encode(
        image.as_rgb8().unwrap(),
        image.width() as u16,
        image.height() as u16,
        ColorType::Rgb,
    )?;
    let encode_image_time = Instant::now().duration_since(encode_image_start_time);
    debug!(?encode_image_time, "Encode image time");
    info!(?jpeg_file, "Image saved");
    Ok(())
}

pub fn is_jpeg(image_name: &str) -> bool {
    image_name.to_lowercase().ends_with(".jpg") || image_name.to_lowercase().ends_with(".jpeg")
}

pub fn create_random_jpeg_name() -> String {
    format!("image_{}.jpg", uuid::Uuid::new_v4())
}

pub fn create_od_image_name(image_name: &str, strip_path: bool) -> anyhow::Result<String> {
    if !is_jpeg(image_name) {
        bail!("Image is not a JPEG file");
    }

    let image_name = if strip_path {
        Path::new(image_name)
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Failed to strip path from image name"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert image name to string"))?
            .to_string()
    } else {
        image_name.to_string()
    };

    let (mut od_image_name, ext) = if let Some(pos) = image_name.rfind('.') {
        if pos + 1 >= image_name.len() {
            bail!("Failed to get image extension");
        }
        (
            image_name[..pos].to_string(),
            image_name[(pos + 1)..].to_string(),
        )
    } else {
        bail!("Failed to get image extension");
    };

    od_image_name.push_str("_od.");
    od_image_name.push_str(&ext);
    Ok(od_image_name)
}

pub fn create_dynamic_image_maybe_with_boundary_box(
    predictions: Option<&[Prediction]>,
    decoded_image: &Image,
    legend_size: u32,
) -> anyhow::Result<DynamicImage> {
    let mut img = ImageBuffer::from_vec(
        decoded_image.width as u32,
        decoded_image.height as u32,
        decoded_image.pixels.clone(),
    )
    .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?;
    let font = if legend_size > 0 {
        Some(FontArc::try_from_slice(include_bytes!(
            "./../assets/roboto-mono-stripped.ttf"
        ))?)
    } else {
        None
    };
    if let Some(predictions) = predictions {
        for prediction in predictions {
            let dx = prediction.x_max - prediction.x_min;
            let dy = prediction.y_max - prediction.y_min;

            if dx > 0 && dy > 0 {
                imageproc::drawing::draw_hollow_rect_mut(
                    &mut img,
                    imageproc::rect::Rect::at(prediction.x_min as i32, prediction.y_min as i32)
                        .of_size(dx as u32, dy as u32),
                    image::Rgb([255, 0, 0]),
                );
            }
            if let Some(font) = font.as_ref() {
                imageproc::drawing::draw_filled_rect_mut(
                    &mut img,
                    imageproc::rect::Rect::at(prediction.x_min as i32, prediction.y_min as i32)
                        .of_size(dx as u32, legend_size),
                    image::Rgb([170, 0, 0]),
                );
                let legend = format!(
                    "{}   {:.0}%",
                    prediction.label,
                    prediction.confidence * 100_f32
                );
                imageproc::drawing::draw_text_mut(
                    &mut img,
                    image::Rgb([255, 255, 255]),
                    prediction.x_min as i32,
                    prediction.y_min as i32,
                    PxScale::from(legend_size as f32 - 1.),
                    font,
                    &legend,
                )
            }
        }
    }
    Ok(DynamicImage::ImageRgb8(img))
}

pub struct Resizer {
    resizer: fast_image_resize::Resizer,
    target_width: usize,
    target_height: usize,
}

impl Default for Resizer {
    fn default() -> Self {
        Self {
            resizer: fast_image_resize::Resizer::new(),
            target_width: 640,
            target_height: 640,
        }
    }
}

impl Resizer {
    pub fn new(target_width: usize, target_height: usize) -> anyhow::Result<Self> {
        let resizer = fast_image_resize::Resizer::new();
        Ok(Self {
            resizer,
            target_width,
            target_height,
        })
    }

    pub fn resize_image(
        &mut self,
        original_image: &mut Image,
        resized_image: &mut Image,
    ) -> anyhow::Result<()> {
        debug!(
            "Resizing image from {}x{} to {}x{}",
            original_image.width, original_image.height, self.target_width, self.target_height
        );
        let src_image = fast_image_resize::images::Image::from_slice_u8(
            original_image.width as u32,
            original_image.height as u32,
            &mut original_image.pixels,
            fast_image_resize::PixelType::U8x3,
        )?;

        if resized_image.height != self.target_height {
            resized_image.height = self.target_height
        }

        if resized_image.width != self.target_width {
            resized_image.width = self.target_width
        }

        resized_image.resize(self.target_width * self.target_height * 3);

        let mut dst_image = fast_image_resize::images::Image::from_slice_u8(
            resized_image.width as u32,
            resized_image.height as u32,
            &mut resized_image.pixels,
            fast_image_resize::PixelType::U8x3,
        )?;

        self.resizer.resize(&src_image, &mut dst_image, None)?;

        Ok(())
    }
}
