# build_installer.ps1
# Script to build the Blue Onyx Windows installer using cargo-packager

param(
    [switch]$Release = $false,
    [switch]$Clean = $false,
    [string]$OutputDir = "target\packager"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Status {
    param([string]$Message)
    Write-Host "`n[INFO] $Message" -ForegroundColor Green
}

function Write-Error {
    param([string]$Message)
    Write-Host "`n[ERROR] $Message" -ForegroundColor Red
}

try {
    Write-Status "Building Blue Onyx Windows Installer"

    # Check if cargo-packager is installed
    Write-Status "Checking for cargo-packager..."
    $cargoPackagerCheck = cargo packager --version 2>$null
    if ($LASTEXITCODE -ne 0) {
        Write-Status "Installing cargo-packager..."
        cargo install cargo-packager --locked
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to install cargo-packager"
        }
    }
    else {
        Write-Host "cargo-packager is already installed: $cargoPackagerCheck"
    }

    # Clean if requested
    if ($Clean) {
        Write-Status "Cleaning previous builds..."
        if (Test-Path "target") {
            Remove-Item "target" -Recurse -Force
        }
        cargo clean
    }

    # Build release binaries first
    Write-Status "Building release binaries..."
    if ($Release) {
        cargo build --release --bins
    }
    else {
        cargo build --bins
    }

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to build binaries"
    }

    # Check if required model file exists, download if needed
    $modelFile = "models\rt-detrv2-s.onnx"
    if (!(Test-Path $modelFile)) {
        Write-Status "Model file not found. Downloading models..."
        if ($Release) {
            & "target\release\blue_onyx_download_models.exe"
        }
        else {
            & "target\debug\blue_onyx_download_models.exe"
        }

        if ($LASTEXITCODE -ne 0) {
            Write-Host "Warning: Could not download models automatically. Models will be downloaded during installation." -ForegroundColor Yellow
        }
    }

    # Create the installer package
    Write-Status "Creating Windows installer package..."

    $packagerArgs = @("packager")
    if ($Release) {
        $packagerArgs += "--release"
    }

    # Add output directory if specified
    if ($OutputDir -ne "target\packager") {
        $packagerArgs += "--out-dir", $OutputDir
    }

    Write-Host "Running: cargo $($packagerArgs -join ' ')"
    & cargo @packagerArgs

    if ($LASTEXITCODE -ne 0) {
        throw "Failed to create installer package"
    }

    # Find the generated installer
    $installerPattern = if ($Release) { "target\packager\release\*.exe" } else { "target\packager\debug\*.exe" }
    $installerFiles = Get-ChildItem -Path $installerPattern -ErrorAction SilentlyContinue

    if ($installerFiles) {
        Write-Status "Installer created successfully!"
        foreach ($file in $installerFiles) {
            Write-Host "  - $($file.FullName)" -ForegroundColor Cyan
            $size = [math]::Round($file.Length / 1MB, 2)
            Write-Host "    Size: $size MB" -ForegroundColor Gray
        }

        Write-Status "Installation Instructions:"
        Write-Host "1. Run the installer as Administrator" -ForegroundColor Yellow
        Write-Host "2. Select the components you want to install" -ForegroundColor Yellow
        Write-Host "3. The installer will:" -ForegroundColor Yellow
        Write-Host "   - Install all Blue Onyx executables" -ForegroundColor Gray
        Write-Host "   - Download AI models (if not present)" -ForegroundColor Gray
        Write-Host "   - Install and start the Windows service" -ForegroundColor Gray
        Write-Host "   - Create desktop and start menu shortcuts" -ForegroundColor Gray
        Write-Host "   - Add Blue Onyx to system PATH" -ForegroundColor Gray
        Write-Host "4. After installation, access the web interface at http://127.0.0.1:32168" -ForegroundColor Yellow

    }
    else {
        Write-Error "Installer was created but could not be found in expected location"
        Write-Host "Check the output directory: $OutputDir" -ForegroundColor Yellow
    }

}
catch {
    Write-Error $_.Exception.Message
    Write-Host "`nBuild failed. Please check the error messages above." -ForegroundColor Red
    exit 1
}

Write-Status "Build completed successfully!"
