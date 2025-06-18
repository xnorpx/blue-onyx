# Configuration

Blue Onyx supports both command-line arguments and JSON configuration files, making it easy to manage complex configurations and deploy consistent settings across different environments.

## Command Line vs Configuration Files

Blue Onyx offers two distinct configuration modes:

### Command Line Arguments
Use traditional command-line arguments for quick setup and testing:

```bash
blue_onyx --port 8080 --confidence_threshold 0.7 --log_level Debug
```

### Configuration File
Create a JSON configuration file for persistent, complete settings:

```bash
blue_onyx --config production.json
```

**Important**: When using a configuration file, it completely replaces command-line defaults. You cannot mix config files with additional CLI arguments - it's either one or the other for clarity and simplicity.

## Configuration Behavior Examples

### Using CLI Arguments Only
```bash
# All settings via command line
blue_onyx --port 8080 --confidence_threshold 0.7 --log_level Debug --save_image_path "C:\Temp\images"
```

### Using Configuration File Only
```bash
# All settings from config file, no additional CLI arguments allowed
blue_onyx --config my_settings.json
```

### Invalid: Mixing Config File and CLI Arguments
```bash
# ❌ This won't work - you can't mix config file with other arguments
blue_onyx --config my_settings.json --port 9090
```

The above command will load all settings from `my_settings.json` and ignore the `--port 9090` argument entirely.

## Configuration File Format

All command-line options are available in the JSON configuration format. Here are platform-specific examples:

### Windows Configuration Example
```json
{
  "port": 8080,
  "request_timeout": 30,
  "worker_queue_size": 10,
  "model": "C:\\BlueOnyx\\Models\\custom-model.onnx",
  "object_detection_model_type": "RtDetrv2",
  "object_classes": "C:\\BlueOnyx\\Config\\custom-classes.yaml",
  "object_filter": ["person", "car", "bicycle"],
  "log_level": "Info",
  "log_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx.log",
  "confidence_threshold": 0.7,
  "force_cpu": false,
  "intra_threads": 4,
  "inter_threads": 2,
  "save_image_path": "C:\\Temp\\processed_images",
  "save_ref_image": true,
  "gpu_index": 0,
  "save_stats_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx-stats.json"
}
```

### Linux/macOS Configuration Example
```json
{
  "port": 8080,
  "request_timeout": 30,
  "worker_queue_size": 10,
  "model": "/opt/blue-onyx/models/custom-model.onnx",
  "object_detection_model_type": "RtDetrv2",
  "object_classes": "/etc/blue-onyx/custom-classes.yaml",
  "object_filter": ["person", "car", "bicycle"],
  "log_level": "Info",
  "log_path": "/var/log/blue-onyx.log",
  "confidence_threshold": 0.7,
  "force_cpu": false,
  "intra_threads": 4,
  "inter_threads": 2,
  "save_image_path": "/tmp/processed_images",
  "save_ref_image": true,
  "gpu_index": 0,
  "save_stats_path": "/var/log/blue-onyx-stats.json"
}
```

## Example Configuration Files

Blue Onyx includes platform-specific example configuration files:

- **Windows**: Use `blue_onyx_config_service_example.json` as a template for the service
- **Linux/macOS**: Use `blue_onyx_config_example_nix.json` as a template for Unix systems

## Configuration Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `port` | number | 32168 | HTTP server port |
| `request_timeout` | number | 15 | Timeout in seconds for detection requests |
| `worker_queue_size` | number | auto | Queue size for detection workers |
| `model` | string | auto | Path to ONNX model file (auto-downloads rt-detrv2-s.onnx if not specified) |
| `object_detection_model_type` | string | "RtDetrv2" | Model type: "RtDetrv2" or "Yolo5" |
| `object_classes` | string | auto | Path to YAML file with object classes (auto-downloaded with model) |
| `confidence_threshold` | number | 0.5 | Minimum confidence for detections |
| `object_filter` | array | [] | Filter results to specific object types |
| `log_level` | string | "Info" | Logging level: Trace, Debug, Info, Warn, Error |
| `log_path` | string | null | Path to log file (if not set, logs to stdout) |
| `force_cpu` | boolean | false | Force CPU inference (disable GPU) |
| `gpu_index` | number | 0 | GPU device index to use |
| `intra_threads` | number | 192/2 | Intra-op thread count (Windows: 192, Linux: 2) |
| `inter_threads` | number | 192/2 | Inter-op thread count (Windows: 192, Linux: 2) |
| `save_image_path` | string | null | Directory to save processed images |
| `save_ref_image` | boolean | false | Save reference images alongside processed ones |
| `save_stats_path` | string | null | Path to save inference statistics |

