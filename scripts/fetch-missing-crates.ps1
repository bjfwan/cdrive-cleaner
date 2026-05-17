$ErrorActionPreference = 'Stop'
$cacheDir = 'D:\DevTools\Rust\cargo\registry\cache\index.crates.io-1949cf8c6b5b557f'
if (-not (Test-Path $cacheDir)) {
  Write-Host "cache dir not found: $cacheDir"
  exit 1
}

$lockPath = 'D:\Desktop\cdrive-cleaner\src-tauri\Cargo.lock'
$lock = Get-Content $lockPath -Raw

$pattern = '\[\[package\]\]\s+name\s*=\s*"([^"]+)"\s+version\s*=\s*"([^"]+)"\s+source\s*=\s*"registry\+https://github\.com/rust-lang/crates\.io-index"'
$matches = [regex]::Matches($lock, $pattern)

$missing = @()
foreach ($m in $matches) {
  $name = $m.Groups[1].Value
  $version = $m.Groups[2].Value
  $crateFile = Join-Path $cacheDir "$name-$version.crate"
  if (-not (Test-Path $crateFile) -or (Get-Item $crateFile).Length -lt 100) {
    $missing += [PSCustomObject]@{ Name = $name; Version = $version; Path = $crateFile }
  }
}

Write-Host "missing crates: $($missing.Count)"
foreach ($m in $missing) {
  $url = "https://static.crates.io/crates/$($m.Name)/$($m.Name)-$($m.Version).crate"
  Write-Host "downloading $($m.Name) $($m.Version)"
  $ok = $false
  for ($i=0; $i -lt 3 -and -not $ok; $i++) {
    try {
      $req = [System.Net.HttpWebRequest]::Create($url)
      $req.Timeout = 30000
      $req.ReadWriteTimeout = 60000
      $req.AllowAutoRedirect = $true
      $resp = $req.GetResponse()
      $stream = $resp.GetResponseStream()
      $fs = [System.IO.File]::Create($m.Path)
      $stream.CopyTo($fs)
      $fs.Close()
      $stream.Close()
      $resp.Close()
      $ok = $true
    } catch {
      Write-Host "  retry ${i}: $_"
      Remove-Item $m.Path -Force -ErrorAction SilentlyContinue
      Start-Sleep -Seconds 2
    }
  }
  if (-not $ok) {
    Write-Host "  FAILED: $($m.Name) $($m.Version)"
  }
}
Write-Host "done"
