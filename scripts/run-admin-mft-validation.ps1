param(
  [string]$Path = "C:\"
)

$ErrorActionPreference = "Stop"

function Test-IsAdmin {
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = New-Object Security.Principal.WindowsPrincipal($identity)
  return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$tauriRoot = Join-Path $repoRoot "cdrive-cleaner\src-tauri"

if (-not (Test-Path $tauriRoot)) {
  throw "无法定位 src-tauri 工作目录: $tauriRoot"
}

if (-not (Test-IsAdmin)) {
  Start-Process powershell `
    -Verb RunAs `
    -WorkingDirectory $tauriRoot `
    -ArgumentList @(
      "-NoProfile",
      "-ExecutionPolicy", "Bypass",
      "-File", $PSCommandPath,
      "-Path", $Path
    )
  return
}

Push-Location $tauriRoot
try {
  cargo run --example admin_mft_validation -- $Path
} finally {
  Pop-Location
}
