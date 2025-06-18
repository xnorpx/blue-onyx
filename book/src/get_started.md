# Getting Started

This guide will help you get Blue Onyx up and running quickly with automatic model downloads.

## Quick Start

### Windows

1. **Download and Install** (one-liner):
   ```powershell
   powershell -NoProfile -Command "curl 'https://github.com/xnorpx/blue-onyx/releases/latest/download/install_latest_blue_onyx.ps1' -o 'install_latest_blue_onyx.ps1'; Unblock-File '.\install_latest_blue_onyx.ps1'; powershell.exe -ExecutionPolicy Bypass -File '.\install_latest_blue_onyx.ps1'"
   ```

2. **Run Blue Onyx**:
   ```powershell
   blue_onyx
   ```

   On first run, Blue Onyx will automatically download the default model (`rt-detrv2-s.onnx`) and start the service on port 32168.

3. **Test the Service**:
   Open your browser and go to: `http://127.0.0.1:32168/`

### Linux (Docker)

```bash
# Pull and run the Docker container
docker pull ghcr.io/xnorpx/blue_onyx:latest
docker run -p 32168:32168 ghcr.io/xnorpx/blue_onyx:latest
```

## Model Management

Blue Onyx automatically manages models for you, but you can also download them manually for better control.

### Automatic Model Download

By default, Blue Onyx will:
- Use `rt-detrv2-s.onnx` as the default model
- Automatically download the model and its YAML file on first run
- Download to the current working directory or specify with `--model`

### Manual Model Download

To download models ahead of time or to a specific location:

```bash
# Download all available models to binary directory (simplest)
blue_onyx --download-all-models

# Download only RT-DETR v2 models to binary directory (recommended for general use)
blue_onyx --download-rt-detr2

# Download only YOLO5 specialized models to binary directory (for IP cameras/delivery)
blue_onyx --download-yolo5

# Download to a specific directory
blue_onyx --download-all-models --download-model-path ./models
blue_onyx --download-rt-detr2 --download-model-path ./models

# List all available models
blue_onyx --list-models
```

### Using a Specific Model

```bash
# Use a larger RT-DETR model for better accuracy
blue_onyx --model ./models/rt-detrv2-l.onnx

# Use a specialized YOLO5 model for delivery detection
blue_onyx --model ./models/delivery.onnx --object-detection-model-type yolo5
```

## Configuration

### Basic Configuration

```bash
# Run on a different port
blue_onyx --port 8080

# Increase confidence threshold (fewer, more confident detections)
blue_onyx --confidence_threshold 0.7

# Force CPU usage (disable GPU acceleration)
blue_onyx --force_cpu

# Enable debug logging
blue_onyx --log_level Debug
```

### Configuration File

For complex setups, use a JSON configuration file:

**config.json:**
```json
{
  "port": 8080,
  "confidence_threshold": 0.7,
  "model": "./models/rt-detrv2-l.onnx",
  "log_level": "Info",
  "force_cpu": false,
  "object_filter": ["person", "car", "bicycle"]
}
```

**Run with config:**
```bash
blue_onyx --config config.json
```

## Testing Object Detection

### Using the Web Interface

1. Open `http://127.0.0.1:32168/` in your browser
2. Upload an image or use the test endpoint
3. View detection results with bounding boxes

### Using curl

```bash
# Test with an image file
curl -X POST -F "image=@test_image.jpg" http://127.0.0.1:32168/detect

# Test with a URL
curl -X POST -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/image.jpg"}' \
  http://127.0.0.1:32168/detect
```

## Next Steps

- **[Models](models.md)** - Learn about available models and their use cases
- **[Configuration](configuration.md)** - Detailed configuration options
- **[Windows Service](windows_service.md)** - Run Blue Onyx as a Windows service
- **[Integration](configure_blue_iris_5.md)** - Integrate with Blue Iris 5

## Troubleshooting

### Common Issues

**Service won't start:**
- Check if the port (32168) is already in use
- Verify the model files are downloaded correctly
- Check the logs for error messages

**Poor detection performance:**
- Try a larger model (e.g., `rt-detrv2-l.onnx`)
- Adjust the confidence threshold
- Ensure GPU acceleration is working if available

**Model download failures:**
- Check internet connectivity
- Verify disk space is available
- Try downloading to a different directory

**GPU not being used:**
- Check that GPU drivers are installed
- Verify that the GPU supports the required compute capabilities
- Try setting `--gpu_index` to a different value if multiple GPUs are present

For more detailed troubleshooting, see the [FAQ](faq.md).