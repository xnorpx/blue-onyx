# Frequently Asked Questions

## General Questions

### What is Blue Onyx?

Blue Onyx is a reliable object detection service written in Rust using the ONNX runtime. It's designed to be simple, robust, and performant for local object detection needs, particularly for security cameras and automated monitoring systems.

### What makes Blue Onyx different from other object detection services?

- **Rust-based**: Built for reliability and performance
- **ONNX Runtime**: Industry-standard inference engine
- **Automatic Model Management**: Downloads and manages models automatically
- **Multiple Model Support**: RT-DETR v2 and specialized YOLO5 models
- **Simple Design**: Focused on object detection without feature creep
- **Cross-platform**: Windows and Linux support

### Is Blue Onyx free to use?

Yes, Blue Onyx is open source and free to use. It's licensed under [Apache 2.0](https://github.com/xnorpx/blue-onyx/blob/main/LICENSE).

## Installation and Setup

### How do I install Blue Onyx?

**Windows**: Use the one-line PowerShell installer:
```powershell
powershell -NoProfile -Command "curl 'https://github.com/xnorpx/blue-onyx/releases/latest/download/install_latest_blue_onyx.ps1' -o 'install_latest_blue_onyx.ps1'; Unblock-File '.\install_latest_blue_onyx.ps1'; powershell.exe -ExecutionPolicy Bypass -File '.\install_latest_blue_onyx.ps1'"
```

**Linux**: Use Docker:
```bash
docker pull ghcr.io/xnorpx/blue_onyx:latest
docker run -p 32168:32168 ghcr.io/xnorpx/blue_onyx:latest
```

### What are the system requirements?

**Minimum**:
- Windows 10 x64 or Linux x64
- 4GB RAM
- 2GB disk space (for models)
- Internet connection (for model downloads)

**Recommended**:
- 8GB+ RAM
- Dedicated GPU (NVIDIA, AMD, or Intel)
- SSD storage
- Multi-core CPU

### Do I need to download models manually?

No! Blue Onyx automatically downloads models when needed. However, you can pre-download them:

```bash
# Download all models
blue_onyx --download-model-path ./models --download-all-models

# Download only RT-DETR models
blue_onyx --download-model-path ./models --download-rt-detr2
```

## Models and Performance

### Which model should I use?

**For general use**: `rt-detrv2-s.onnx` (default) - good balance of speed and accuracy

**For higher accuracy**: `rt-detrv2-l.onnx` or `rt-detrv2-x.onnx` - slower but more accurate

**For specialized scenarios**:
- `delivery.onnx` - package and delivery detection
- `IPcam-animal.onnx` - animal detection
- `IPcam-dark.onnx` - low-light conditions

### How do I check available models?

```bash
blue_onyx --list-models
```

### My GPU isn't being used. How do I fix this?

1. **Check GPU drivers** are up to date
2. **Verify GPU support**: Not all GPUs support ONNX acceleration
3. **Try different GPU index**: `--gpu_index 1` (if multiple GPUs)
4. **Check compute capability**: NVIDIA GPUs need compute capability 6.0+
5. **Force CPU if needed**: `--force_cpu` for troubleshooting

### How do I improve detection performance?

**For speed**:
- Use smaller models (`rt-detrv2-s.onnx`)
- Enable GPU acceleration
- Reduce image resolution before sending
- Lower confidence threshold if getting too few detections

**For accuracy**:
- Use larger models (`rt-detrv2-l.onnx`, `rt-detrv2-x.onnx`)
- Increase confidence threshold
- Use appropriate specialized models for your use case

## Configuration and Usage

### How do I change the default port?

```bash
# Command line
blue_onyx --port 8080

# Configuration file
{
  "port": 8080
}
```

### Can I filter for specific objects only?

Yes, use the `object_filter` parameter:

```bash
# Command line
blue_onyx --object_filter person,car,bicycle

# Configuration file
{
  "object_filter": ["person", "car", "bicycle"]
}
```

### How do I save processed images?

```bash
blue_onyx --save_image_path ./processed_images --save_ref_image
```

### Can I use Blue Onyx with Blue Iris?

Yes! See the [Blue Iris Integration Guide](configure_blue_iris_5.md) for detailed setup instructions.

### How do I run Blue Onyx as a Windows service?

```powershell
# Create service
sc.exe create blue_onyx_service binPath= "C:\Program Files\BlueOnyx\blue_onyx_service.exe --port 32168" start= auto displayname= "Blue Onyx Service"

# Start service
net start blue_onyx_service
```

## Troubleshooting

### Blue Onyx won't start

**Check port availability**:
```bash
# Windows
netstat -ano | findstr :32168

# Linux
sudo netstat -tlpn | grep :32168
```

**Check logs**:
```bash
blue_onyx --log_level Debug
```

