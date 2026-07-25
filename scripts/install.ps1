<#
.SYNOPSIS
    rgytui installer — Windows
.DESCRIPTION
    Installs yt-dlp + mpv, builds rgytui from source, installs to
    %LOCALAPPDATA%\rgytui\bin\, and adds it to the user PATH.
    Run this in PowerShell as Administrator.
#>

$ErrorActionPreference = "Stop"

$RepoUrl  = "https://github.com/rojasape/rgytui.git"
$HomeDir  = "$env:LOCALAPPDATA\rgytui"
$RepoDir  = "$HomeDir\repo"
$BinDir   = "$HomeDir\bin"
$RgytuiExe = "$BinDir\rgytui.exe"

# ── Helper: add directory to user PATH if not already there ─────────────────

function Add-ToPath {
    param([string]$Dir)
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($current -split ";" -notcontains $Dir) {
        $new = if ($current) { "$current;$Dir" } else { $Dir }
        [Environment]::SetEnvironmentVariable("Path", $new, "User")
        Write-Host "  Added '$Dir' to PATH (user)."
    } else {
        Write-Host "  '$Dir' already in PATH."
    }
}

# ── Helper: ensure an .exe is available, install if missing ─────────────────

function Ensure-InPath {
    param(
        [string]$Name,
        [string]$WingetId
    )
    $found = Get-Command $Name -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "  ✓ $Name found at $($found.Source)"
        return
    }

    Write-Host "  :: Installing $Name via winget (exact ID: $WingetId)..."
    winget install --exact --id $WingetId --accept-package-agreements --accept-source-agreements
    if (-not $?) {
        throw "winget install failed for $WingetId"
    }

    # Re-check; if still not in PATH, find the .exe and fix it
    $found = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $found) {
        Write-Host "  $Name installed but not in PATH. Searching..."
        $candidates = @(
            "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\**\$Name.exe"
            "$env:USERPROFILE\AppData\Local\Programs\**\$Name.exe"
            "${env:ProgramFiles}\**\$Name.exe"
            "${env:ProgramFiles(x86)}\**\$Name.exe"
        )
        $exePath = $null
        foreach ($pattern in $candidates) {
            $matches = Get-ChildItem -Path $pattern -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($matches) { $exePath = $matches.DirectoryName; break }
        }
        if (-not $exePath) {
            throw "Could not find $Name.exe after installation. Please install manually."
        }
        Add-ToPath -Dir $exePath
        Write-Host "  ✓ $Name PATH added (from $exePath)"
    }
}

# ── Install dependencies ────────────────────────────────────────────────────

Write-Host "=== rgytui installer (Windows) ==="
Write-Host ""

# yt-dlp (mandatory)
Write-Host ":: Installing yt-dlp..."
Ensure-InPath -Name "yt-dlp" -WingetId "yt-dlp.yt-dlp"

# mpv (optional for video mode, but install anyway)
Write-Host ":: Installing mpv..."
Ensure-InPath -Name "mpv" -WingetId "shinchiro.mpv"

# ── Clone / update repo ─────────────────────────────────────────────────────

if (-not (Test-Path -Path $RepoDir)) {
    Write-Host ":: Cloning rgytui into $RepoDir..."
    New-Item -ItemType Directory -Path $HomeDir -Force | Out-Null
    git clone $RepoUrl $RepoDir
} else {
    Write-Host ":: Repository exists, updating..."
    Push-Location $RepoDir
    git fetch --ff-only
    git pull --ff-only
    Pop-Location
}

# ── Build ───────────────────────────────────────────────────────────────────

Write-Host ":: Building rgytui (release)..."
Push-Location $RepoDir
cargo build --release
Pop-Location

# ── Install binary ──────────────────────────────────────────────────────────

Write-Host ":: Installing rgytui.exe to $BinDir..."
New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
Copy-Item "$RepoDir\target\release\rgytui.exe" -Destination $RgytuiExe -Force

# ── Add to PATH ─────────────────────────────────────────────────────────────

Write-Host ":: Adding rgytui to PATH..."
Add-ToPath -Dir $BinDir

$env:Path = [Environment]::GetEnvironmentVariable("Path", "User")

Write-Host ""
Write-Host "✓ rgytui installed successfully!"
Write-Host "  Binary: $RgytuiExe"
Write-Host "  Run 'rgytui' to start."
Write-Host ""
Write-Host "Note: You may need to restart your terminal for PATH changes to take effect."
