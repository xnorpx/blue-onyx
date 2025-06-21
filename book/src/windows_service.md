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

## Service Installation After Using Windows Installer

If you installed Blue Onyx using the Windows installer, follow these steps to set up the service:

### Post-Installation Service Setup

1. **Open PowerShell as Administrator**:
   - Right-click on PowerShell and select "Run as Administrator"

2. **Navigate to the Installation Directory**:
   ```powershell
   cd "C:\Program Files\blue-onyx\scripts"
   ```

3. **Run the Service Installation Script**:
   ```powershell
   .\install_service.ps1
   ```

This script will automatically:
- Set service timeout to 10 minutes (for model loading)
- Create event log source for Blue Onyx
- Install the service to run automatically with LocalSystem privileges
- Configure the service properly

### Service Management

After installing the service:

```powershell
# Start the service
net start BlueOnyxService

# Stop the service
net stop BlueOnyxService

# Check service status
sc.exe query BlueOnyxService

# Remove the service (if needed)
.\uninstall_service.ps1
```

### Manual Service Installation (Alternative)

If the automated script doesn't work, you can manually install the service:

```powershell
# Run as Administrator

# 1. Set service timeout (10 minutes for model loading)
reg add "HKLM\SYSTEM\CurrentControlSet\Control" /v ServicesPipeTimeout /t REG_DWORD /d 600000 /f

# 2. Create event log source
New-EventLog -LogName Application -Source BlueOnyxService

# 3. Install the service (replace path as needed)
sc.exe create BlueOnyxService binPath= "C:\Program Files\blue-onyx\blue_onyx_service.exe" start= auto displayname= "Blue Onyx Service" obj= LocalSystem

# 4. Configure service type
sc.exe config BlueOnyxService type= own

# 5. Start the service
net start BlueOnyxService
```

## Advanced Service Configuration

1. **Service Timeout**: Increase timeout for model loading if necessary
   ```powershell
   reg add "HKLM\SYSTEM\CurrentControlSet\Control" /v ServicesPipeTimeout /t REG_DWORD /d 600000 /f
   ```

2. **Event Logging**: Ensure event log source is created
   ```powershell
   New-EventLog -LogName Application -Source BlueOnyxService
   ```

3. **Service Account**: For maximum compatibility, use LocalSystem
   ```cmd
   sc.exe config blue_onyx_service obj= LocalSystem
   ```

4. **Service Type**: Configure service to own process
   ```cmd
   sc.exe config blue_onyx_service type= own
   ```

5. **Start the Service**: After configuration, start the service
   ```cmd
   net start blue_onyx_service
   ```