**Verify model files**:
```bash
blue_onyx --list-models
```

### Getting 404 errors during model download

Some model files may have naming inconsistencies in the repository. The download will continue with available files. You can:

1. Try downloading again later
2. Use alternative models
3. Check the [models page](models.md) for known issues

### Object detection is slow

**Check system resources**:
- Monitor CPU/GPU usage
- Ensure sufficient RAM
- Check if GPU acceleration is working

**Optimize settings**:
- Use a smaller model
- Reduce thread counts on Linux: `--intra_threads 2 --inter_threads 2`
- Enable GPU if available

### Getting poor detection results

**Adjust confidence threshold**:
```bash
# Lower for more detections (may include false positives)
blue_onyx --confidence_threshold 0.3

# Higher for fewer, more confident detections
blue_onyx --confidence_threshold 0.7
```

**Try a different model**:
- Larger RT-DETR models for better accuracy
- Specialized YOLO5 models for specific scenarios

**Check image quality**:
- Ensure images are clear and well-lit
- Avoid heavily compressed images
- Consider image preprocessing

### Docker container issues

**Container won't start**:
```bash
# Check logs
docker logs blue-onyx

# Check if port is in use
sudo netstat -tlpn | grep :32168
```

**Volume mount issues**:
```bash
# Check permissions
ls -la ~/blue-onyx/

# Fix permissions
sudo chown -R 1000:1000 ~/blue-onyx/
```

### High memory usage

**Normal behavior**: Model loading requires significant memory (2-8GB depending on model)

**Reduce memory usage**:
- Use smaller models
- Close other applications
- Monitor with `top` or Task Manager

### Windows service won't start

**Check service configuration**:
```powershell
sc.exe query blue_onyx_service
```

**Check event logs**:
```powershell
Get-EventLog -LogName Application -Source "blue_onyx_service"
```

**Verify file paths**: Ensure all paths in service configuration are absolute and accessible

## API and Integration

### What API endpoints are available?

- `POST /detect` - Detect objects in images
- `GET /` - Web interface
- `GET /stats` - Service statistics
- `GET /test` - Test endpoint

### How do I send images for detection?

**Upload file**:
```bash
curl -X POST -F "image=@photo.jpg" http://localhost:32168/detect
```

**Send URL**:
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/image.jpg"}' \
  http://localhost:32168/detect
```

### What response format does the API use?

JSON format with detected objects:

```json
{
  "success": true,
  "predictions": [
    {
      "label": "person",
      "confidence": 0.95,
      "x_min": 100,
      "y_min": 50,
      "x_max": 200,
      "y_max": 300
    }
  ],
  "inference_time": 45.2,
  "image_id": "abc123"
}
```

### Can I integrate with other systems?

Yes, Blue Onyx provides a standard REST API that can integrate with:
- Security camera systems (Blue Iris, Agent DVR)
- Home automation systems (Home Assistant)
- Custom applications
- Monitoring systems

## Advanced Topics

### Can I train custom models?

Blue Onyx uses pre-trained ONNX models. To use custom models:

1. Train your model in a supported framework (PyTorch, TensorFlow)
2. Convert to ONNX format
3. Create a corresponding YAML file with class names
4. Use with `--model` parameter

### How do I benchmark performance?

Use the benchmark tool:

```bash
blue_onyx_benchmark --model ./models/rt-detrv2-s.onnx --iterations 100
```

### Can I run multiple instances?

Yes, run on different ports:

```bash
# Instance 1
blue_onyx --port 32168 --model ./models/rt-detrv2-s.onnx

# Instance 2
blue_onyx --port 32169 --model ./models/delivery.onnx
```

### How do I update Blue Onyx?

**Windows**: Re-run the installation script

**Linux**: Pull the latest Docker image:
```bash
docker pull ghcr.io/xnorpx/blue_onyx:latest
```

### Is there a roadmap for future features?

Check the [GitHub repository](https://github.com/xnorpx/blue-onyx) for:
- Current issues and feature requests
- Development roadmap
- Contribution guidelines

## Support and Community

### Where can I get help?

- **Documentation**: This book covers most common scenarios
- **GitHub Issues**: [Report bugs or request features](https://github.com/xnorpx/blue-onyx/issues)
- **Discussions**: Community discussions on GitHub

### How can I contribute?

- Report bugs or suggest features
- Contribute to documentation
- Submit pull requests for improvements
- Share your use cases and configurations

### Where are the source code and releases?

- **Source**: [GitHub Repository](https://github.com/xnorpx/blue-onyx)
- **Releases**: [GitHub Releases](https://github.com/xnorpx/blue-onyx/releases)
- **Docker Images**: [GitHub Container Registry](https://ghcr.io/xnorpx/blue_onyx)
