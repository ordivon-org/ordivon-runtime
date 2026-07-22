param(
    [Parameter(Mandatory = $true)][string]$Distro,
    [Parameter(Mandatory = $true)][string]$Manifest,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [Parameter(Mandatory = $true)][string]$Completion
)

$ErrorActionPreference = "Stop"
$wsl = Join-Path $env:WINDIR "System32\wsl.exe"
Start-Sleep -Seconds 5
& $wsl --shutdown
Start-Sleep -Seconds 5
$collector = "until systemctl show-environment >/dev/null 2>&1; do sleep 1; done; /usr/lib/ordivon/ordivon-m7-reboot-harness collect '$Manifest' '$Evidence'"
& $wsl -d $Distro -- bash -lc $collector
$exitCode = $LASTEXITCODE
$payload = [ordered]@{
    schemaVersion = 1
    distro = $Distro
    restartMode = "full-wsl-kernel-shutdown"
    exitCode = $exitCode
    completedAt = (Get-Date).ToUniversalTime().ToString("o")
}
$payload | ConvertTo-Json | Set-Content -Encoding UTF8 -Path $Completion
exit $exitCode
