# install_latest_blue_onyx.ps1
<#
.SYNOPSIS
    Installs (or updates) the latest Blue Onyx release on Windows, overwriting existing files if present,
    downloads all models
.DESCRIPTION
    1. Creates (if needed) a temporary folder in %TEMP% (BlueOnyxInstall).
    2. Always downloads 'version.json' into %TEMP%.
    3. Parses 'version.json' to get the ZIP filename and its .sha256 filename.
    4. If the ZIP and .sha256 files are already found in %TEMP%, skip downloading them (caching).
    5. Verifies the ZIP file's integrity using a SHA256 checksum (via regex).
    6. Extracts the ZIP into a "temp_unzip" folder inside %USERPROFILE%\.blue-onyx, then flattens it if needed.
    7. Copies the new files into %USERPROFILE%\.blue-onyx, overwriting old ones if they exist, without asking permission.
    8. Adds that folder to the PATH
    9. Runs "blue_onyx_download_models.exe --download-model-path" to download all models into .blue-onyx folder.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-ErrorRed {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Message
    )
    Write-Host "`n[ERROR] $Message" -ForegroundColor Red
}

function Write-Green {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Message
    )
    Write-Host "$Message" -ForegroundColor Green
}

try {
    Write-Host "Checking if blue_onyx.exe is running..." -ForegroundColor Yellow
    $processName = "blue_onyx"
    $process = Get-Process -Name $processName -ErrorAction SilentlyContinue

    if ($process) {
        Write-Host "$processName.exe is currently running. Attempting to terminate..."
        Start-Process -FilePath "cmd.exe" -ArgumentList "/c taskkill /F /IM $processName.exe" -Verb RunAs | Wait-Process
        Start-Sleep -Seconds 2
        Write-Host "$processName.exe is not running. Proceeding with installation..." -ForegroundColor Green
    }

    else {
        Write-Host "$processName.exe is not running. Proceeding with installation..." -ForegroundColor Green
    }

    # Prepare download path in %TEMP% ---
    $tempPath = Join-Path $env:TEMP "BlueOnyxInstall"
    if (-not (Test-Path $tempPath)) {
        New-Item -ItemType Directory -Path $tempPath | Out-Null
    }

    $destinationPath = Join-Path $env:USERPROFILE ".blue-onyx"

    if (-not (Test-Path $destinationPath)) {
        # Create .blue-onyx if it doesn't exist
        New-Item -ItemType Directory -Path $destinationPath | Out-Null
    }

    # Download version.json into temporary path
    $versionJsonFile = Join-Path $tempPath "version.json"
    $versionJsonUrl = "https://github.com/xnorpx/blue-onyx/releases/latest/download/version.json"

    Write-Host "Downloading version.json from $versionJsonUrl to $versionJsonFile..." -ForegroundColor Green
    Invoke-WebRequest -Uri $versionJsonUrl -OutFile $versionJsonFile -UseBasicParsing -ErrorAction Stop

    if (-not (Test-Path $versionJsonFile)) {
        throw "Failed to retrieve version.json file from GitHub."
    }

    # Extract version info from JSON
    $jsonContent = Get-Content $versionJsonFile | ConvertFrom-Json
    $remoteVersion = $jsonContent.version

    # Get local version info by executing blue_onyx.exe --version
    # If no version installed then we give it s 0 version
    try {
        $localVersionOutput = & "$destinationPath\blue_onyx.exe" "--version"
    }
    catch {
        $localVersionOutput = "blue-onyx 0.0.0"
    }
    # Extract version from the output; Example outpout: "blue-onyx 0.3.0"
    $localVersion = ($localVersionOutput -split " ")[1]

    # Convert version strings to comparable versions
    $remoteVersion = [System.Version]::Parse($remoteVersion)
    $localVersion = [System.Version]::Parse($localVersion)

    # Compare versions; if local version is lower version or if local verison is not yet installed then install.
    if ($remoteVersion -gt $localVersion -or [string]::IsNullOrEmpty($localVersion)) {
        Write-Host "newer version ($remoteVersion) is available! Currently Installed version is ($localVersion)." -ForegroundColor DarkYellow
        Write-Host "Proceeding with upgrade!" -ForegroundColor DarkYellow

        Write-Host "Parsing version.json..." -ForegroundColor Green
        $jsonContent = Get-Content -Path $versionJsonFile -Raw
        $json = $jsonContent | ConvertFrom-Json

        if (-not $json.version -or -not $json.windows -or -not $json.windows_sha256) {
            throw "version.json does not contain the required fields (version, windows, windows_sha256)."
        }

        $zipUrl = "https://github.com/xnorpx/blue-onyx/releases/latest/download/$($json.windows)"
        $sha256Url = "https://github.com/xnorpx/blue-onyx/releases/latest/download/$($json.windows_sha256)"

        Write-Host "Version: $($json.version)" -ForegroundColor Cyan
        Write-Host "ZIP URL: $zipUrl" -ForegroundColor Blue
        Write-Host "SHA256 URL: $sha256Url" -ForegroundColor Yellow

        $zipFile = Join-Path $tempPath $json.windows
        $sha256File = Join-Path $tempPath $json.windows_sha256

        if (Test-Path $zipFile) {
            Write-Host "Found cached ZIP file at $zipFile. Skipping download..." -ForegroundColor Green
        }
        else {
            Write-Host "Downloading ZIP file to $zipFile..." -ForegroundColor Yellow
            Invoke-WebRequest -Uri $zipUrl -OutFile $zipFile -UseBasicParsing -ErrorAction Stop
        }

        if (Test-Path $sha256File) {
            Write-Host "Found cached SHA256 file at $sha256File. Skipping download..." -ForegroundColor Green
        }
        else {
            Write-Host "Downloading SHA256 file to $sha256File..." -ForegroundColor Green
            Invoke-WebRequest -Uri $sha256Url -OutFile $sha256File -UseBasicParsing -ErrorAction Stop
        }

        Write-Host "Verifying ZIP file integrity..." -ForegroundColor DarkYellow
        $sha256FileContent = Get-Content $sha256File -Raw
        $pattern = '[A-Fa-f0-9]{64}'  # 64 hex characters for a SHA256 hash

        $match = [System.Text.RegularExpressions.Regex]::Match($sha256FileContent, $pattern)
        if (-not $match.Success) {
            throw "Could not parse a valid 64-hex SHA256 from the .sha256 file."
        }
        $expectedSha256 = $match.Value.ToLower()

        $actualHash = (Get-FileHash -Algorithm SHA256 $zipFile).Hash.ToLower()

        Write-Host "Expected SHA256: $expectedSha256" -ForegroundColor Yellow
        Write-Host "Actual   SHA256: $actualHash" -ForegroundColor Yellow

        if ($expectedSha256 -ne $actualHash) {
            throw "ZIP file SHA256 does not match expected value!"
        }
        Write-Host "SHA256 verification successful." -ForegroundColor Green

        # Create a temporary folder under .blue-onyx for unzipping
        $tempExtractPath = Join-Path $destinationPath "temp_unzip"
        if (Test-Path $tempExtractPath) {
            Remove-Item -Recurse -Force $tempExtractPath
        }
        New-Item -ItemType Directory -Path $tempExtractPath | Out-Null

        Write-Host "Extracting ZIP contents to $tempExtractPath..." -ForegroundColor Yellow
        Expand-Archive -Path $zipFile -DestinationPath $tempExtractPath -Force

        # Force results into an array so .Count is always valid
        $directories = @(Get-ChildItem -Path $tempExtractPath -Directory)
        $flattenPath = $tempExtractPath

        if ($directories.Count -eq 1) {
            # There's exactly one directory, use it as the flattened path
            $flattenPath = $directories[0].FullName
            Write-Host "Single top-level folder found, flattening from: $flattenPath"
        }
        else {
            Write-Host "Multiple or no subfolders found, skipping single-folder flatten logic."
        }


        Write-Host "Overwriting existing files in $destinationPath with the new files..." -ForegroundColor Green
        Copy-Item -Path (Join-Path $flattenPath '*') -Destination $destinationPath -Recurse -Force

        # Cleanup the temp_unzip folder
        Remove-Item -Path $tempExtractPath -Recurse -Force

        Write-Host "Adding $destinationPath to User PATH..." -ForegroundColor Green
        # Gets the Current User PATH
        $userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")

        if ($userPath -notlike "*$destinationPath*") {
            if ([string]::IsNullOrEmpty($userPath)) {
                $newPath = $destinationPath
            }
            else {
                #append new Install Path to User Path.
                $newPath = "$userPath;$destinationPath"
            }

            [System.Environment]::SetEnvironmentVariable("PATH", $newPath, "User")
            # Also update the current session PATH so user can test immediately
            $env:PATH = "$($env:PATH);$destinationPath"
        }

        Write-Host "Downloading all models into $destinationPath..." -ForegroundColor DarkMagenta
        $exePath = Join-Path $destinationPath "blue_onyx.exe"
        if (-not (Test-Path $exePath)) {
            throw "Could not find blue_onyx.exe at $exePath. Installation may have failed or the file may be missing."
        }

        & $exePath --download-model-path $destinationPath

        Write-Host "`nInstallation is complete! Blue Onyx is now installed in:" -ForegroundColor Green
        Write-Host "  $destinationPath" -ForegroundColor DarkYellow
        Write-Host "`nDownloaded models into $destinationPath" -ForegroundColor DarkMagenta

        Write-Host "`nPlease restart your PowerShell or Command Prompt to ensure the updated PATH is loaded." -ForegroundColor Green
        Write-Host "`nPlease restart Blue Onyx if this was a reinstall or update." -ForegroundColor Green
        Write-Host "Done!" -ForegroundColor Green

    }
    else {
        Write-Host "Installed version ($localVersion) is up-to-date or newer than remote version ($remoteVersion)." -ForegroundColor Green
        Write-host "Done!" -ForegroundColor Green
        # Add logic to inform user that update is not necessary here
    }

}
catch {
    Write-ErrorRed "$($_.Exception.Message)"
    Write-ErrorRed "Installation failed. Exiting..."
    exit 1
}
