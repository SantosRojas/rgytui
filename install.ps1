<#
.SYNOPSIS
    Installs rgytui (YouTube TUI Music Player) and its dependencies.
.DESCRIPTION
    Downloads rgytui from GitHub Releases, installs it to %LOCALAPPDATA%\rgytui,
    verifies/installs yt-dlp and mpv via winget, adds to PATH, and creates
    Start Menu shortcuts.
.PARAMETER Prefix
    Installation directory (default: %LOCALAPPDATA%\rgytui).
.PARAMETER NoDeps
    Skip dependency installation (yt-dlp, mpv).
.PARAMETER NoPath
    Skip adding to PATH.
.PARAMETER Version
    Specific version to install (default: latest).
.EXAMPLE
    .\install.ps1
    .\install.ps1 -Prefix "D:\Tools\rgytui" -NoDeps
#>

param(
    [string]$Prefix = "",
    [switch]$NoDeps,
    [switch]$NoPath,
    [string]$Version = "latest"
)

$ErrorActionPreference = "Stop"

# ── Colors ──────────────────────────────────────────────────────────────────
$Host.UI.RawUI.ForegroundColor = "Cyan"
Write-Host "╔══════════════════════════════════════════════╗"
Write-Host "║         rgytui — YouTube TUI Player         ║"
Write-Host "║              Windows Installer              ║"
Write-Host "╚══════════════════════════════════════════════╝"
$Host.UI.RawUI.ForegroundColor = "White"

# ── Admin check ─────────────────────────────────────────────────────────────
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdmin) {
    Write-Host "⚠  Some operations require administrator privileges (PATH, winget)." -ForegroundColor Yellow
    Write-Host "  Please re-run as Administrator." -ForegroundColor Yellow
    Write-Host "  Right-click PowerShell → Run as Administrator" -ForegroundColor Yellow
}

# ── Detect architecture ─────────────────────────────────────────────────────
$arch = switch ([Environment]::Is64BitOperatingSystem) {
    $true  { "x86_64" }
    $false { "i686"   }
}

Write-Host "`n› Architecture: $arch" -ForegroundColor Gray

# ── Installation directory ──────────────────────────────────────────────────
if (-not $Prefix) {
    $Prefix = Join-Path $env:LOCALAPPDATA "rgytui"
}
$binDir = Join-Path $Prefix "bin"

Write-Host "› Target: $Prefix" -ForegroundColor Gray

# ── Check existing installation ─────────────────────────────────────────────
$existingPath = Join-Path $binDir "rgytui.exe"
if (Test-Path $existingPath) {
    Write-Host "  Existing installation found. Will upgrade." -ForegroundColor Yellow
}

# ── Download binary ─────────────────────────────────────────────────────────
$exePath = Join-Path $binDir "rgytui.exe"

if (-not (Test-Path (Join-Path $PSScriptRoot "rgytui.exe"))) {
    Write-Host "`n⬇  Downloading rgytui..." -ForegroundColor Green

    if ($Version -eq "latest") {
        $releases = "https://api.github.com/repos/rojasape/rgytui/releases/latest"
        try {
            $tag = (Invoke-RestMethod -Uri $releases).tag_name
        } catch {
            Write-Host "  Failed to fetch latest release: $_" -ForegroundColor Red
            Write-Host "  Build locally with: cargo build --release" -ForegroundColor Yellow
            exit 1
        }
    } else {
        $tag = $Version
    }

    $url = "https://github.com/rojasape/rgytui/releases/download/$tag/rgytui-x86_64-windows.zip"
    $zipPath = Join-Path $env:TEMP "rgytui.zip"

    try {
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
        Expand-Archive -Path $zipPath -DestinationPath $binDir -Force
        Remove-Item $zipPath -Force
        Write-Host "  ✓ Downloaded $tag" -ForegroundColor Green
    } catch {
        Write-Host "  Download failed: $_" -ForegroundColor Red
        Write-Host "  Falling back to local build..." -ForegroundColor Yellow
        $localBin = Join-Path $PSScriptRoot "target\release\rgytui.exe"
        if (Test-Path $localBin) {
            New-Item -ItemType Directory -Force -Path $binDir | Out-Null
            Copy-Item $localBin $exePath
            Write-Host "  ✓ Copied local build" -ForegroundColor Green
        } else {
            Write-Host "  ✗ No local build found. Run 'cargo build --release' first." -ForegroundColor Red
            exit 1
        }
    }
} else {
    Write-Host "`n📦 Using local binary..." -ForegroundColor Green
    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    Copy-Item (Join-Path $PSScriptRoot "rgytui.exe") $exePath
}

