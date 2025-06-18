# Binaries

Blue Onyx provides several different binaries for different use cases. Each binary is designed for specific scenarios and usage patterns.

## Available Binaries

### blue_onyx.exe / blue_onyx

The main Blue Onyx server application.

**Purpose**: Primary HTTP server for object detection
**Use Case**: Development, testing, and standalone deployment
**Platform**: Windows (.exe) and Linux

**Features**:
- HTTP API server for object detection
- Model management and automatic downloading
- Web interface for testing
- Configuration via CLI or JSON file
- Real-time object detection via REST API

**Example Usage**:
```bash
# Start with default settings
blue_onyx

# Start with custom configuration
blue_onyx --port 8080 --confidence_threshold 0.7

# Download models before starting
blue_onyx --download-model-path ./models --download-rt-detr2
```

### blue_onyx_service.exe

Windows service-specific binary.

**Purpose**: Run Blue Onyx as a Windows service
**Use Case**: Production deployments on Windows servers
**Platform**: Windows only

**Features**:
- Designed to run as a Windows service
- Automatic startup on boot
- Service management integration
- Background operation without user session

**Example Usage**:
```powershell
# Install as Windows service
sc.exe create blue_onyx_service binPath= "C:\Program Files\BlueOnyx\blue_onyx_service.exe --port 32168" start= auto displayname= "Blue Onyx Service"

# Start the service
net start blue_onyx_service
```

### blue_onyx_benchmark.exe / blue_onyx_benchmark

Performance benchmarking tool.

**Purpose**: Benchmark model performance and system capabilities
**Use Case**: Performance testing, model comparison, hardware evaluation
**Platform**: Windows (.exe) and Linux

**Features**:
- Model performance benchmarking
- Hardware utilization testing
- Inference speed measurement
- Memory usage analysis
- GPU vs CPU performance comparison

**Example Usage**:
```bash
# Benchmark default model
blue_onyx_benchmark

# Benchmark specific model
blue_onyx_benchmark --model ./models/rt-detrv2-l.onnx

# Benchmark with specific settings
blue_onyx_benchmark --model ./models/rt-detrv2-s.onnx --force_cpu
```

### test_blue_onyx.exe / test_blue_onyx

Testing and validation utility.

**Purpose**: Validate Blue Onyx installation and functionality
**Use Case**: Installation verification, debugging, CI/CD testing
**Platform**: Windows (.exe) and Linux

**Prerequisites**: **Requires a running Blue Onyx instance** to test against (starts blue_onyx server first)

**Features**:
- Installation validation
- Model loading tests
- API endpoint testing (tests live HTTP endpoints)
- Configuration validation
- System requirements checking
- Performance testing with live requests

**Example Usage**:
```bash
# Start Blue Onyx server first
blue_onyx &

# Then run all tests against the running instance
test_blue_onyx

# Test specific functionality
test_blue_onyx --test model_loading

# Test with custom configuration (server must be running with same config)
test_blue_onyx --config ./test_config.json
```

## Binary Comparison

| Binary | Purpose | Use Case | Web UI | Service Mode | Benchmarking |
|--------|---------|----------|--------|--------------|--------------|
| blue_onyx | Main server | Development/Standalone | ✅ | ❌ | ❌ |
| blue_onyx_service | Windows service | Production (Windows) | ✅ | ✅ | ❌ |
| blue_onyx_benchmark | Performance testing | Benchmarking | ❌ | ❌ | ✅ |
| test_blue_onyx | Testing/validation | Debugging/CI | ❌ | ❌ | ❌ |

## Download and Installation

### Pre-built Binaries

