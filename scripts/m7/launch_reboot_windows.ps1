param(
    [Parameter(Mandatory = $true)][string]$Script,
    [Parameter(Mandatory = $true)][string]$Distro,
    [Parameter(Mandatory = $true)][string]$Manifest,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [Parameter(Mandatory = $true)][string]$Completion
)

$arguments = @(
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", $Script,
    "-Distro", $Distro,
    "-Manifest", $Manifest,
    "-Evidence", $Evidence,
    "-Completion", $Completion
)
Start-Process -FilePath "powershell.exe" -ArgumentList $arguments -WindowStyle Hidden
