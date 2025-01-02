// Automatic License Plate Recognition (ALPR)

// This module is responsible for detecting license plates in images.
// <https://openaccess.thecvf.com/content_ECCV_2018/papers/Sergio_Silva_License_Plate_Detection_ECCV_2018_paper.pdf>

// Step 1: Car detection (DETR)
// Step 2: License Plate Detection (WPOD-NET)
// Step 3: Rectification
// Step 4: OCR

use anyhow::anyhow;
use nalgebra::{SMatrix, SVector};
use ndarray::{array, s, Array, ArrayBase, ArrayView, Axis, Dim, OwnedRepr};
use ort::{
    execution_providers::DirectMLExecutionProvider,
    inputs,
    session::{builder::SessionBuilder, Session, SessionOutputs},
};
use std::{path::PathBuf, time::Instant};
use tracing::info;

use crate::{
    api::Prediction,
    image::{encode_maybe_draw_boundary_boxes_and_save_jpeg, Image, Resizer},
};

pub struct LicensePlateRecognition {
    session: Session,
    resizer: Resizer,
    resized_image: Image,
    ocr_image: Image,
    resized_ocr_image: Image,
    input: ndarray::ArrayBase<ndarray::OwnedRepr<f32>, ndarray::Dim<[usize; 4]>>,
    q: ArrayBase<OwnedRepr<f32>, Dim<[usize; 2]>>,
    confidence_threshold: f32,
}

impl LicensePlateRecognition {
    pub fn new(model: PathBuf, confidence_threshold: f32) -> anyhow::Result<Self> {
        // TODO: Should have a common session init with execution providers
        let mut execution_providers = vec![];
        execution_providers.push(
            DirectMLExecutionProvider::default()
                .with_device_id(0)
                .build()
                .error_on_failure(),
        );
        let session = Session::builder()?
            .with_execution_providers(execution_providers)?
            .commit_from_file(model)?;
        let q = array![
            [-0.5, 0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5, 0.5],
            [1.0, 1.0, 1.0, 1.0]
        ];

        Ok(Self {
            session,
            resizer: Resizer::new(512, 384)?,
            resized_image: Image::default(),
            ocr_image: Image::default(),
            resized_ocr_image: Image::default(),
            input: Array::zeros((1, 384, 512, 3)),
            q,
            confidence_threshold,
        })
    }