# ── Verify binary ───────────────────────────────────────────────────────────
if (-not (Test-Path $exePath)) {
    Write-Host "✗  Binary not found at $exePath" -ForegroundColor Red
    exit 1
}

# ── Install dependencies ────────────────────────────────────────────────────
if (-not $NoDeps) {
    Write-Host "`n🔍 Checking dependencies..." -ForegroundColor Green

    # yt-dlp
    $ytdlp = Get-Command "yt-dlp" -ErrorAction SilentlyContinue
    if (-not $ytdlp) {
        Write-Host "  Installing yt-dlp via winget..." -ForegroundColor Yellow
        try {
            winget install --id yt-dlp.yt-dlp --silent --accept-package-agreements 2>$null
            Write-Host "  ✓ yt-dlp installed" -ForegroundColor Green
        } catch {
            Write-Host "  winget failed. Install yt-dlp manually: https://github.com/yt-dlp/yt-dlp" -ForegroundColor Red
        }
    } else {
        Write-Host "  ✓ yt-dlp found" -ForegroundColor Green
    }

    # mpv
    $mpv = Get-Command "mpv" -ErrorAction SilentlyContinue
    if (-not $mpv) {
        Write-Host "  Installing mpv via winget..." -ForegroundColor Yellow
        try {
            winget install --id mpv.mpv --silent --accept-package-agreements 2>$null
            Write-Host "  ✓ mpv installed" -ForegroundColor Green
        } catch {
            Write-Host "  winget failed. Install mpv manually: https://mpv.io" -ForegroundColor Red
        }
    } else {
        Write-Host "  ✓ mpv found" -ForegroundColor Green
    }
} else {
    Write-Host "`n⚠  Skipping dependency installation (--NoDeps)" -ForegroundColor Yellow
}

# ── Add to PATH ─────────────────────────────────────────────────────────────
if (-not $NoPath) {
    Write-Host "`n🔧 Adding to PATH..." -ForegroundColor Green
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$binDir*") {
        $newPath = "$userPath;$binDir"
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Host "  ✓ Added $binDir to user PATH" -ForegroundColor Green
    } else {
        Write-Host "  ✓ Already in PATH" -ForegroundColor Green
    }
}

# ── Start Menu shortcut ────────────────────────────────────────────────────
Write-Host "`n📌 Creating Start Menu shortcut..." -ForegroundColor Green
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\rgytui"
New-Item -ItemType Directory -Force -Path $startMenuDir | Out-Null

$shortcutPath = Join-Path $startMenuDir "rgytui.lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $exePath
$shortcut.Description = "rgytui — YouTube TUI Music Player"
$shortcut.WorkingDirectory = $Prefix
$shortcut.Save()
Write-Host "  ✓ Shortcut created" -ForegroundColor Green

# ── Uninstaller ─────────────────────────────────────────────────────────────
Copy-Item (Join-Path $PSScriptRoot "uninstall.ps1") (Join-Path $Prefix "uninstall.ps1")

# ── Done ────────────────────────────────────────────────────────────────────
Write-Host "`n╔══════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║               Installation Complete          ║" -ForegroundColor Cyan
Write-Host "╠══════════════════════════════════════════════╣" -ForegroundColor Cyan
Write-Host "║  Run:  rgytui                                ║" -ForegroundColor Cyan
Write-Host "║  Help: rgytui --help                         ║" -ForegroundColor Cyan
Write-Host "║                                              ║" -ForegroundColor Cyan
Write-Host "║  Uninstall:                                  ║" -ForegroundColor Cyan
Write-Host "║    $((Join-Path $Prefix 'uninstall.ps1'))    ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════╝" -ForegroundColor Cyan

Write-Host "`nPress any key to close..." -ForegroundColor Gray
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
