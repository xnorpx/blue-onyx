# install_latest_blue_onyx.ps1
<#
.SYNOPSIS
    Installs (or updates) the latest Blue Onyx release on Windows, overwriting existing files if present,
    downloads all models, and creates three .bat files on the user's Desktop:
      1) blue_onyx_start_server.bat
      2) blue_onyx_benchmark_my_machine.bat
      3) test_blue_onyx_server.bat
.DESCRIPTION
    1. Creates (if needed) a temporary folder in %TEMP% (BlueOnyxInstall).
    2. Always downloads 'version.json' into %TEMP%.
    3. Parses 'version.json' to get the ZIP filename and its .sha256 filename.
    4. If the ZIP and .sha256 files are already found in %TEMP%, skip downloading them (caching).
    5. Verifies the ZIP file's integrity using a SHA256 checksum (via regex).
    6. Extracts the ZIP into a "temp_unzip" folder inside %USERPROFILE%\.blue-onyx, then flattens it if needed.
    7. Copies the new files into %USERPROFILE%\.blue-onyx, overwriting old ones if they exist, without asking permission.
    8. Adds that folder to the PATH (System "Machine" environment).
    9. Runs "blue_onyx.exe --download-model-path" to download all models into .blue-onyx.
    10. Creates .bat files on the user's Desktop with server start / benchmarking / testing commands.
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-ErrorRed {
    param(
        [Parameter(Mandatory=$true)]
        [string] $Message
    )
    Write-Host "`n[ERROR] $Message" -ForegroundColor Red
}

function Write-Green {
    param(
        [Parameter(Mandatory=$true)]
        [string] $Message
    )
    Write-Host "$Message" -ForegroundColor Green
}

