# Windows Service Configuration for GPU Access

This guide covers the proper configuration of Blue Onyx as a Windows service with optimal GPU/DirectX 12/DirectML access.

## Quick Installation

Use the provided PowerShell script for automatic installation:

```powershell
# Run as Administrator
.\install_service_with_gpu.ps1
```

## Manual Installation

### Basic Service Installation

```cmd
# Create the service with NetworkService account (recommended for GPU access)
sc.exe create blue_onyx_service binPath= "C:\BlueOnyx\blue_onyx_service.exe" start= auto displayname= "Blue Onyx Service" obj= "NT AUTHORITY\NetworkService"

# Configure for desktop interaction (helps with GPU access)
sc.exe config blue_onyx_service type= own type= interact

# Set service description
sc.exe description blue_onyx_service "Blue Onyx AI Object Detection Service with DirectML GPU acceleration"

# Configure failure recovery
sc.exe failure blue_onyx_service reset= 86400 actions= restart/30000/restart/60000/restart/120000

# Set required privileges for GPU access
sc.exe privs blue_onyx_service SeIncreaseQuotaPrivilege/SeAssignPrimaryTokenPrivilege/SeServiceLogonRight/SeCreateGlobalPrivilege

# Start the service
net start blue_onyx_service
```

## Service Account Options

### NetworkService (Recommended)
- **Best for**: Most installations with GPU access requirements
- **Pros**: Good GPU access, network capabilities, moderate security
- **Account**: `NT AUTHORITY\NetworkService`

### LocalSystem
- **Best for**: Maximum compatibility but reduced security
- **Pros**: Full system access, best compatibility
- **Cons**: Runs with highest privileges, security risk
- **Account**: `LocalSystem`

### LocalService
- **Best for**: Highest security, local-only operations
- **Pros**: Limited privileges, good security
- **Cons**: Limited GPU access
- **Account**: `NT AUTHORITY\LocalService`

## GPU Access Considerations

### Session 0 Isolation
Windows services run in Session 0, which has limited access to graphics subsystems. The service includes:

- **DirectML Detection**: Validates DirectML.dll availability
- **DirectX 12 Validation**: Checks for GPU adapters and DirectX support
- **Environment Variables**: Sets optimal DirectML configuration

### Required Files
Ensure these files are in the service executable directory:
- `blue_onyx_service.exe`
- `DirectML.dll`
- Service configuration file

### GPU Monitoring
Monitor GPU usage to verify DirectML acceleration:
1. Open Task Manager → Performance → GPU
2. Look for "DirectML" or "Compute" activity
3. Check service logs for GPU detection messages

## Configuration

### Service Configuration File
Create `blue_onyx_config_service.json` in the same directory:

```json
{
    "port": 32168,
    "force_cpu": false,
    "gpu_index": 0,
    "log_level": "Info",
    "confidence_threshold": 0.5,
    "model": "C:\\BlueOnyx\\Models\\custom-model.onnx",
    "save_stats_path": "C:\\ProgramData\\BlueOnyx\\service_stats.json"
}
```

### Environment Variables
The service automatically sets:
- `DIRECTML_DEBUG=0`: Disable DirectML debug output
- `D3D12_EXPERIMENTAL_SHADER_MODELS=1`: Enable experimental DirectX features

## Troubleshooting

### GPU Not Detected
1. Verify DirectML.dll is present
2. Check Windows Event Logs for DirectX errors
3. Update GPU drivers
4. Try different service account (NetworkService vs LocalSystem)

### Service Won't Start
1. Check file permissions on service directory
2. Verify service account has required privileges
3. Review service logs in Event Viewer
4. Ensure configuration file is valid JSON

### Poor Performance
1. Verify GPU is being used (Task Manager)
2. Check `force_cpu` setting in configuration
3. Monitor service logs for DirectML initialization
4. Consider increasing `gpu_index` if multiple GPUs present

## Service Management

```cmd
# Start service
net start blue_onyx_service

# Stop service
net stop blue_onyx_service

# Check status
sc.exe query blue_onyx_service

# View service configuration
sc.exe qc blue_onyx_service

# Remove service
sc.exe delete blue_onyx_service
```

## Event Logging

The service logs important events to:
- **Application Event Log**: Service start/stop events
- **Service Logs**: DirectML and GPU detection (if log_path configured)

Check Event Viewer → Windows Logs → Application for service events.

## Security Considerations

1. **Service Account**: Use NetworkService for balanced security and functionality
2. **File Permissions**: Ensure service account has read access to model files
3. **Network Access**: Configure firewall rules for the service port
4. **Privileges**: Service runs with minimal required privileges for GPU access

## Performance Optimization

1. **GPU Selection**: Use `gpu_index` to select optimal GPU in multi-GPU systems
2. **Thread Configuration**: Adjust `intra_threads` and `inter_threads` for CPU fallback
3. **Model Placement**: Store models on fast storage (SSD)
4. **Memory Management**: Monitor memory usage, especially with large models