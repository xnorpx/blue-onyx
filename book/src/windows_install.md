# Windows Installation

Blue Onyx provides multiple installation methods for Windows, with the Windows installer being the recommended approach for most users.

## Windows Installer (Recommended)

The Windows installer provides a complete, professional installation experience with automatic Windows service setup.

### Features

The Windows installer includes:

- **Complete Installation**: All Blue Onyx executables and dependencies
- **Windows Service Integration**: Automatically installs and starts Blue Onyx as a Windows service
- **Model Download**: Downloads required AI models during installation
- **Shortcuts**: Creates desktop and start menu shortcuts for easy access
- **PATH Integration**: Adds Blue Onyx to system PATH for command-line access
- **Clean Uninstall**: Proper removal of service, shortcuts, and files

### Download and Install

1. **Download**: Get the latest installer from [GitHub Releases](https://github.com/xnorpx/blue-onyx/releases)
2. **Run as Administrator**: Right-click the installer and select "Run as administrator"
3. **Follow the wizard**: Select the components you want to install
4. **Access the service**: Navigate to http://127.0.0.1:32168 after installation

### Installation Components

During installation, you can select:

- **Blue Onyx Core** (required)
  - All executables and dependencies
  - Configuration files and AI models
  - Documentation

- **Blue Onyx Service** (recommended)
  - Windows service installation
  - Automatic startup configuration
  - Service management shortcuts

- **Desktop Shortcuts**
  - Quick access shortcuts on desktop
  - Server start/stop shortcuts
  - Benchmark and test utilities

- **Start Menu Shortcuts**
  - Program group in start menu
  - Service management tools
  - Web interface shortcut

### What Gets Installed

The installer includes these executables:

| Executable | Purpose |
|------------|---------|
| `blue_onyx.exe` | Main server application |
| `blue_onyx_service.exe` | Windows service wrapper |
| `blue_onyx_benchmark.exe` | Performance benchmarking tool |
| `blue_onyx_download_models.exe` | Model download utility |
| `test_blue_onyx.exe` | Server testing utility |

### Service Configuration

The Windows service is configured with:

- **Service Name**: `blue_onyx_service`
- **Display Name**: "Blue Onyx Object Detection Service"
- **Start Type**: Automatic (starts with Windows)
- **Port**: 32168 (default)
- **Dependencies**: TCP/IP

### Post-Installation

After installation:

1. **Service Status**: The service starts automatically
2. **Web Interface**: Access at http://127.0.0.1:32168
3. **Shortcuts**: Use desktop or start menu shortcuts
4. **Command Line**: Blue Onyx is added to your PATH

## Manual Installation

For advanced users who prefer manual installation:

### Download Binaries

1. Download the Windows ZIP file from [GitHub Releases](https://github.com/xnorpx/blue-onyx/releases)
2. Extract to your preferred directory (e.g., `C:\blue-onyx`)
3. Add the directory to your system PATH

### Manual Service Installation

Create the Windows service manually:

```powershell
# Create the service
sc.exe create blue_onyx_service binPath= "C:\blue-onyx\blue_onyx_service.exe --port 32168" start= auto displayname= "Blue Onyx Object Detection Service"

# Start the service
net start blue_onyx_service
```

### Download Models

Download the required AI models:

```powershell
# Download models to the installation directory
blue_onyx_download_models.exe
```

## Service Management

### Using Services Manager

1. Press `Win + R`, type `services.msc`, press Enter
2. Find "Blue Onyx Object Detection Service"
3. Right-click for start/stop/restart options

### Using Command Line

```powershell
# Start service
net start blue_onyx_service

# Stop service
net stop blue_onyx_service

# Check service status
sc.exe query blue_onyx_service
```

### Using Start Menu

Navigate to Start Menu → Blue Onyx → Service Manager

## Troubleshooting

### Service Won't Start

1. **Check Event Viewer**: Look for service-specific errors
2. **Port Conflicts**: Ensure port 32168 is available
3. **Permissions**: Verify the service has proper permissions
4. **Firewall**: Check Windows Firewall settings

### Models Not Downloading

1. **Internet Connection**: Verify network connectivity
2. **Firewall**: Allow Blue Onyx through firewall
3. **Manual Download**: Run `blue_onyx_download_models.exe` manually

### Permission Denied Errors

1. **Run as Administrator**: Ensure installer runs with admin privileges
2. **Antivirus**: Check if antivirus is blocking installation
3. **User Account Control**: Confirm UAC prompts

### Web Interface Not Accessible

1. **Service Status**: Verify service is running
2. **Port Check**: Ensure port 32168 is not blocked
3. **Browser**: Try different browser or incognito mode
4. **Localhost**: Try http://localhost:32168 instead

## Uninstallation

### Using Control Panel

1. Open "Add or Remove Programs"
2. Find "Blue Onyx Object Detection Service"
3. Click "Uninstall"
4. Choose whether to keep or remove models

### Manual Uninstallation

If needed, remove manually:

```powershell
# Stop and remove service
net stop blue_onyx_service
sc.exe delete blue_onyx_service

# Remove from PATH (manual registry edit required)
# Remove installation directory
```

## Building the Installer

For developers who want to build the installer from source:

### Prerequisites

- Rust toolchain (latest stable)
- NSIS (Nullsoft Scriptable Install System)
- Windows SDK (recommended)

### Build Process

```powershell
# Install cargo-packager
cargo install cargo-packager --locked

# Build release binaries
cargo build --release --bins

# Create installer
cargo packager --release
```

### Using Build Scripts

```powershell
# PowerShell script
.\build_installer.ps1 -Release

# Or batch file
build_installer.bat
```

The installer will be created in `target\packager\release\`.

### Configuration

The installer is configured via `Packager.toml` in the project root. Key settings include:

- Product metadata and branding
- Installation components
- Service configuration
- NSIS script customization

## Advanced Configuration

### Custom Port

To run the service on a different port:

1. Stop the service: `net stop blue_onyx_service`
2. Modify service configuration:
   ```powershell
   sc.exe config blue_onyx_service binPath= "C:\blue-onyx\blue_onyx_service.exe --port 8080"
   ```
3. Start the service: `net start blue_onyx_service`

### Custom Models

To use custom AI models:

1. Place model files in the installation directory
2. Update configuration or use command-line arguments
3. Restart the service

### Logging Configuration

Service logs are stored in:
- `%PROGRAMDATA%\blue_onyx_service\` (default)
- Or custom path specified during installation

## Security Considerations

### Firewall

The installer may prompt to allow Blue Onyx through Windows Firewall. This is required for:
- Web interface access
- API endpoints
- Service communication

### User Privileges

The service runs with:
- Local Service account (default)
- Minimal required privileges
- No network access beyond required ports

### Network Access

Blue Onyx requires internet access for:
- Initial model downloads
- Model updates (optional)
- Version checking (optional)

## Performance Optimization

### GPU Acceleration

Blue Onyx automatically detects and uses available GPU acceleration:
- Intel GPUs (DirectML)
- AMD GPUs (DirectML)
- NVIDIA GPUs (DirectML)

### Memory Usage

Monitor memory usage in Task Manager. Typical usage:
- Base service: 100-200 MB
- With models loaded: 500 MB - 2 GB (depending on model size)

### CPU Usage

CPU usage varies by:
- Inference frequency
- Model complexity
- Image resolution
- Available GPU acceleration
