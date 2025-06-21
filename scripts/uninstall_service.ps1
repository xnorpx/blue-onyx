# Blue Onyx Service Uninstallation Script
# Run this script as Administrator to remove Blue Onyx Service

Write-Host "Removing Blue Onyx Service..." -ForegroundColor Green

# Stop the service if it's running
Write-Host "Stopping Blue Onyx Service..." -ForegroundColor Yellow
net stop BlueOnyxService
if ($LASTEXITCODE -eq 0) {
    Write-Host "Service stopped successfully" -ForegroundColor Green
} else {
    Write-Host "Service may not be running or could not be stopped" -ForegroundColor Yellow
}

# Delete the service
Write-Host "Removing Blue Onyx Service..." -ForegroundColor Yellow
sc.exe delete BlueOnyxService
if ($LASTEXITCODE -eq 0) {
    Write-Host "Service removed successfully" -ForegroundColor Green
} else {
    Write-Host "Failed to remove service or service may not exist" -ForegroundColor Yellow
}

Write-Host "Blue Onyx Service uninstallation completed!" -ForegroundColor Green
pause
