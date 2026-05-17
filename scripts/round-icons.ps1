<#
.SYNOPSIS
  Apply rounded-corner alpha mask to every Windows app icon PNG and rebuild icon.ico.

.DESCRIPTION
  Walks src-tauri/icons (and the iOS / Android subfolders) and rewrites every PNG
  with a transparent rounded-rectangle alpha mask. Radius is 22% of the smaller
  dimension by default, which matches Win11 / Edge / Office squircle look.

  After rewriting PNGs, multi-size icon.ico is rebuilt from 16 / 24 / 32 / 48 /
  64 / 128 / 256 frames so File Explorer, taskbar, start menu and the .exe icon
  all show the rounded silhouette.

.PARAMETER RadiusPercent
  Corner radius as a percent of icon side length. Default 22 (a bit softer than
  Apple's 22.37%, similar to Win11).

.PARAMETER IconsRoot
  Path to the icons folder. Defaults to src-tauri\icons relative to repo root.
#>
[CmdletBinding()]
param(
    [int]$RadiusPercent = 22,
    [string]$IconsRoot  = ''
)

if (-not $IconsRoot) {
    $scriptDir = if ($PSScriptRoot) { $PSScriptRoot } else { Split-Path -Parent $MyInvocation.MyCommand.Path }
    $IconsRoot = Join-Path (Split-Path -Parent $scriptDir) 'src-tauri\icons'
}

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

if (-not (Test-Path $IconsRoot)) {
    throw "Icons folder not found: $IconsRoot"
}

function New-RoundedPng {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [int]    $RadiusPercent
    )

    $src = [System.Drawing.Image]::FromFile((Resolve-Path $Path))
    try {
        $w = $src.Width
        $h = $src.Height
        $r = [int]([Math]::Min($w, $h) * $RadiusPercent / 100)

        $bmp = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $bmp.SetResolution($src.HorizontalResolution, $src.VerticalResolution)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try {
            $g.SmoothingMode      = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
            $g.InterpolationMode  = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $g.PixelOffsetMode    = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
            $g.Clear([System.Drawing.Color]::Transparent)

            $gp = New-Object System.Drawing.Drawing2D.GraphicsPath
            $d = $r * 2
            if ($d -le 0) {
                $gp.AddRectangle((New-Object System.Drawing.RectangleF 0, 0, $w, $h))
            } else {
                $gp.AddArc(0,        0,        $d, $d, 180, 90)
                $gp.AddArc($w - $d,  0,        $d, $d, 270, 90)
                $gp.AddArc($w - $d,  $h - $d,  $d, $d, 0,   90)
                $gp.AddArc(0,        $h - $d,  $d, $d, 90,  90)
                $gp.CloseFigure()
            }

            $g.SetClip($gp)
            $g.DrawImage($src, 0, 0, $w, $h)
            $gp.Dispose()
        } finally {
            $g.Dispose()
        }
    } finally {
        $src.Dispose()
    }

    # Atomic write: save to temp, replace original.
    $tmp = "$Path.tmp"
    $bmp.Save($tmp, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Move-Item -Force $tmp $Path
}

function New-MultiSizeIco {
    param(
        [Parameter(Mandatory)] [string]   $SourcePng,
        [Parameter(Mandatory)] [string]   $DestIco,
        [int[]] $Sizes = @(16, 24, 32, 48, 64, 128, 256)
    )

    $bytes = [System.IO.File]::ReadAllBytes($SourcePng)
    $ms    = New-Object System.IO.MemoryStream(,$bytes)
    $src   = [System.Drawing.Image]::FromStream($ms)

    $frames = @()
    foreach ($size in $Sizes) {
        $bmp = New-Object System.Drawing.Bitmap $size, $size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.SmoothingMode      = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $g.InterpolationMode  = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $g.PixelOffsetMode    = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
        $g.Clear([System.Drawing.Color]::Transparent)
        $g.DrawImage($src, 0, 0, $size, $size)
        $g.Dispose()

        $frameStream = New-Object System.IO.MemoryStream
        $bmp.Save($frameStream, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose()
        $frames += ,$frameStream.ToArray()
        $frameStream.Dispose()
    }

    $src.Dispose()
    $ms.Dispose()

    $count = $frames.Count
    $out = New-Object System.IO.MemoryStream
    $w   = New-Object System.IO.BinaryWriter($out)

    # ICONDIR
    $w.Write([uint16]0)        # reserved
    $w.Write([uint16]1)        # type = icon
    $w.Write([uint16]$count)

    $headerSize  = 6 + (16 * $count)
    $imageOffset = $headerSize

    # ICONDIRENTRY[]
    for ($i = 0; $i -lt $count; $i++) {
        $size = $Sizes[$i]
        $byte = if ($size -ge 256) { 0 } else { $size }
        $len  = $frames[$i].Length

        $w.Write([byte]$byte)        # width  (0 = 256)
        $w.Write([byte]$byte)        # height (0 = 256)
        $w.Write([byte]0)            # color count
        $w.Write([byte]0)            # reserved
        $w.Write([uint16]1)          # planes
        $w.Write([uint16]32)         # bpp
        $w.Write([uint32]$len)       # bytes in resource
        $w.Write([uint32]$imageOffset)
        $imageOffset += $len
    }

    foreach ($frame in $frames) {
        $w.Write($frame)
    }

    $w.Flush()
    [System.IO.File]::WriteAllBytes($DestIco, $out.ToArray())
    $w.Dispose()
    $out.Dispose()
}

Write-Host "Icons root: $IconsRoot"
Write-Host "Radius:     $RadiusPercent% of side"
Write-Host ""

$pngs = Get-ChildItem -Path $IconsRoot -Recurse -Filter *.png -File
foreach ($png in $pngs) {
    Write-Host ("  [PNG]  {0}" -f $png.FullName.Substring($IconsRoot.Length).TrimStart('\'))
    New-RoundedPng -Path $png.FullName -RadiusPercent $RadiusPercent
}

$mainPng = Join-Path $IconsRoot 'icon.png'
$mainIco = Join-Path $IconsRoot 'icon.ico'
if (Test-Path $mainPng) {
    Write-Host ""
    Write-Host "  [ICO]  rebuilding icon.ico from icon.png (16/24/32/48/64/128/256)"
    New-MultiSizeIco -SourcePng $mainPng -DestIco $mainIco
}

Write-Host ""
Write-Host "Done. $($pngs.Count) PNG(s) rewritten, icon.ico rebuilt."