    pub fn detect(&mut self, decoded_image: &mut Image) -> anyhow::Result<Option<Prediction>> {
        let orig_h = decoded_image.height;
        let orig_w = decoded_image.width;

        self.resizer
            .resize_image(decoded_image, &mut self.resized_image)?;

        for (i, &pixel) in self.resized_image.pixels.iter().enumerate() {
            let c = i % 3;
            let pixel_idx = i / 3;
            let row = pixel_idx / 512;
            let col = pixel_idx % 512;
            self.input[[0, row, col, c]] = pixel as f32 / 255.0;
        }

        let inference_start_time = Instant::now();
        let outputs: SessionOutputs = self.session.run(inputs!["input" => self.input.view()]?)?;
        info!("Inference time: {:?}", inference_start_time.elapsed());

        let y: ArrayView<f32, _> = outputs["concatenate_1"].try_extract_tensor::<f32>()?;
        let y = y.index_axis(Axis(0), 0);

        // The output shape is (M, N, 8), where:
        // - The first 2 channels represent the probabilities for each cell.
        // - The next 6 channels contain the affine transformation values for each cell.
        // The probabilities help identify the cell most likely to contain the license plate.
        // The affine values are used to compute the coordinates of the license plate.

        // The code is adapted from the original implementation and a pytorch rewrite:
        //  <https://github.com/sergiomsilva/alpr-unconstrained>
        //  <https://github.com/Pandede/WPODNet-Pytorch>

        // This is the probabilities for each cell
        let probs = y.slice(s![.., .., 0]); // shape: (24, 32)
                                            // Not sure what [.., .., 1] is used for?
                                            // This is 6 affine values for each cell
        let affines = y.slice(s![.., .., 2..]); // shape: (24, 32, 6)

        // Find the maximum probability and its corresponding index
        // Assuming there is only one car with one license plate,
        // we select the highest probability
        let (max_probs, (anchor_y, anchor_x)) = {
            let mut max_idx = (0, 0);
            let mut max_probs = std::f32::NEG_INFINITY;
            for (y_idx, row) in probs.outer_iter().enumerate() {
                for (x_idx, &val) in row.iter().enumerate() {
                    if val > probs[max_idx] {
                        max_idx = (y_idx, x_idx);
                        max_probs = val;
                    }
                }
            }
            (max_probs, max_idx)
        };

        if max_probs < self.confidence_threshold {
            // We could not find any licence plate with a high enough confidence
            return Ok(None);
        }

        // Variable names are funky because they are taken from the original implementation.
        let a = affines.slice(s![anchor_y, anchor_x, ..]).to_owned();
        let mut a = a.to_shape((2, 3)).unwrap();
        a[[0, 0]] = a[[0, 0]].max(0.0);
        a[[1, 1]] = a[[1, 1]].max(0.0);
        let pts = a.dot(&self.q);
        const NET_STRIDE: f32 = 16.0; // 2^4
        const SIDE: f32 = ((208. + 40.) / 2.) / NET_STRIDE; // 7.75
        const SCALING_CONST: f32 = 1.0;
        let points_mn_center_mn = pts * SIDE * SCALING_CONST;
        let points_mn =
            points_mn_center_mn + array![[anchor_x as f32 + 0.5], [anchor_y as f32 + 0.5]];
        let grid_h = affines.shape()[0] as f32;
        let grid_w = affines.shape()[1] as f32;
        let points_prop = points_mn / array![[grid_w], [grid_h]];

        let license_plate_points = LicensePlatePoints::new(&points_prop, orig_w as f32, orig_h as f32);
        let top_left_corner = license_plate_points.get_top_left();
        let bottom_right_corner = license_plate_points.get_bottom_right();

        let coeffs = license_plate_points.get_perspective_coeffs().ok_or(anyhow!("Failed to compute perspective coefficients"))?;

        let x_min = top_left_corner[0];
        let y_min = top_left_corner[1] ;
        let x_max = bottom_right_corner[0] ;
        let y_max = bottom_right_corner[1] ;

        // Adust the max up a little bit to make sure we get the whole plate
        let ocr_x_max = (x_max-x_min as f32) * 1.1; // TODO: these might need to be configurable
        let ocr_y_max = (y_max-y_min as f32) * 1.5; // TODO: these might need to be configurable
        decoded_image.apply_perspective_transform(coeffs, 0 as f32, 0 as f32, ocr_x_max, ocr_y_max, &mut self.ocr_image);

        // TODO: Should be done at init
        let model = SessionBuilder::new()?.commit_from_file(PathBuf::from(r"c:\\git\blue-onyx\paddle_ocr_rec.onnx"))?;

        let w = self.ocr_image.width;
        let h = self.ocr_image.width;

        // Since w is dynamic, we need a new resizer for the OCR image
        let mut ocr_resizer: Resizer = Resizer::new(w, 48)?;
        ocr_resizer.resize_image(&mut self.ocr_image, &mut self.resized_ocr_image)?;

        let mut ocr_input: ndarray::ArrayBase<ndarray::OwnedRepr<f32>, ndarray::Dim<[usize; 4]>> = Array::zeros((1, 3, 48, w as usize));
        for (i, &pixel) in self.resized_ocr_image.pixels.iter().enumerate() {
            let c = i % 3;
            let pixel_idx = i / 3;
            let row = pixel_idx / w;
            let col = pixel_idx % w;
            ocr_input[[0, c, row, col]] = pixel as f32 / 255.0;
        }

        let inference_start_time = Instant::now();
        let outputs: SessionOutputs = model.run(inputs!["x" => ocr_input.view()]?)?;
        info!("Inference time: {:?}", inference_start_time.elapsed());

        let output = outputs.iter().next().unwrap().1;
        let output = output.try_extract_tensor::<f32>()?;
        let output = output.view();
        let output = output.slice(s![0, .., ..]);
        let output: Vec<_> = output.axis_iter(Axis(0))
            .filter_map(|x| {
                x.iter().copied().enumerate().max_by(|(_, x), (_, y)|{
                    x.total_cmp(y)
                })
            })
            .filter(|(index, score)|{
                *index != 0 
            })
            .collect();

        let ocr_jpeg_file = Some("ocr.jpeg".to_string()).unwrap();
        encode_maybe_draw_boundary_boxes_and_save_jpeg(&self.ocr_image, &ocr_jpeg_file, None).unwrap();
        let ocr_resized_jpeg_file = Some("ocr_resize.jpeg".to_string()).unwrap();
        encode_maybe_draw_boundary_boxes_and_save_jpeg(&self.resized_ocr_image, &ocr_resized_jpeg_file, None).unwrap();
        println!("OCR output: {:?}", output);

        let license_plate = Prediction {
            label: "license_plate".to_owned(),
            confidence: max_probs,
            x_min: x_min as usize,
            y_min: y_min as usize,
            x_max: x_max as usize,
            y_max: y_max as usize,
        };
        Ok(Some(license_plate))

    }
}

