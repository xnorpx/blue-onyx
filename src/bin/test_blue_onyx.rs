use blue_onyx::{DOG_BIKE_CAR_BYTES, api::VisionDetectionResponse};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{Body, Client, multipart};
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};

// Simple test client to send multiple requests to blue onyx service for testing
#[derive(Parser)]
#[command(author = "Marcus Asteborg", version=env!("CARGO_PKG_VERSION"), about = "TODO")]
struct Args {
    /// Origin for the requests
    #[clap(short, long, default_value = "http://127.0.0.1:32168")]
    origin: String,

    /// Min confidence
    #[arg(long, default_value_t = 0.60)]
    pub min_confidence: f32,

    /// Optional image input path
    #[clap(short, long)]
    image: Option<String>,

    /// Number of requests to make
    #[clap(short, long, default_value_t = 1)]
    number_of_requests: u32,

    /// Interval in milliseconds for making requests
    #[clap(long, default_value_t = 1000)]
    interval: u64,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let mut futures = Vec::with_capacity(args.number_of_requests as usize);

    let pb = ProgressBar::new(args.number_of_requests as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );

    println!(
        "Calling {}, {} times with {} ms interval",
        args.origin, args.number_of_requests, args.interval
    );

    let start_time = Instant::now();
    for i in 0..args.number_of_requests {
        let image = args.image.clone();
        let origin = args.origin.clone();
        let min_confidence = args.min_confidence;
        futures.push(tokio::task::spawn(send_vision_detection_request(
            origin,
            image,
            min_confidence,
        )));
        pb.inc(1);
        if i < args.number_of_requests - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(args.interval)).await;
        }
    }
    let results = futures::future::join_all(futures).await;
    pb.finish_with_message("All requests completed!");
    let runtime_duration = Instant::now().duration_since(start_time);
    let mut request_times: Vec<Duration> = Vec::with_capacity(args.number_of_requests as usize);
    let mut inference_times: Vec<i32> = Vec::with_capacity(args.number_of_requests as usize);
    let mut processing_times: Vec<i32> = Vec::with_capacity(args.number_of_requests as usize);

    let mut vision_detection_response = VisionDetectionResponse::default();
    results.into_iter().for_each(|result| {
        if let Ok(Ok(result)) = result {
            vision_detection_response = result.0;
            inference_times.push(vision_detection_response.inferenceMs);
            processing_times.push(vision_detection_response.processMs);
            request_times.push(result.1);
        }
    });

    assert!(inference_times.len() == args.number_of_requests as usize);
    println!("{:#?}", vision_detection_response);

    println!("Runtime duration: {:?}", runtime_duration);
    if !request_times.is_empty() {
        let min_duration = request_times.iter().min().unwrap();
        let max_duration = request_times.iter().max().unwrap();
        let avg_duration = request_times.iter().sum::<Duration>() / request_times.len() as u32;

        println!(
            "Request times -- min: {:?}, avg: {:?}, max: {:?}",
            min_duration, avg_duration, max_duration
        );
    } else {
        println!("No request times to summarize");
    }

    if !inference_times.is_empty() {
        let min_inference = inference_times.iter().min().unwrap();
        let max_inference = inference_times.iter().max().unwrap();
        let avg_inference = inference_times.iter().sum::<i32>() / inference_times.len() as i32;

        println!(
            "Inference times -- min: {}, avg: {}, max: {}",
            min_inference, avg_inference, max_inference
        );
    } else {
        println!("No inference times to summarize");
    }

    if !processing_times.is_empty() {
        let min_processing = processing_times.iter().min().unwrap();
        let max_processing = processing_times.iter().max().unwrap();
        let avg_processing = processing_times.iter().sum::<i32>() / processing_times.len() as i32;

        println!(
            "Processing times -- min: {}, avg: {}, max: {}",
            min_processing, avg_processing, max_processing
        );
    } else {
        println!("No processing times to summarize");
    }

    Ok(())
}

async fn send_vision_detection_request(
    origin: String,
    image: Option<String>,
    min_confidence: f32,
) -> anyhow::Result<(VisionDetectionResponse, Duration)> {
    let url = reqwest::Url::parse(&origin)?.join("v1/vision/detection")?;
    let client = Client::new();

    let image_part = if let Some(image) = image {
        let file = File::open(image).await?;
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        multipart::Part::stream(body).file_name("image.jpg")
    } else {
        multipart::Part::bytes(DOG_BIKE_CAR_BYTES.to_vec()).file_name("image.jpg")
    };

    let form = multipart::Form::new()
        .text("min_confidence", min_confidence.to_string())
        .part("image", image_part);

    let request_start_time = Instant::now();
    let response = match client.post(url).multipart(form).send().await {
        Ok(resp) => resp,
        Err(e) => {
            eprintln!("Request send error: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };
    if !response.status().is_success() {
        let status = response.status();
        let body = match response.text().await {
            Ok(text) => text,
            Err(e) => {
                eprintln!("Failed to read response body: {}", e);
                return Err(anyhow::anyhow!(e));
            }
        };
        eprintln!("Error: Status: {}, Body: {}", status, body);
        return Err(anyhow::anyhow!("Request failed with status {}", status));
    }
    let response = match response.json::<VisionDetectionResponse>().await {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to parse JSON: {}", e);
            return Err(anyhow::anyhow!(e));
        }
    };

    Ok((response, Instant::now().duration_since(request_start_time)))
}
