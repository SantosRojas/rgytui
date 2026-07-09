<#
.SYNOPSIS
    Uninstalls rgytui (YouTube TUI Music Player).
.DESCRIPTION
    Removes rgytui binary, config, PATH entries, and Start Menu shortcuts.
    Optionally removes yt-dlp and mpv.
.PARAMETER RemoveConfig
    Remove configuration and playlist data.
.PARAMETER RemoveDeps
    Also remove yt-dlp and mpv.
.EXAMPLE
    .\uninstall.ps1
    .\uninstall.ps1 -RemoveConfig -RemoveDeps
#>

param(
    [switch]$RemoveConfig,
    [switch]$RemoveDeps
)

$ErrorActionPreference = "Stop"

# ── Colors ──────────────────────────────────────────────────────────────────
$Host.UI.RawUI.ForegroundColor = "Cyan"
Write-Host "╔══════════════════════════════════════════════╗"
Write-Host "║         rgytui — YouTube TUI Player         ║"
Write-Host "║              Windows Uninstaller            ║"
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

# ── Confirmation ────────────────────────────────────────────────────────────
Write-Host "`nThis will remove rgytui and its files." -ForegroundColor Yellow
$confirm = Read-Host "Continue? [y/N]"
if ($confirm -ne "y") {
    Write-Host "Aborted." -ForegroundColor Gray
    exit
}

# ── Detect installation ─────────────────────────────────────────────────────
$prefix = Join-Path $env:LOCALAPPDATA "rgytui"
$binDir = Join-Path $prefix "bin"

if (-not (Test-Path $prefix)) {
    Write-Host "  Installation not found at $prefix" -ForegroundColor Yellow
    # Try to find it from the script location
    $prefix = Split-Path $PSScriptRoot -Parent
    $binDir = Join-Path $prefix "bin"
    if (-not (Test-Path $prefix)) {
        Write-Host "  Could not find installation. Proceeding with PATH cleanup." -ForegroundColor Yellow
    }
}

# ── Remove from PATH ────────────────────────────────────────────────────────
Write-Host "`n🔧 Removing from PATH..." -ForegroundColor Green
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -like "*$binDir*") {
    $newPath = ($userPath.Split(';') | Where-Object { $_ -ne $binDir }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "  ✓ Removed from PATH" -ForegroundColor Green
}

$machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
if ($machinePath -like "*$binDir*") {
    $newPath = ($machinePath.Split(';') | Where-Object { $_ -ne $binDir }) -join ';'
    [Environment]::SetEnvironmentVariable("Path", $newPath, "Machine")
    Write-Host "  ✓ Removed from system PATH" -ForegroundColor Green
}

# ── Remove Start Menu shortcut ──────────────────────────────────────────────
Write-Host "`n📌 Removing Start Menu shortcut..." -ForegroundColor Green
$startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\rgytui"
if (Test-Path $startMenuDir) {
    Remove-Item -Path $startMenuDir -Recurse -Force
    Write-Host "  ✓ Shortcut removed" -ForegroundColor Green
}

# ── Remove installation directory ───────────────────────────────────────────
Write-Host "`n🗑  Removing installation directory..." -ForegroundColor Green
if (Test-Path $prefix) {
    Remove-Item -Path $prefix -Recurse -Force
    Write-Host "  ✓ Removed $prefix" -ForegroundColor Green
}

# ── Remove config (optional) ────────────────────────────────────────────────
$configDir = Join-Path $env:APPDATA "rgytui"
if (-not $RemoveConfig) {
    $choice = Read-Host "`nRemove configuration and playlists? [y/N]"
    if ($choice -eq "y") { $RemoveConfig = $true }
}

if ($RemoveConfig -and (Test-Path $configDir)) {
    Remove-Item -Path $configDir -Recurse -Force
    Write-Host "  ✓ Config removed" -ForegroundColor Green
} elseif (Test-Path $configDir) {
    Write-Host "  Config kept at $configDir" -ForegroundColor Gray
}

# ── Remove dependencies (optional) ─────────────────────────────────────────
if (-not $RemoveDeps) {
    Write-Host "`n"
    Write-Host "⚠ ⚠ ⚠  WARNING  ⚠ ⚠ ⚠" -ForegroundColor Yellow -BackgroundColor DarkRed
    Write-Host "yt-dlp and mpv are general-purpose tools that MAY be used by" -ForegroundColor Yellow
    Write-Host "other programs on your system (e.g., yt-dlg, mpv.net, others)." -ForegroundColor Yellow
    Write-Host "Only proceed if you are sure no other application depends on them." -ForegroundColor Yellow
    Write-Host "⚠ ⚠ ⚠ ⚠ ⚠ ⚠ ⚠ ⚠ ⚠ ⚠ ⚠" -ForegroundColor Yellow -BackgroundColor DarkRed
    $choice = Read-Host "`nRemove yt-dlp and mpv? [y/N]"
    if ($choice -eq "y") { $RemoveDeps = $true }
}

if ($RemoveDeps) {
    Write-Host "`n🗑  Removing dependencies..." -ForegroundColor Green
    try {
        winget uninstall --id yt-dlp.yt-dlp --silent 2>$null
        Write-Host "  ✓ yt-dlp removed" -ForegroundColor Green
    } catch {
        Write-Host "  yt-dlp not found or could not be removed" -ForegroundColor Yellow
    }
    try {
        winget uninstall --id mpv.mpv --silent 2>$null
        Write-Host "  ✓ mpv removed" -ForegroundColor Green
    } catch {
        Write-Host "  mpv not found or could not be removed" -ForegroundColor Yellow
    }
}

# ── Done ────────────────────────────────────────────────────────────────────
Write-Host "`n╔══════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║           Uninstallation Complete            ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════╝" -ForegroundColor Cyan

Write-Host "`nPress any key to close..." -ForegroundColor Gray
$null = $Host.UI.RawUI.ReadKey("NoEcho,IncludeKeyDown")