struct LicensePlatePoints {
    top_left: [f32; 2],
    top_right: [f32; 2],
    bottom_right: [f32; 2],
    bottom_left: [f32; 2],
}

impl LicensePlatePoints {
    fn new(points_prop: &ArrayBase<OwnedRepr<f32>, Dim<[usize; 2]>>, orig_w: f32, orig_h: f32) -> Self {
        let tl = [points_prop[[0, 0]] * orig_w, points_prop[[1, 0]] * orig_h];
        let tr = [points_prop[[0, 1]] * orig_w, points_prop[[1, 1]] * orig_h];
        let br = [points_prop[[0, 2]] * orig_w, points_prop[[1, 2]] * orig_h];
        let bl = [points_prop[[0, 3]] * orig_w, points_prop[[1, 3]] * orig_h];
        Self {
            top_left: tl,
            top_right: tr,
            bottom_right: br,
            bottom_left: bl,
        }
    }

    fn get_top_left(&self) -> [f32; 2] {
        self.top_left
    }

    fn get_bottom_right(&self) -> [f32; 2] {
        self.bottom_right
    }

    /// Computes the perspective transformation coefficients that map the source points
    /// to automatically estimated horizontal destination points while preserving height and length.
    ///
    /// # Returns
    ///
    /// * `Option<[f32; 9]>` - An array of 9 coefficients if successful, otherwise `None`.
    fn get_perspective_coeffs(&self) -> Option<[f32; 9]> {
        let src = [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ];

        let dest = self.estimate_destination_points(&src)?;

        let mut a_matrix = SMatrix::<f32, 8, 8>::zeros();
        let mut b_vector = SVector::<f32, 8>::zeros();

        for i in 0..4 {
            let (x, y) = (src[i][0], src[i][1]);
            let (x_prime, y_prime) = (dest[i][0], dest[i][1]);

            a_matrix[(2 * i, 0)] = x;
            a_matrix[(2 * i, 1)] = y;
            a_matrix[(2 * i, 2)] = 1.0;
            a_matrix[(2 * i, 6)] = -x * x_prime;
            a_matrix[(2 * i, 7)] = -y * x_prime;
            b_vector[2 * i] = x_prime;

            a_matrix[(2 * i + 1, 3)] = x;
            a_matrix[(2 * i + 1, 4)] = y;
            a_matrix[(2 * i + 1, 5)] = 1.0;
            a_matrix[(2 * i + 1, 6)] = -x * y_prime;
            a_matrix[(2 * i + 1, 7)] = -y * y_prime;
            b_vector[2 * i + 1] = y_prime;
        }

        let lu = a_matrix.lu();
        if let Some(solution) = lu.solve(&b_vector) {
            let mut coeffs_array = [1.0_f32; 9];
            coeffs_array[..8].copy_from_slice(&solution.as_slice());
            Some(coeffs_array)
        } else {
            None
        }
    }

    fn estimate_destination_points(&self, src: &[[f32; 2]; 4]) -> Option<[[f32; 2]; 4]> {
        let width_top = distance(src[0], src[1]);
        let width_bottom = distance(src[3], src[2]);
        let avg_width = (width_top + width_bottom) / 2.0;

        let height_left = distance(src[0], src[3]);
        let height_right = distance(src[1], src[2]);
        let avg_height = (height_left + height_right) / 2.0;

        let dest = [
            [0.0, 0.0],                        // Destination Top Left
            [avg_width, 0.0],                  // Destination Top Right
            [avg_width, avg_height],           // Destination Bottom Right
            [0.0, avg_height],                  // Destination Bottom Left
        ];

        Some(dest)
    }
}

fn distance(p1: [f32; 2], p2: [f32; 2]) -> f32 {
    ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt()
}
