# Windows Installation

This guide covers installing Blue Onyx on Windows, including both standalone usage and Windows service installation.

## Prerequisites

- Windows 10 or later (x86_64)
- PowerShell (pre-installed on Windows 10+)
- Internet connection for downloading Blue Onyx and models

### Optional: GPU Acceleration

For GPU acceleration, ensure you have compatible drivers:
- **NVIDIA GPU**: Latest GeForce or Quadro drivers
- **AMD GPU**: Latest Radeon drivers
- **Intel GPU**: Latest Intel Graphics drivers

## Installation Methods

### Method 1: Windows Installer (Recommended)

The easiest way to install Blue Onyx is using the official Windows installer:

1. **Download the Installer**:
   - Go to [Blue Onyx Releases](https://github.com/xnorpx/blue-onyx/releases)
   - Download `blue_onyx-X.Y.Z-installer.exe` (where X.Y.Z is the version number)

2. **Run the Installer**:
   - Right-click the installer and select "Run as administrator"
   - Windows will show a UAC prompt - click "Yes" to proceed
   - Follow the installation wizard

3. **What's Included**:
   The installer includes:
   - **Binaries:**
     - `blue_onyx.exe` - Main Blue Onyx application
     - `blue_onyx_service.exe` - Windows service for Blue Onyx
     - `blue_onyx_benchmark.exe` - Performance benchmarking tool
     - `test_blue_onyx.exe` - Testing utilities
   - **Required DLLs:**
     - `DirectML.dll` - DirectML library for GPU acceleration
     - `onnxruntime.dll` - ONNX Runtime library
   - **Scripts:**
     - `install_service.ps1` - Install Blue Onyx as a Windows service
     - `uninstall_service.ps1` - Remove the Windows service
     - `windows_event_logs_to_txt.ps1` - Export Windows event logs

4. **Smart Installation Features**:
   - Automatically checks for existing Blue Onyx Service installation
   - Stops the service if it's currently running before updating files
   - Installs to `C:\Program Files\blue-onyx` by default
   - Requests administrator privileges automatically

**Note:** If you're upgrading an existing installation, the installer will automatically stop the running service before updating the files.

### Method 2: One-Line Installation Script

This is the fastest way to get Blue Onyx running:

```powershell
powershell -NoProfile -Command "curl 'https://github.com/xnorpx/blue-onyx/releases/latest/download/install_latest_blue_onyx.ps1' -o 'install_latest_blue_onyx.ps1'; Unblock-File '.\install_latest_blue_onyx.ps1'; powershell.exe -ExecutionPolicy Bypass -File '.\install_latest_blue_onyx.ps1'"
```

This script will:
- Download the latest Blue Onyx release
- Extract it to `%USERPROFILE%\.blue-onyx\`
- Add the directory to your PATH
- Download default models

### Method 3: Manual Installation

1. **Download the Latest Release**:
   - Go to [Blue Onyx Releases](https://github.com/xnorpx/blue-onyx/releases)
   - Download `blue_onyx-windows-x86_64.zip`

2. **Extract the Archive**:
   ```powershell
   # Extract to a permanent location
   Expand-Archive -Path "blue_onyx-windows-x86_64.zip" -DestinationPath "C:\Program Files\BlueOnyx"
   ```

3. **Add to PATH** (optional):
   ```powershell
   # Add to user PATH
   $env:PATH += ";C:\Program Files\BlueOnyx"

   # Or add permanently via System Properties > Environment Variables
   ```

## First Run

### Download Models

Before running Blue Onyx, download the required models:

```powershell
# Navigate to installation directory
cd "C:\Program Files\BlueOnyx"

# Download all models to a models subfolder
.\blue_onyx.exe --download-model-path .\models --download-all-models

# Or download only the default RT-DETR models
.\blue_onyx.exe --download-model-path .\models --download-rt-detr2
```

### Start Blue Onyx

```powershell
# Run with default settings
.\blue_onyx.exe

# Or specify the downloaded models
.\blue_onyx.exe --model .\models\rt-detrv2-s.onnx
```

The service will start on `http://127.0.0.1:32168` by default.

## Install as Windows Service

For production use, install Blue Onyx as a Windows service to start automatically.

### Step 1: Run as Administrator

Open PowerShell as Administrator (required for service installation).

### Step 2: Create the Service

```powershell
# Navigate to installation directory
cd "C:\Program Files\BlueOnyx"

# Create service with basic configuration
sc.exe create blue_onyx_service binPath= "C:\Program Files\BlueOnyx\blue_onyx_service.exe --port 32168" start= auto displayname= "Blue Onyx Service"
```

### Step 3: Configure Service (Optional)

For custom configuration, create a JSON config file:

**C:\Program Files\BlueOnyx\service_config.json:**
```json
{
  "port": 32168,
  "model": "C:\\Program Files\\BlueOnyx\\models\\rt-detrv2-s.onnx",
  "confidence_threshold": 0.5,
  "log_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx.log",
  "save_stats_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx-stats.json",
  "force_cpu": false
}
```

Then update the service to use the config:

```powershell
# Update service to use configuration file
sc.exe config blue_onyx_service binPath= "C:\Program Files\BlueOnyx\blue_onyx_service.exe --config C:\Program Files\BlueOnyx\service_config.json"
```

### Step 4: Start the Service

```powershell
# Start the service
net start blue_onyx_service

# Or use services.msc to manage the service
services.msc
```

### Step 5: Verify Installation

1. Open your browser and go to: `http://127.0.0.1:32168/`
2. You should see the Blue Onyx web interface
3. Check Windows Event Viewer for any service errors

## Service Management

### Common Service Commands

```powershell
# Start service
net start blue_onyx_service

# Stop service
net stop blue_onyx_service

# Restart service
net stop blue_onyx_service && net start blue_onyx_service

# Delete service (if needed)
net stop blue_onyx_service
sc.exe delete blue_onyx_service
```

### View Service Logs

If logging to file is configured, check the log files:

```powershell
# View recent logs
Get-Content "C:\ProgramData\BlueOnyx\blue-onyx.log" -Tail 50

# Monitor logs in real-time
Get-Content "C:\ProgramData\BlueOnyx\blue-onyx.log" -Wait -Tail 10
```

## Configuration Examples

### Basic Home Security Setup

```json
{
  "port": 32168,
  "model": "C:\\Program Files\\BlueOnyx\\models\\rt-detrv2-s.onnx",
  "confidence_threshold": 0.6,
  "object_filter": ["person", "car", "bicycle", "motorcycle"],
  "log_level": "Info",
  "force_cpu": false
}
```

### High-Performance Setup (GPU Required)

```json
{
  "port": 32168,
  "model": "C:\\Program Files\\BlueOnyx\\models\\rt-detrv2-l.onnx",
  "confidence_threshold": 0.5,
  "force_cpu": false,
  "gpu_index": 0,
  "intra_threads": 4,
  "inter_threads": 2,
  "save_image_path": "C:\\ProgramData\\BlueOnyx\\processed_images",
  "save_stats_path": "C:\\ProgramData\\BlueOnyx\\blue-onyx-stats.json"
}
```

### Delivery Detection Setup

```json
{
  "port": 32168,
  "model": "C:\\Program Files\\BlueOnyx\\models\\delivery.onnx",
  "object_detection_model_type": "Yolo5",
  "confidence_threshold": 0.7,
  "log_level": "Info"
}
```

## Troubleshooting

### Installation Issues

**PowerShell Execution Policy Error:**
```powershell
# Fix execution policy
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

**Download Failures:**
- Check internet connectivity
- Try running PowerShell as Administrator
- Disable antivirus temporarily during installation

### Service Issues

**Service Won't Start:**
1. Check Windows Event Viewer (Windows Logs > Application)
2. Verify file paths in service configuration
3. Ensure models are downloaded correctly
4. Check port availability

**Port Already in Use:**
```powershell
# Find what's using port 32168
netstat -ano | findstr :32168

# Kill the process if needed (replace PID)
taskkill /PID 1234 /F
```

### Performance Issues

**GPU Not Being Used:**
1. Check GPU drivers are up to date
2. Verify GPU supports required compute capabilities
3. Try different `gpu_index` values
4. Check Windows Device Manager for GPU status

**High CPU Usage:**
1. Try a smaller model (rt-detrv2-s instead of rt-detrv2-l)
2. Reduce `intra_threads` and `inter_threads`
3. Consider using `force_cpu: false` to enable GPU

## Uninstallation

### Remove Service

```powershell
# Stop and delete service
net stop blue_onyx_service
sc.exe delete blue_onyx_service
```

### Remove Files

```powershell
# Remove installation directory
Remove-Item -Recurse -Force "C:\Program Files\BlueOnyx"

# Remove user data (optional)
Remove-Item -Recurse -Force "$env:USERPROFILE\.blue-onyx"

# Remove program data (optional)
Remove-Item -Recurse -Force "C:\ProgramData\BlueOnyx"
```

### Remove from PATH

Remove the Blue Onyx directory from your PATH environment variable via:
- System Properties > Environment Variables, or
- Edit the PATH variable directly

## Next Steps

- **[Getting Started](get_started.md)** - Basic usage and testing
- **[Configuration](configuration.md)** - Detailed configuration options
- **[Models](models.md)** - Choose the right model for your use case
- **[Blue Iris Integration](configure_blue_iris_5.md)** - Integrate with Blue Iris 5
