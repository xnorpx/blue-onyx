# Get current date for file naming
$date = Get-Date -Format "yyyyMMdd"
$outputFile = "BlueOnyxServiceLog$date.txt"

# Get events from the last 24 hours
$startTime = (Get-Date).AddHours(-24)

Write-Host "Searching for Blue Onyx events from the last 24 hours..."

# Try to find Blue Onyx events
$events = @()

# Define all possible Blue Onyx variations with wildcards
$searchPatterns = @(
    "*blue*onyx*",
    "*blueonyx*",
    "*blue_onyx*",
    "*blue-onyx*",
    "*blueOnyx*"
)

# Search System log
Write-Host "Searching System log..."
try {
    $systemEvents = Get-WinEvent -FilterHashtable @{
        LogName   = 'System'
        StartTime = $startTime
    } -ErrorAction SilentlyContinue

    foreach ($pattern in $searchPatterns) {
        $matchingEvents = $systemEvents | Where-Object {
            $_.Message -like $pattern -or
            $_.ProviderName -like $pattern -or
            $_.TaskDisplayName -like $pattern
        }
        if ($matchingEvents) {
            Write-Host "  Found $($matchingEvents.Count) events matching pattern '$pattern' in System log"
            $events += $matchingEvents
        }
    }
}
catch {
    Write-Host "  Error searching System log: $($_.Exception.Message)"
}

# Search Application log
Write-Host "Searching Application log..."
try {
    $appEvents = Get-WinEvent -FilterHashtable @{
        LogName   = 'Application'
        StartTime = $startTime
    } -ErrorAction SilentlyContinue

    foreach ($pattern in $searchPatterns) {
        $matchingEvents = $appEvents | Where-Object {
            $_.Message -like $pattern -or
            $_.ProviderName -like $pattern -or
            $_.TaskDisplayName -like $pattern
        }
        if ($matchingEvents) {
            Write-Host "  Found $($matchingEvents.Count) events matching pattern '$pattern' in Application log"
            $events += $matchingEvents
        }
    }
}
catch {
    Write-Host "  Error searching Application log: $($_.Exception.Message)"
}

# Search Security log (if accessible)
Write-Host "Searching Security log..."
try {
    $secEvents = Get-WinEvent -FilterHashtable @{
        LogName   = 'Security'
        StartTime = $startTime
    } -ErrorAction SilentlyContinue

    foreach ($pattern in $searchPatterns) {
        $matchingEvents = $secEvents | Where-Object {
            $_.Message -like $pattern -or
            $_.ProviderName -like $pattern -or
            $_.TaskDisplayName -like $pattern
        }
        if ($matchingEvents) {
            Write-Host "  Found $($matchingEvents.Count) events matching pattern '$pattern' in Security log"
            $events += $matchingEvents
        }
    }
}
catch {
    Write-Host "  Security log not accessible or error: $($_.Exception.Message)"
}

# Remove duplicates, filter out Service Control Manager, and sort
$uniqueEvents = $events | Where-Object {
    $_.ProviderName -ne "Service Control Manager"
} | Sort-Object TimeCreated -Unique

# Export events to a text file with Rust-like log format
if ($uniqueEvents) {
    Write-Host "Found total of $($uniqueEvents.Count) unique Blue Onyx related events"

    # Create Rust-like log format: timestamp level [provider] message
    $output = @()

    foreach ($event in $uniqueEvents) {
        # Format timestamp like Rust logs (ISO 8601)
        $timestamp = $event.TimeCreated.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")

        # Get level (convert to Rust-style)
        $level = switch ($event.LevelDisplayName) {
            "Error" { "ERROR" }
            "Warning" { "WARN " }
            "Information" { "INFO " }
            "Verbose" { "DEBUG" }
            "Critical" { "ERROR" }
            default { "INFO " }
        }

        # Clean up the message - remove extra whitespace and newlines, take first meaningful line
        $message = if ($event.Message) {
            $cleanMessage = $event.Message -replace "`r`n", " " -replace "`n", " " -replace "\s+", " "
            $cleanMessage.Trim()
        }
        else {
            "No message"
        }

        # Format like Rust log: timestamp LEVEL [provider] message
        $logLine = "$timestamp $level [$($event.ProviderName)] $message"
        $output += $logLine
    }

    $output | Out-File -FilePath $outputFile -Encoding utf8
    Write-Host "Rust-style log exported to $outputFile"

}
else {
    Write-Host "No Blue Onyx related events found in the last 24 hours"
    "No Blue Onyx related events found in the last 24 hours" | Out-File -FilePath $outputFile -Encoding utf8
}