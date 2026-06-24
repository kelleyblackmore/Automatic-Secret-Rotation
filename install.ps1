# Automatic Secret Rotation installer for Windows
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.ps1 | iex
#
# Environment variable overrides:
#   $env:ASR_VERSION = "0.5.0"   # Install a specific version (default: latest)
#   $env:ASR_INSTALL_DIR = "C:\custom\path"  # Custom install directory

$ErrorActionPreference = 'Stop'

$repo  = "kelleyblackmore/Automatic-Secret-Rotation"
$asset = "asr-windows-x86_64.exe"
$binName = "asr.exe"

# Default install dir: %LOCALAPPDATA%\Programs\asr
$installDir = if ($env:ASR_INSTALL_DIR) { $env:ASR_INSTALL_DIR } `
              else { Join-Path $env:LOCALAPPDATA "Programs\asr" }

Write-Host "Installing Automatic Secret Rotation (asr) for Windows..."
Write-Host "Install directory: $installDir"

# Determine download URL
if ($env:ASR_VERSION) {
    $version = $env:ASR_VERSION
    if (-not $version.StartsWith("v")) { $version = "v$version" }
    $downloadUrl = "https://github.com/$repo/releases/download/$version/$asset"
} else {
    # Use latest release redirect
    $downloadUrl = "https://github.com/$repo/releases/latest/download/$asset"
}

Write-Host "Downloading $asset from GitHub releases..."

# Create install directory
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir | Out-Null
}

$destPath = Join-Path $installDir $binName

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $destPath -UseBasicParsing
} catch {
    Write-Error "Failed to download $asset from $downloadUrl`nError: $_"
    exit 1
}

Write-Host "Downloaded successfully to: $destPath"

# Add install dir to user PATH if not already present
$userPath = [System.Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$installDir*") {
    [System.Environment]::SetEnvironmentVariable(
        "PATH",
        "$installDir;$userPath",
        "User"
    )
    Write-Host "Added $installDir to your user PATH."
    Write-Host "Restart your terminal or run the following to use 'asr' in this session:"
    Write-Host "  `$env:PATH = `"$installDir;`$env:PATH`""
} else {
    Write-Host "$installDir is already in PATH."
}

# Verify
Write-Host ""
Write-Host "Verifying installation..."
try {
    $version = & $destPath --version 2>&1
    Write-Host "Installed: $version"
} catch {
    Write-Warning "Binary installed but could not run --version: $_"
}

Write-Host ""
Write-Host "Get started:"
Write-Host "  asr --help"
Write-Host "  asr init         # Create a config file"
Write-Host "  asr gen-password myapp/db"
Write-Host ""
Write-Host "For more information: https://github.com/$repo"
