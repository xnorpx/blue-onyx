# Blue Onyx Service Installation Script
# Run this script as Administrator after installing Blue Onyx

Write-Host "Setting up Blue Onyx Service..." -ForegroundColor Green

# Set service timeout to 10 minutes for model loading
Write-Host "Setting service timeout to 10 minutes..." -ForegroundColor Yellow
reg add "HKLM\SYSTEM\CurrentControlSet\Control" /v ServicesPipeTimeout /t REG_DWORD /d 600000 /f

# Create event log source
Write-Host "Creating event log source..." -ForegroundColor Yellow
try {
    New-EventLog -LogName Application -Source BlueOnyxService -ErrorAction SilentlyContinue
    Write-Host "Event log source created successfully" -ForegroundColor Green
} catch {
    Write-Host "Event log source may already exist or could not be created" -ForegroundColor Yellow
}

# Get the installation directory (assume we're in the scripts subdirectory)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$installDir = Split-Path -Parent $scriptDir

if (-Not (Test-Path "$installDir\blue_onyx_service.exe")) {
    # Try a few common installation locations
    $commonPaths = @(
        "${env:ProgramFiles}\Blue Onyx",
        "${env:ProgramFiles}\blue-onyx",
        "${env:ProgramFiles(x86)}\Blue Onyx",
        "${env:ProgramFiles(x86)}\blue-onyx"
    )

    $found = $false
    foreach ($path in $commonPaths) {
        if (Test-Path "$path\blue_onyx_service.exe") {
            $installDir = $path
            $found = $true
            break
        }
    }

    if (-not $found) {
        Write-Host "Installation directory not found automatically." -ForegroundColor Red
        Write-Host "Please specify the correct path to blue_onyx_service.exe" -ForegroundColor Red
        $servicePath = Read-Host "Enter the full path to blue_onyx_service.exe"
        $installDir = Split-Path -Parent $servicePath
    }
}

$servicePath = "$installDir\blue_onyx_service.exe"

# Install the service
Write-Host "Installing Blue Onyx Service..." -ForegroundColor Yellow
sc.exe create BlueOnyxService binPath= "`"$servicePath`"" start= auto displayname= "Blue Onyx Service" obj= LocalSystem

if ($LASTEXITCODE -eq 0) {
    Write-Host "Service created successfully" -ForegroundColor Green

    # Configure service type
    Write-Host "Configuring service type..." -ForegroundColor Yellow
    sc.exe config BlueOnyxService type= own

    if ($LASTEXITCODE -eq 0) {
        Write-Host "Service configured successfully" -ForegroundColor Green

        # Ask user if they want to start the service now
        $start = Read-Host "Do you want to start the Blue Onyx Service now? (y/n)"
        if ($start -eq "y" -or $start -eq "Y") {
            Write-Host "Starting Blue Onyx Service..." -ForegroundColor Yellow
            net start BlueOnyxService
            if ($LASTEXITCODE -eq 0) {
                Write-Host "Blue Onyx Service started successfully!" -ForegroundColor Green
            } else {
                Write-Host "Failed to start the service. Check the configuration and logs." -ForegroundColor Red
            }
        } else {
            Write-Host "Service installed but not started. You can start it later with: net start BlueOnyxService" -ForegroundColor Yellow
        }
    } else {
        Write-Host "Failed to configure service type" -ForegroundColor Red
    }
} else {
    Write-Host "Failed to create service. Error code: $LASTEXITCODE" -ForegroundColor Red
    Write-Host "Make sure you are running as Administrator" -ForegroundColor Yellow
}

Write-Host "Blue Onyx Service setup completed!" -ForegroundColor Green
Write-Host ""
Write-Host "Service Management Commands:" -ForegroundColor Cyan
Write-Host "  Start service:     net start BlueOnyxService" -ForegroundColor White
Write-Host "  Stop service:      net stop BlueOnyxService" -ForegroundColor White
Write-Host "  Uninstall service: sc.exe delete BlueOnyxService" -ForegroundColor White

pause