try {
    # --- 1. Prepare download path in %TEMP% ---
    $tempPath = Join-Path $env:TEMP "BlueOnyxInstall"
    if (-not (Test-Path $tempPath)) {
        New-Item -ItemType Directory -Path $tempPath | Out-Null
    }

    # --- 2. Download version.json into %TEMP% (always) ---
    $versionJsonFile = Join-Path $tempPath "version.json"
    $versionJsonUrl  = "https://github.com/xnorpx/blue-onyx/releases/latest/download/version.json"
    Write-Host "Downloading version.json from $versionJsonUrl to $versionJsonFile..." -ForegroundColor Green
    Invoke-WebRequest -Uri $versionJsonUrl -OutFile $versionJsonFile -UseBasicParsing -ErrorAction Stop

    if (-not (Test-Path $versionJsonFile)) {
        throw "Failed to retrieve version.json file from GitHub."
    }

    # --- 3. Parse JSON ---
    Write-Host "Parsing version.json..." -ForegroundColor Green
    $jsonContent = Get-Content -Path $versionJsonFile -Raw
    $json = $jsonContent | ConvertFrom-Json

    if (-not $json.version -or -not $json.windows -or -not $json.windows_sha256) {
        throw "version.json does not contain the required fields (version, windows, windows_sha256)."
    }

    $zipUrl    = "https://github.com/xnorpx/blue-onyx/releases/latest/download/$($json.windows)"
    $sha256Url = "https://github.com/xnorpx/blue-onyx/releases/latest/download/$($json.windows_sha256)"

    Write-Host "Version: $($json.version)" -ForegroundColor Cyan
    Write-Host "ZIP URL: $zipUrl" -ForegroundColor Blue
    Write-Host "SHA256 URL: $sha256Url" -ForegroundColor Yellow

    # --- 4. Check if ZIP and SHA256 already exist in %TEMP%, else download (cache) ---
    $zipFile    = Join-Path $tempPath $json.windows
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

    # --- 5. Verify the ZIP file's integrity with the SHA256 ---
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

    # --- 6. Extract the ZIP into a temporary subfolder, then flatten if needed ---

    # Get the desired folder from the user using a file dialog
    $folderBrowser = New-Object -ComObject Shell.Application
    $folder = $folderBrowser.BrowseForFolder(0, "Select Installation Folder, Cancel to install in default location", 0)
    if ($folder) {
        $destinationPath = Join-Path $folder.Self.Path ".blue-onyx"
    } else {
        # Handle the case where the user cancels the dialog
        Write-Warning "No folder selected, installing in default location." 
        $destinationPath = Join-Path $env:USERPROFILE ".blue-onyx"
    }

    if (-not (Test-Path $destinationPath)) {
        # Create .blue-onyx if it doesn't exist
        New-Item -ItemType Directory -Path $destinationPath | Out-Null
    }

    # # --- 6. Extract the ZIP into a temporary subfolder, then flatten if needed ---
    # $destinationPath = Join-Path $env:USERPROFILE ".blue-onyx"
    # if (-not (Test-Path $destinationPath)) {
    #     # Create .blue-onyx if it doesn't exist
    #     New-Item -ItemType Directory -Path $destinationPath | Out-Null
    # }

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

    # --- 6.5. Check if blue_onyx.exe is running and kill it ---
    Write-Host "Checking if blue_onyx.exe is running..." -ForegroundColor Yellow
    $processName = "blue_onyx"
    $process = Get-Process -Name $processName -ErrorAction SilentlyContinue

    if ($process) {
        Write-Host "$processName.exe is currently running. Attempting to terminate..."
        try {
            Stop-Process -Id $process.Id -Force
            Write-Green "$processName.exe has been terminated successfully." -ForegroundColor Green
        }
        catch {
            throw "Failed to terminate $processName.exe. Please close the application manually and retry."
        }

        # Optionally, wait for the process to exit
        Start-Sleep -Seconds 2
        if (Get-Process -Name $processName -ErrorAction SilentlyContinue) {
            throw "$processName.exe is still running after attempting to terminate."
        }
    }
    else {
        Write-Host "$processName.exe is not running. Proceeding with installation..." -ForegroundColor Green
    }

    # --- 7. Copy new files into .blue-onyx, overwriting if they exist ---
    Write-Host "Overwriting existing files in $destinationPath with the new files..." -ForegroundColor Green
    Copy-Item -Path (Join-Path $flattenPath '*') -Destination $destinationPath -Recurse -Force

    # Cleanup the temp_unzip folder
    Remove-Item -Path $tempExtractPath -Recurse -Force

    # --- 8. Add that folder to the PATH (for the user environment) ---
    Write-Host "Adding $destinationPath to User PATH..." -ForegroundColor Green
    $userPath = [System.Environment]::GetEnvironmentVariable("PATH", "Machine")

    if ($userPath -notlike "*$destinationPath*") {
        if ([string]::IsNullOrEmpty($userPath)) {
            $newPath = $destinationPath
        } else {
            $newPath = "$userPath;$destinationPath"
        }

        [System.Environment]::SetEnvironmentVariable("PATH", $newPath, "Machine")
        # Also update the current session PATH so user can test immediately
        $env:PATH = "$($env:PATH);$destinationPath"
    } else {
        Write-Host "Path already contains $destinationPath, skipping update."
    }

    # --- 9. Run blue_onyx.exe to download all models ---
    Write-Host "Downloading all models into $destinationPath..." -ForegroundColor DarkMagenta
    $exePath = Join-Path $destinationPath "blue_onyx.exe"
    if (-not (Test-Path $exePath)) {
        throw "Could not find blue_onyx.exe at $exePath. Installation may have failed or the file may be missing."
    }
    # We'll run the exe in a separate process.
    # This will download models into .blue-onyx folder.
    & $exePath --download-model-path $destinationPath

    # Now that we can see all the GPU's identifed when downloading all models.
    # Select which GPU blue_onyx will use in bat files. 
    # Import necessary namespace
    Add-Type -AssemblyName System.Windows.Forms

    # Get a list of available GPUs (replace with your actual GPU detection method)
	# We want to exclude RDP Displays, and Sort the list like how it is in Task Manager so the Index is correct. 
	$gpus = Get-CimInstance Win32_VideoController | Select-Object Name | Where-Object name -NotMatch "Microsoft Remote Display Adapter" |Sort-Object Name
    $gpuNames = $gpus.Name

    # Create a form
    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Blue Onyx GPU Selection"
    $form.Size = New-Object System.Drawing.Size(350, 210)
    $form.StartPosition = 'CenterScreen'

    # Create a label
    $label = New-Object System.Windows.Forms.Label
    $label.Text = "Select GPU:"
    $label.Location = New-Object System.Drawing.Point(20, 20)
    $form.Controls.Add($label)

    # Create a combobox
    $comboBox = New-Object System.Windows.Forms.ComboBox
    $comboBox.Location = New-Object System.Drawing.Point(20, 50)
    $comboBox.Width = 300
    $comboBox.DataSource = $gpuNames
    $form.Controls.Add($comboBox)

    # Create an OK button
    $okButton = New-Object System.Windows.Forms.Button
    $okButton.Text = "OK"
    $okButton.Location = New-Object System.Drawing.Point(135, 90)
    $okButton.DialogResult = [System.Windows.Forms.DialogResult]::OK
    $form.Controls.Add($okButton)

    # Show the form and get user selection
    $result = $form.ShowDialog()

    # Check if the user clicked OK
    if ($result -eq [System.Windows.Forms.DialogResult]::OK) {
        # Get the selected GPU name
        $selectedGpuName = $comboBox.SelectedItem

        # Get the index of the selected GPU in the list
        $selectedGpuIndex = $gpuNames.IndexOf($selectedGpuName)

        # Display the selected GPU index (replace with your desired action)
        Write-Host "Blue Onyx will use GPU: ($selectedGpuIndex) $selectedGpuName" -ForegroundColor DarkYellow
    }


    # --- 10. Create Batch Files on Desktop BEFORE final success text ---
    #Write-Host "Creating batch files on the Desktop..."
    Write-Host "Creating batch files in the Install folder..." -ForegroundColor Green

    #$desktopPath = [Environment]::GetFolderPath("Desktop")

    # 1. blue_onyx_start_server.bat
    $startServerContent = @"
:: change --log-level info to --log-level debug for more information
:: add --log-path %temp% to log to file instead
blue_onyx.exe --port 32168 --gpu-index $selectedGpuIndex --log-level info
pause
"@
    Set-Content -Path (Join-Path $destinationPath "blue_onyx_start_server.bat") -Value $startServerContent -Force
    
    # 2. blue_onyx_start_server_large_models.bat
    $startServerContentLG = @"
:: change --log-level info to --log-level debug for more information
:: add --log-path %temp% to log to file instead
blue_onyx.exe --port 32168 --gpu-index $selectedGpuIndex --log-level info --model "$destinationPath\rt-detrv2-x.onnx"
pause
"@
    Set-Content -Path (Join-Path $destinationPath "blue_onyx_start_server_large_models.bat") -Value $startServerContentLG -Force

    # 3. blue_onyx_benchmark_my_machine.bat
    $benchmarkContent = @"
::GPU
blue_onyx_benchmark.exe --repeat 100 --save-stats-path .

::CPU
blue_onyx_benchmark.exe --force-cpu --repeat 100 --save-stats-path .

pause
"@
    Set-Content -Path (Join-Path $destinationPath "blue_onyx_benchmark_my_machine.bat") -Value $benchmarkContent -Force

    # 4. test_blue_onyx_server.bat
    $testServerContent = @"
test_blue_onyx.exe --origin http://127.0.0.1:32168 -n 10 --interval 10
pause
"@
    Set-Content -Path (Join-Path $destinationPath "test_blue_onyx_server.bat") -Value $testServerContent -Force



    # --- 11. Prompt user to restart shell and exit ---
    Write-Host "`nInstallation is complete! Blue Onyx is now installed in:" -ForegroundColor Green
    Write-Host "  $destinationPath" -ForegroundColor DarkYellow

    Write-Host "`nDownloaded models into $destinationPath" -ForegroundColor DarkMagenta

    Write-Host "`nBatch files created on your Install Folder:" -ForegroundColor Green
    Write-Host "  blue_onyx_start_server.bat" -ForegroundColor Yellow
    Write-Host "  blue_onyx_start_server_large_models.bat" -ForegroundColor Yellow
    Write-Host "  blue_onyx_benchmark_my_machine.bat" -ForegroundColor Yellow
    Write-Host "  test_blue_onyx_server.bat" -ForegroundColor Yellow

    Write-Host "`nPlease restart your PowerShell or Command Prompt to ensure the updated PATH is loaded." -ForegroundColor Green
    Write-Host "`nPlease restart Blue Onyx if this was a reinstall or update." -ForegroundColor Green
    Write-Host "Done!" -ForegroundColor Green
}
catch {
    Write-ErrorRed "$($_.Exception.Message)"
    Write-ErrorRed "Installation failed. Exiting..."
    exit 1
}