Download pre-built binaries from the [releases page](https://github.com/xnorpx/blue-onyx/releases):

**Windows (x86_64)**:
- `blue_onyx-windows-x86_64.zip` - Contains all Windows binaries

**Linux (Docker)**:
- Use the Docker image: `ghcr.io/xnorpx/blue_onyx:latest`

### Building from Source

```bash
# Clone the repository
git clone https://github.com/xnorpx/blue-onyx.git
cd blue-onyx

# Build all binaries
cargo build --release

# Build specific binary
cargo build --release --bin blue_onyx
cargo build --release --bin blue_onyx_service
cargo build --release --bin blue_onyx_benchmark
cargo build --release --bin test_blue_onyx
```

Binaries will be available in `target/release/`.

## Usage Scenarios

### Development and Testing

For development, debugging, and testing:

```bash
# Start main server for development
blue_onyx --port 8080 --log_level Debug

# In another terminal, run tests against the running server
test_blue_onyx

# Benchmark performance during development
blue_onyx_benchmark --model ./models/rt-detrv2-s.onnx
```

### Production Deployment (Windows)

For production Windows servers:

```powershell
# Install as service
sc.exe create blue_onyx_service binPath= "C:\Program Files\BlueOnyx\blue_onyx_service.exe --config C:\Program Files\BlueOnyx\production.json" start= auto displayname= "Blue Onyx Service"

# Start service
net start blue_onyx_service

# Verify with test tool
test_blue_onyx --config C:\Program Files\BlueOnyx\production.json
```

### Production Deployment (Linux)

For production Linux servers (using Docker):

```bash
# Deploy with Docker Compose
docker-compose up -d

# Test the deployment
docker run --rm --network host ghcr.io/xnorpx/blue_onyx:latest test_blue_onyx
```

### Performance Evaluation

For evaluating different models and hardware configurations:

```bash
# Compare model performance
blue_onyx_benchmark --model ./models/rt-detrv2-s.onnx > benchmark_small.txt
blue_onyx_benchmark --model ./models/rt-detrv2-l.onnx > benchmark_large.txt

# Test GPU vs CPU performance
blue_onyx_benchmark --model ./models/rt-detrv2-m.onnx --force_cpu > benchmark_cpu.txt
blue_onyx_benchmark --model ./models/rt-detrv2-m.onnx > benchmark_gpu.txt
```

## Command Line Options

### Common Options (All Binaries)

Most binaries support these common options:

```bash
--help                    # Display help information
--version                 # Display version information
--config <file>           # Use JSON configuration file
--log_level <level>       # Set logging level (Error, Warn, Info, Debug, Trace)
--model <path>            # Specify model file path
```

### Server-Specific Options (blue_onyx, blue_onyx_service)

```bash
--port <port>             # HTTP server port (default: 32168)
--confidence_threshold    # Detection confidence threshold (default: 0.5)
--force_cpu              # Disable GPU acceleration
--request_timeout        # API request timeout
--worker_queue_size      # Worker queue size
```

### Download Options (blue_onyx)

```bash
--download-model-path <dir>    # Download models to directory
--download-all-models          # Download all available models
--download-rt-detr2           # Download RT-DETR v2 models
--download-yolo5              # Download YOLO5 specialized models
--list-models                 # List all available models
```

### Benchmark Options (blue_onyx_benchmark)

```bash
--iterations <n>          # Number of benchmark iterations
--warmup <n>             # Number of warmup iterations
--output <file>          # Save benchmark results to file
--detailed               # Show detailed per-iteration results
```

## File Locations

### Windows

Default installation paths for Windows:

```
C:\Program Files\BlueOnyx\
├── blue_onyx.exe
├── blue_onyx_service.exe
├── blue_onyx_benchmark.exe
├── test_blue_onyx.exe
├── models\
├── config\
└── logs\
```

User-specific paths:
```
%USERPROFILE%\.blue-onyx\
├── models\
├── config\
└── cache\
```

### Linux (Docker)

Container paths:

```
/app/
├── blue_onyx
├── blue_onyx_benchmark
├── test_blue_onyx
├── models/
├── config/
└── logs/
```

Host-mounted volumes:
```
~/blue-onyx/
├── models/
├── config/
├── logs/
└── processed_images/
```

## Troubleshooting

### Binary Won't Start

**Check dependencies**:
```bash
# Windows: Check for missing DLLs
dumpbin /dependents blue_onyx.exe

# Linux: Check shared libraries
ldd blue_onyx
```

**Verify file permissions**:
```bash
# Ensure executable permissions
chmod +x blue_onyx
```

### Performance Issues

**Use benchmark tool**:
```bash
# Identify performance bottlenecks
blue_onyx_benchmark --detailed --model ./models/rt-detrv2-s.onnx
```

**Check resource usage**:
```bash
# Monitor while running
top -p $(pgrep blue_onyx)
```

### Service Issues (Windows)

**Check service status**:
```powershell
# View service details
sc.exe query blue_onyx_service

# Check service logs
Get-EventLog -LogName Application -Source "blue_onyx_service"
```

### Validation Issues

**Run tests**:
```bash
# Validate installation
test_blue_onyx --verbose

# Test specific configuration
test_blue_onyx --config ./test_config.json
```

## Next Steps

- **[Getting Started](get_started.md)** - Learn how to use the main blue_onyx binary
- **[Windows Service](windows_service.md)** - Set up blue_onyx_service for production
- **[Benchmark](benchmark.md)** - Use blue_onyx_benchmark for performance testing
- **[Configuration](configuration.md)** - Configure binaries for your use case
