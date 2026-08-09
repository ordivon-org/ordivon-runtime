param(
    [Parameter(Mandatory = $true)]
    [string]$Distro,

    [Parameter(Mandatory = $true)]
    [string]$Root,

    [string[]]$StartUnits = @('ordivon-runtime.service', 'ordivon-host-mcp.service'),

    [int]$PreTerminateDelaySeconds = 2,

    [int]$PostTerminateDelaySeconds = 2,

    [int]$PostRestartDelaySeconds = 3
)

$ErrorActionPreference = 'Stop'
$manifestPath = Join-Path $Root 'manifest.json'
$resultPath = Join-Path $Root 'watchdog-result.json'
if (-not (Test-Path -LiteralPath $manifestPath)) {
    throw "R-W5 manifest is missing: $manifestPath"
}
$manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
foreach ($field in @('attemptId', 'marker', 'preBootId')) {
    if ([string]::IsNullOrWhiteSpace([string]$manifest.$field)) {
        throw "R-W5 manifest field is missing: $field"
    }
}

function Get-MarkerCount([string]$Marker) {
    $rows = Get-CimInstance Win32_Process | Where-Object {
        $_.ProcessId -ne $PID -and $_.CommandLine -like ('*' + $Marker + '*')
    }
    return @($rows).Count
}

function Get-PowerPresent([string]$AttemptId) {
    $text = (& "$env:SystemRoot\System32\powercfg.exe" /requests | Out-String)
    return $text.IndexOf(
        ('Ordivon Runtime Attempt ' + $AttemptId),
        [System.StringComparison]::OrdinalIgnoreCase
    ) -ge 0
}

$result = [ordered]@{
    schemaVersion = 1
    distro = $Distro
    attemptId = $manifest.attemptId
    marker = $manifest.marker
    preBootId = $manifest.preBootId
    startedAtUtc = [DateTime]::UtcNow.ToString('o')
}

try {
    Start-Sleep -Seconds $PreTerminateDelaySeconds
    $result.beforeMarkerCount = Get-MarkerCount $manifest.marker
    $result.beforePowerPresent = Get-PowerPresent $manifest.attemptId
    if ($result.beforeMarkerCount -le 0 -or -not $result.beforePowerPresent) {
        throw 'R-W5 specimen is not active before WSL termination'
    }

    $terminate = Start-Process `
        -FilePath "$env:SystemRoot\System32\wsl.exe" `
        -ArgumentList @('--terminate', $Distro) `
        -Wait -PassThru -WindowStyle Hidden
    $result.terminateExitCode = $terminate.ExitCode
    if ($terminate.ExitCode -ne 0) {
        throw "wsl --terminate failed with exit code $($terminate.ExitCode)"
    }

    Start-Sleep -Seconds $PostTerminateDelaySeconds
    $result.afterTerminateMarkerCount = Get-MarkerCount $manifest.marker
    $result.afterTerminatePowerPresent = Get-PowerPresent $manifest.attemptId

    $systemctlArgs = @('-d', $Distro, '-u', 'root', '--', '/usr/bin/systemctl', 'start') + $StartUnits
    $restart = Start-Process `
        -FilePath "$env:SystemRoot\System32\wsl.exe" `
        -ArgumentList $systemctlArgs `
        -Wait -PassThru -WindowStyle Hidden
    $result.restartExitCode = $restart.ExitCode
    if ($restart.ExitCode -ne 0) {
        throw "WSL restart/bootstrap failed with exit code $($restart.ExitCode)"
    }

    Start-Sleep -Seconds $PostRestartDelaySeconds
    $result.afterRestartMarkerCount = Get-MarkerCount $manifest.marker
    $result.afterRestartPowerPresent = Get-PowerPresent $manifest.attemptId
    $result.completed = $true
}
catch {
    $result.completed = $false
    $result.error = $_.Exception.ToString()
    try {
        $systemctlArgs = @('-d', $Distro, '-u', 'root', '--', '/usr/bin/systemctl', 'start') + $StartUnits
        $recovery = Start-Process `
            -FilePath "$env:SystemRoot\System32\wsl.exe" `
            -ArgumentList $systemctlArgs `
            -Wait -PassThru -WindowStyle Hidden
        $result.recoveryRestartExitCode = $recovery.ExitCode
    }
    catch {
        $result.recoveryRestartError = $_.Exception.ToString()
    }
}
finally {
    $result.finishedAtUtc = [DateTime]::UtcNow.ToString('o')
    $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $resultPath -Encoding UTF8
}

if (-not $result.completed) {
    exit 1
}
