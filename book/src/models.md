# Models

Blue Onyx supports multiple object detection models from different sources. You can download and use various pre-trained models depending on your specific use case.

## Automatic Model Management

Blue Onyx automatically manages models with the following behavior:

1. **Default Model**: If no model is specified, `rt-detrv2-s.onnx` is used as the default
2. **Auto-Download**: Missing models and their corresponding YAML files are automatically downloaded when needed
3. **YAML Validation**: Both model (.onnx) and metadata (.yaml) files are required and verified before use
4. **Error Handling**: Clear error messages if models cannot be downloaded or loaded
5. **First Run**: On first startup, Blue Onyx will automatically download the default model if not present

This means you can start using Blue Onyx immediately without manually downloading models - the system will handle it automatically.

## Available Model Types

Blue Onyx supports two main categories of models:

1. **RT-DETR v2 Models** - General-purpose object detection models
2. **YOLO5 Specialized Models** - IP camera and delivery-focused models

## Downloading Models

### CLI Download Options

You can download models using the following CLI commands:

```bash
# List all available models
blue_onyx --list-models

# Download all models to binary directory (simplest)
blue_onyx --download-all-models

# Download only RT-DETR v2 models to binary directory
blue_onyx --download-rt-detr2

# Download only YOLO5 specialized models to binary directory
blue_onyx --download-yolo5

# Download to a specific directory
blue_onyx --download-all-models --download-model-path ./models
blue_onyx --download-rt-detr2 --download-model-path ./models
blue_onyx --download-yolo5 --download-model-path ./models

# Download both RT-DETR and YOLO5 (equivalent to --download-all-models)
blue_onyx --download-rt-detr2 --download-yolo5
```

**Note**: `--download-model-path` is **optional** and specifies where to download. If not provided, models are downloaded to the directory where the Blue Onyx binary is located.

### Download Behavior

The download logic works as follows:

1. **`--download-all-models`** - Downloads all available models (RT-DETR v2 + YOLO5)
2. **`--download-rt-detr2 --download-yolo5`** - Downloads all models (same as above)
3. **`--download-rt-detr2`** - Downloads only RT-DETR v2 models
4. **`--download-yolo5`** - Downloads only YOLO5 specialized models
5. **`--download-model-path` alone** - Does **nothing** (you must specify what to download)

**Default Location**: If no `--download-model-path` is specified, models are downloaded to the same directory as the Blue Onyx binary.

## Model Details

### RT-DETR v2 Models

RT-DETR v2 (Real-Time Detection Transformer) models are general-purpose object detection models trained on the COCO dataset. These models offer excellent performance for detecting common objects.

| Model Name | Size | Description | Classes | Source |
|------------|------|-------------|---------|--------|
| rt-detrv2-s | ~80MB | Small variant - fastest inference | 80 COCO classes | [RT-DETR](https://github.com/lyuwenyu/RT-DETR) |
| rt-detrv2-ms | ~120MB | Medium-small variant - balanced speed/accuracy | 80 COCO classes | [RT-DETR](https://github.com/lyuwenyu/RT-DETR) |
| rt-detrv2-m | ~200MB | Medium variant - good balance | 80 COCO classes | [RT-DETR](https://github.com/lyuwenyu/RT-DETR) |
| rt-detrv2-l | ~300MB | Large variant - higher accuracy | 80 COCO classes | [RT-DETR](https://github.com/lyuwenyu/RT-DETR) |
| rt-detrv2-x | ~400MB | Extra large variant - highest accuracy | 80 COCO classes | [RT-DETR](https://github.com/lyuwenyu/RT-DETR) |

**Default Model**: `rt-detrv2-s.onnx` is used as the default model when no specific model is specified.

### YOLO5 Specialized Models

These are specialized YOLO5 models designed for specific use cases, particularly IP cameras and delivery scenarios.

| Model Name | Size | Description | Specialized For | Source |
|------------|------|-------------|-----------------|--------|
| delivery | ~25MB | Package and delivery detection | Delivery trucks, packages, postal workers | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| IPcam-animal | ~25MB | Animal detection for IP cameras | Animals, pets, wildlife | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| ipcam-bird | ~25MB | Bird detection for IP cameras | Birds, flying objects | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| IPcam-combined | ~25MB | Combined detection for IP cameras | Multiple object types optimized for cameras | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| IPcam-dark | ~25MB | Low-light detection for IP cameras | Objects in dark/night conditions | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| IPcam-general | ~25MB | General purpose IP camera detection | General objects optimized for IP cameras | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |
| package | ~25MB | Package detection | Packages, boxes, deliveries | [CodeProject.AI Custom IPcam Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models) |

## Model Sources and References

### RT-DETR
- **Repository**: [lyuwenyu/RT-DETR](https://github.com/lyuwenyu/RT-DETR)
- **Download Source**: [xnorpx/rt-detr2-onnx](https://huggingface.co/xnorpx/rt-detr2-onnx)
- **Paper**: "DETRs Beat YOLOs on Real-time Object Detection"
- **License**: Apache 2.0
- **Description**: RT-DETR is a real-time object detector that efficiently processes images by eliminating NMS (Non-Maximum Suppression) and using transformer architecture.

### YOLO5 Specialized Models
- **Repository**: [MikeLud/CodeProject.AI-Custom-IPcam-Models](https://github.com/MikeLud/CodeProject.AI-Custom-IPcam-Models)
- **Base Framework**: [ultralytics/yolov5](https://github.com/ultralytics/yolov5)
- **Download Source**: [xnorpx/blue-onyx-yolo5](https://huggingface.co/xnorpx/blue-onyx-yolo5)
- **License**: AGPL-3.0
- **Description**: Custom trained YOLO5 models specifically optimized for IP camera scenarios and delivery detection.

> **⚠️ IMPORTANT LICENSING NOTE**: YOLO5 models are licensed under AGPL-3.0, which **prohibits commercial use** without proper licensing. If your use case does not satisfy the AGPL-3.0 license requirements (e.g., commercial/proprietary applications), you must obtain a commercial license from [Ultralytics](https://ultralytics.com/license). For commercial applications, consider using RT-DETR models instead, which are licensed under Apache 2.0.

## Using Models

### Specifying a Model

You can specify which model to use with the `--model` parameter:

```bash
# Use a specific RT-DETR model
blue_onyx --model ./models/rt-detrv2-l.onnx

# Use a specialized YOLO5 model
blue_onyx --model ./models/delivery.onnx --object-detection-model-type yolo5
```

### Model Requirements

Each model requires two files:
- **`.onnx` file**: The actual model weights and architecture
- **`.yaml` file**: Model metadata including class names and configuration

Both files are automatically downloaded when using the download commands.

### Performance Considerations

- **RT-DETR Models**: Better for general object detection, more accurate on diverse scenes
- **YOLO5 Specialized Models**: Faster inference, optimized for specific scenarios
- **Size vs. Accuracy**: Larger models generally provide better accuracy but slower inference
- **Hardware**: GPU acceleration is recommended for larger models

## Troubleshooting

### Common Issues

- **404 Errors**: Some model files may have naming inconsistencies in the repository
- **Network Issues**: Download failures due to connectivity problems
- **Disk Space**: Ensure sufficient disk space for model downloads
- **Permissions**: Verify write permissions in the target directory

### Verification

You can verify downloaded models by checking the file sizes match the expected values in the table above.