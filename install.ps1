# Automatic Secret Rotation installer for Windows
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.ps1 | iex
#
# Environment variable overrides:
#   $env:ASR_VERSION = "0.5.0"   # Install a specific version (default: latest)
#   $env:ASR_INSTALL_DIR = "C:\custom\path"  # Custom install directory
#   $env:ASR_SKIP_VERIFY = "1"   # Skip the SHA256SUMS check (only for releases
#                                # older than 0.7.0, which don't publish one)
#
# The download is checked against the release's SHA256SUMS before it is
# installed; a mismatch aborts the install.

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
    $baseUrl = "https://github.com/$repo/releases/download/$version"
} else {
    # Use latest release redirect
    $baseUrl = "https://github.com/$repo/releases/latest/download"
}
$downloadUrl = "$baseUrl/$asset"

Write-Host "Downloading $asset from GitHub releases..."

# Create install directory
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir | Out-Null
}

$destPath = Join-Path $installDir $binName
$tmpPath = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $tmpPath -UseBasicParsing
} catch {
    Write-Error "Failed to download $asset from $downloadUrl`nError: $_"
    exit 1
}

if ($env:ASR_SKIP_VERIFY -eq "1") {
    Write-Warning "ASR_SKIP_VERIFY=1 set, installing without checksum verification"
} else {
    try {
        $sums = (Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -UseBasicParsing).Content
        if ($sums -is [byte[]]) { $sums = [System.Text.Encoding]::UTF8.GetString($sums) }
    } catch {
        Remove-Item $tmpPath -ErrorAction SilentlyContinue
        Write-Error ("Could not download SHA256SUMS for this release, refusing to install.`n" +
            "Releases before 0.7.0 don't publish one; set `$env:ASR_SKIP_VERIFY = '1' to install anyway.")
        exit 1
    }
    $expected = $null
    foreach ($line in ($sums -split "`n")) {
        $parts = $line.Trim() -split '\s+'
        if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $asset) { $expected = $parts[0] }
    }
    $actual = (Get-FileHash -Algorithm SHA256 $tmpPath).Hash.ToLower()
    if (-not $expected -or $expected.ToLower() -ne $actual) {
        Remove-Item $tmpPath -ErrorAction SilentlyContinue
        Write-Error "Checksum verification failed for $asset (expected: $expected, actual: $actual), refusing to install."
        exit 1
    }
    Write-Host "Checksum verified (sha256 $actual)"
}

Move-Item -Force $tmpPath $destPath

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
