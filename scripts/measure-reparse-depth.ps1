# Measure depth distribution of reparse points under real directories.
$paths = @('C:\Users\admin', 'C:\Program Files', 'C:\Program Files (x86)')

foreach ($p in $paths) {
    Write-Host ""
    Write-Host "===== $p ====="
    $count = 0
    $byDepth = @{}
    $maxObserved = 0
    Get-ChildItem -Path $p -Recurse -Force -ErrorAction SilentlyContinue -Depth 10 |
        Where-Object {
            $_.Attributes -match 'ReparsePoint' -or
            $_.LinkType -eq 'Junction' -or
            $_.LinkType -eq 'SymbolicLink'
        } |
        ForEach-Object {
            $rel = $_.FullName.Substring($p.Length).Trim('\')
            $depth = ($rel -split '\\').Length
            if (-not $byDepth.ContainsKey($depth)) { $byDepth[$depth] = 0 }
            $byDepth[$depth] = $byDepth[$depth] + 1
            if ($depth -gt $maxObserved) { $maxObserved = $depth }
            $count++
        }
    Write-Host ("Total reparse points: {0}" -f $count)
    Write-Host ("Max observed depth: {0}" -f $maxObserved)
    $byDepth.GetEnumerator() | Sort-Object Name | ForEach-Object {
        Write-Host ("  depth {0,2}: {1}" -f $_.Key, $_.Value)
    }
}
