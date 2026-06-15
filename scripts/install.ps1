#!/usr/bin/env pwsh
# Synapsis Installer - Windows (PowerShell)
# Pure Rust - No Python dependencies
param(
    [string]$Version = "0.7.0",
    [string]$InstallDir = "$env:LOCALAPPDATA\synapsis"
)

$Repo = "methodwhite/synapsis"
$DataDir = "$env:LOCALAPPDATA\synapsis\data"

Write-Host "Synapsis v$Version Installer (Windows)" -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
$Arch = $env:PROCESSOR_ARCHITECTURE
$Target = switch ($Arch) {
    "AMD64"  { "x86_64-pc-windows-msvc" }
    "ARM64"  { "aarch64-pc-windows-msvc" }
    default { Write-Error "Unsupported architecture: $Arch"; exit 1 }
}

Write-Host "Platform: Windows ($Arch) -> $Target"
Write-Host "Install dir: $InstallDir"
Write-Host "Data dir: $DataDir"
Write-Host ""

# Create directories
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null

# Download pre-built binary
$DownloadUrl = "https://github.com/$Repo/releases/download/v$Version/synapsis-$Target.zip"
$TempDir = "$env:TEMP\synapsis-install"

try {
    Write-Host "Downloading synapsis v$Version for $Target..."
    New-Item -ItemType Directory -Force -Path $TempDir | Out-Null

    $ZipPath = "$TempDir\synapsis.zip"
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath -ErrorAction Stop

    Expand-Archive -Path $ZipPath -DestinationPath "$TempDir\extracted" -Force

    # Copy binaries
    Copy-Item "$TempDir\extracted\synapsis.exe" "$InstallDir\" -Force -ErrorAction SilentlyContinue
    Copy-Item "$TempDir\extracted\synapsis-mcp.exe" "$InstallDir\" -Force -ErrorAction SilentlyContinue
    Copy-Item "$TempDir\extracted\synapsis-server.exe" "$InstallDir\" -Force -ErrorAction SilentlyContinue

    Write-Host ""
    Write-Host "Synapsis v$Version installed successfully!" -ForegroundColor Green
    & "$InstallDir\synapsis.exe" --version
    exit 0
} catch {
    Write-Host "No pre-built binary found. Building from source..." -ForegroundColor Yellow
    Write-Host "Requires: Rust (https://rustup.rs) and Visual Studio Build Tools"
}

# Check Rust
$RustInstalled = Get-Command rustc -ErrorAction SilentlyContinue
if (-not $RustInstalled) {
    Write-Host "Installing Rust..." -ForegroundColor Yellow
    Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile "$TempDir\rustup-init.exe"
    Start-Process -Wait -FilePath "$TempDir\rustup-init.exe" -ArgumentList "-y"
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
}

Write-Host "Building from source..." -ForegroundColor Yellow
$BuildDir = "$TempDir\source"
Remove-Item -Recurse -Force $BuildDir -ErrorAction SilentlyContinue
git clone --depth 1 "https://github.com/$Repo.git" $BuildDir

Set-Location $BuildDir
cargo build --release

Copy-Item "target\release\synapsis.exe" "$InstallDir\" -Force
Copy-Item "target\release\synapsis-mcp.exe" "$InstallDir\" -Force
Copy-Item "target\release\synapsis-server.exe" "$InstallDir\" -Force

Write-Host ""
Write-Host "Synapsis built and installed from source!" -ForegroundColor Green
& "$InstallDir\synapsis.exe" --version

# PATH check
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    Write-Host ""
    Write-Host "Add to your PATH (re-run terminal after):" -ForegroundColor Yellow
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path','User') + ';$InstallDir', 'User')"
}

Write-Host ""
Write-Host "Done!" -ForegroundColor Green
