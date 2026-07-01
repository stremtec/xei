# xei Windows installer — run in PowerShell
# iwr https://raw.githubusercontent.com/stremtec/xei/master/install.ps1 | iex

$VERSION = "v0.5.0"
$REPO = "stremtec/xei"
$TARGET = "x86_64-pc-windows-gnu"
$BIN = "xei.exe"

$DEST = "$env:USERPROFILE\.local\bin"
$URL = "https://github.com/$REPO/releases/download/$VERSION/xei-$TARGET.exe.gz"

Write-Host "→ Downloading xei $VERSION..." -ForegroundColor Cyan

New-Item -ItemType Directory -Force -Path $DEST | Out-Null

$gz = "$env:TEMP\xei.exe.gz"
Invoke-WebRequest -Uri $URL -OutFile $gz

# Decompress
$fs = [System.IO.File]::OpenRead($gz)
$gs = New-Object System.IO.Compression.GzipStream($fs, [System.IO.Compression.CompressionMode]::Decompress)
$out = [System.IO.File]::Create("$DEST\$BIN")
$gs.CopyTo($out)
$gs.Close(); $out.Close(); $fs.Close()
Remove-Item $gz

Write-Host "✓ xei installed to $DEST\$BIN" -ForegroundColor Green

# Add to PATH
$path = [Environment]::GetEnvironmentVariable("Path", "User")
if ($path -notlike "*$DEST*") {
    [Environment]::SetEnvironmentVariable("Path", "$path;$DEST", "User")
    $env:Path += ";$DEST"
    Write-Host "✓ Added $DEST to PATH" -ForegroundColor Green
}

Write-Host "  Run: xei" -ForegroundColor White