## Model Download Options (CLI Only)

These options are only available via command line and are used for model management:

| Option | Type | Description |
|--------|------|-------------|
| `--list-models` | boolean | List all available models and exit |

**Example:**
```bash
# List available models
blue_onyx --list-models
```

**Note**: For model download options, see the [Models](models.md) section which covers downloading and managing models in detail.
blue_onyx --download-model-path ./models --download-all-models

# Download only RT-DETR models
blue_onyx --download-model-path ./models --download-rt-detr2

# List available models
blue_onyx --list-models
```

**Note**: Download operations exit after completion and do not start the server.

## Environment-Specific Configurations

### Development Configuration
Perfect for local development and debugging:

**Windows:**
```json
{
  "port": 3000,
  "log_level": "Debug",
  "save_image_path": "C:\\Temp\\debug_images",
  "save_ref_image": true,
  "confidence_threshold": 0.3
}
```

**Linux/macOS:**
```json
{
  "port": 3000,
  "log_level": "Debug",
  "save_image_path": "/tmp/debug_images",
  "save_ref_image": true,
  "confidence_threshold": 0.3
}
```

### Production Configuration
Optimized for production deployments:

**Windows:**
```json
{
  "port": 80,
  "log_level": "Warn",
  "log_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx.log",
  "confidence_threshold": 0.8,
  "worker_queue_size": 50,
  "save_stats_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx-stats.json"
}
```

**Linux/macOS:**
```json
{
  "port": 80,
  "log_level": "Warn",
  "log_path": "/var/log/blue-onyx.log",
  "confidence_threshold": 0.8,
  "worker_queue_size": 50,
  "save_stats_path": "/var/log/blue-onyx-stats.json"
}
```

### High-Performance Configuration
For maximum throughput on powerful hardware:

```json
{
  "port": 32168,
  "confidence_threshold": 0.6,
  "worker_queue_size": 100,
  "intra_threads": 8,
  "inter_threads": 4,
  "gpu_index": 0
}
```

## Automatic Configuration Management

### Auto-Save for Standalone Binary
When running the standalone `blue_onyx` binary without a config file, it automatically saves your current settings to `blue_onyx_config.json` next to the executable. This makes it easy to capture your working configuration for future use.

## Windows Service Configuration

The Blue Onyx Windows service uses a dedicated configuration approach that differs from the standalone binary:

### Service Configuration File
- **Location**: `blue_onyx_config_service.json` (same directory as executable)
- **Auto-creation**: Created with default values if it doesn't exist
- **No CLI arguments**: Service configuration is entirely file-based

### Service Installation
Install the service without any command-line arguments:

```cmd
sc.exe create blue_onyx_service binPath= "C:\path\to\blue_onyx_service.exe" start= auto displayname= "Blue Onyx Service"
```

### Service Configuration Example
Edit `blue_onyx_config_service.json` to configure the service:

```json
{
  "port": 32168,
  "log_level": "Info",
  "log_path": null,
  "confidence_threshold": 0.5,
  "force_cpu": false,
  "worker_queue_size": 20,
  "save_stats_path": "C:\\ProgramData\\blue_onyx_service\\stats.json"
}
```

### Service Logging
If `log_path` is not specified, the service automatically uses `%PROGRAMDATA%\blue_onyx_service` for log files.

## Best Practices

### Configuration Management
1. **Version control**: Store configuration files in version control
2. **Environment separation**: Use different config files for dev/staging/production
3. **Secrets management**: Keep sensitive data in environment variables or secure vaults
4. **Documentation**: Comment complex configurations and maintain examples

### Performance Tuning
1. **Thread counts**: Start with defaults, then tune based on your hardware
2. **Queue sizes**: Monitor queue depth and adjust for your workload
3. **Confidence thresholds**: Balance accuracy vs. detection sensitivity
4. **GPU selection**: Use `gpu_index` to select the optimal GPU on multi-GPU systems

### Security Considerations
1. **Port binding**: Bind to specific interfaces in production environments
2. **Log paths**: Ensure log directories have appropriate permissions
3. **File permissions**: Restrict access to configuration files containing sensitive settings
4. **Network access**: Consider firewall rules for the configured port

## Troubleshooting

### Configuration Loading Issues
- Verify JSON syntax using a JSON validator
- Check file permissions and paths
- Review log output for detailed error messages
- Use `--help` to see all available configuration options

### Service Configuration Problems
- Ensure `blue_onyx_config_service.json` is in the same directory as the executable
- Check Windows Event Viewer for service-specific errors
- Verify that the service has write permissions to create log files
- Test configuration with the standalone binary first
