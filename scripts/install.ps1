<#
.SYNOPSIS
    rgytui installer — Windows
.DESCRIPTION
    Installs yt-dlp + mpv, builds rgytui from source, installs to
    %LOCALAPPDATA%\rgytui\bin\, and adds it to the user PATH.

    Dependencies are installed via winget (when available) or downloaded
    directly from GitHub releases as a fallback.
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
        Write-Host "  ✓ Added '$Dir' to PATH (user)."
    } else {
        Write-Host "  ✓ '$Dir' already in PATH."
    }
}

# ── Helper: find an installed .exe by scanning common locations ─────────────

function Find-InstalledExe {
    param([string]$Name)
    $candidates = @(
        "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\**\$Name.exe"
        "$env:USERPROFILE\AppData\Local\Programs\**\$Name.exe"
        "${env:ProgramFiles}\**\$Name.exe"
        "${env:ProgramFiles(x86)}\**\$Name.exe"
        "$BinDir\$Name.exe"
    )
    foreach ($pattern in $candidates) {
        $match = Get-ChildItem -Path $pattern -Recurse -ErrorAction SilentlyContinue |
                 Select-Object -First 1
        if ($match) { return $match.DirectoryName }
    }
    return $null
}

# ── Ensure yt-dlp is available ─────────────────────────────────────────────

function Ensure-YtDlp {
    $found = Get-Command "yt-dlp" -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "  ✓ yt-dlp found at $($found.Source)"
        return
    }

    if (Get-Command winget -ErrorAction SilentlyContinue) {
        Write-Host "  :: Installing yt-dlp via winget..."
        winget install --exact --id "yt-dlp.yt-dlp" --accept-package-agreements --accept-source-agreements
        if (-not $?) { throw "winget install failed for yt-dlp.yt-dlp" }
    } else {
        Write-Host "  :: winget not found. Downloading yt-dlp directly..."
        New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
        $url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
        Invoke-WebRequest -Uri $url -OutFile "$BinDir\yt-dlp.exe" -UseBasicParsing
        Write-Host "  ✓ yt-dlp.exe downloaded to $BinDir"
    }

    # Re-check; if still not in PATH, search and fix
    $found = Get-Command "yt-dlp" -ErrorAction SilentlyContinue
    if (-not $found) {
        Write-Host "  :: Adding yt-dlp to PATH..."
        $dir = Find-InstalledExe -Name "yt-dlp"
        if (-not $dir) {
            # If we downloaded it ourselves, it's in $BinDir
            if (Test-Path "$BinDir\yt-dlp.exe") { $dir = $BinDir }
        }
        if ($dir) { Add-ToPath -Dir $dir } else {
            throw "Could not find yt-dlp.exe. Please install manually from https://github.com/yt-dlp/yt-dlp"
        }
    }
}

# ── Ensure mpv is available (optional) ──────────────────────────────────────

function Ensure-Mpv {
    $found = Get-Command "mpv" -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "  ✓ mpv found at $($found.Source)"
        return
    }

    if (Get-Command winget -ErrorAction SilentlyContinue) {
        Write-Host "  :: Installing mpv via winget..."
        winget install --exact --id "shinchiro.mpv" --accept-package-agreements --accept-source-agreements
        if (-not $?) {
            Write-Host "  ⚠ winget install for mpv failed. You can install it manually."
            Write-Host "    https://mpv.io/install/"
            return
        }
    } else {
        Write-Host "  ⚠ winget not found. Skipping mpv — install manually for video mode:"
        Write-Host "    https://mpv.io/install/"
        return
    }

    # Re-check; if still not in PATH, search and fix
    $found = Get-Command "mpv" -ErrorAction SilentlyContinue
    if (-not $found) {
        Write-Host "  :: Adding mpv to PATH..."
        $dir = Find-InstalledExe -Name "mpv"
        if ($dir) { Add-ToPath -Dir $dir } else {
            Write-Host "  ⚠ Could not locate mpv.exe after install. Add it to PATH manually."
            Write-Host "    https://mpv.io/install/"
        }
    }
}

# ── Try to bootstrap winget if missing ──────────────────────────────────────

function Try-InstallWinget {
    if (Get-Command winget -ErrorAction SilentlyContinue) { return $true }

    Write-Host "  winget not found, attempting to install App Installer from Microsoft..."
    Write-Host "  (Downloading ~100 MB — this may take a moment)"

    # Ensure VCLibs dependency (required for App Installer)
    try {
        $vcLibs = "https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx"
        $vcOut = "$env:TEMP\VCLibs.appx"
        Invoke-WebRequest -Uri $vcLibs -OutFile $vcOut -UseBasicParsing
        Add-AppxPackage -Path $vcOut -ErrorAction SilentlyContinue
    } catch {
        Write-Host "  ⚠ Could not install VCLibs dependency: $_"
    }

    # Download and install App Installer (winget)
    try {
        $url = "https://github.com/microsoft/winget-cli/releases/latest/download/Microsoft.DesktopAppInstaller_8wekyb3d8bbwe.msixbundle"
        $out = "$env:TEMP\AppInstaller.msixbundle"
        Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
        Add-AppxPackage -Path $out -ErrorAction Stop
        Write-Host "  ✓ winget installed successfully."
        return $true
    } catch {
        Write-Host "  ⚠ Could not install winget automatically: $_"
        Write-Host "  (will fall back to direct download for dependencies)"
        return $false
    }
}

# ── Main ────────────────────────────────────────────────────────────────────

Write-Host "=== rgytui installer (Windows) ==="
Write-Host ""

# Bootstrap winget if possible — this makes the rest smoother
$WingetAvailable = Try-InstallWinget

# yt-dlp (mandatory)
Write-Host ":: Installing yt-dlp..."
Ensure-YtDlp

# mpv (optional)
Write-Host ":: Installing mpv..."
Ensure-Mpv

# ── Ensure Rust is installed ────────────────────────────────────────────────

function Ensure-Rust {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "  ✓ Rust already installed (cargo found)."
        return
    }
    Write-Host "  :: Installing Rust via rustup..."
    $url = "https://win.rustup.rs/x86_64"
    $out = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
    & $out -y --default-toolchain stable --profile default
    # Add cargo to PATH for the rest of the script
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    Write-Host "  ✓ Rust installed."
}

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

Ensure-Rust

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
