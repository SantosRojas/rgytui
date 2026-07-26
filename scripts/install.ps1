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

$RepoUrl  = "https://github.com/SantosRojas/rgytui.git"
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

    $wingetAvailable = Get-Command winget -ErrorAction SilentlyContinue

    # ── Strategy 1: winget with multiple IDs ────────────────────────────
    if ($wingetAvailable) {
        Write-Host "  :: Updating winget sources..."
        winget source update 2>&1 | Out-Null

        $wingetIds = @(
            "shinchiro.mpv"           # Community build (most common)
            "mpv-player.mpv-CI.MSVC"  # Official CI build
        )

        foreach ($id in $wingetIds) {
            Write-Host "  :: Installing mpv via winget ($id)..."
            winget install --exact --id $id --accept-package-agreements --accept-source-agreements 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0) {
                Write-Host "  ✓ mpv installed via winget ($id)"
                break
            }
            Write-Host "  ⚠ winget install failed for $id"
        }
    } else {
        Write-Host "  ⚠ winget not found. Will try direct download..."
    }

    # ── Re-check PATH after winget ──────────────────────────────────────
    $found = Get-Command "mpv" -ErrorAction SilentlyContinue
    if ($found) {
        Write-Host "  ✓ mpv found at $($found.Source)"
        return
    }

    # ── Strategy 2: direct download of portable 7z ─────────────────────
    Write-Host "  :: Trying direct download of mpv portable..."

    $mpvVersion = "0.41.0"
    $url = "https://downloads.sourceforge.net/project/mpv-player-windows/release/mpv-${mpvVersion}-x86_64.7z"
    $archivePath = "$env:TEMP\mpv.7z"
    $extractDir = "$BinDir\mpv"

    try {
        Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing -ErrorAction Stop
        Write-Host "  ✓ Downloaded mpv archive. Extracting..."

        New-Item -ItemType Directory -Path $extractDir -Force | Out-Null

        # Try 7z (either on PATH or standalone 7zr.exe)
        $7zExe = if (Get-Command 7z -ErrorAction SilentlyContinue) { "7z" }
                 elseif (Get-Command 7zr -ErrorAction SilentlyContinue) { "7zr" }
                 else { $null }

        if (-not $7zExe) {
            Write-Host "  :: Downloading 7zr standalone extractor..."
            $7zrUrl = "https://www.7-zip.org/a/7zr.exe"
            $7zrPath = "$env:TEMP\7zr.exe"
            Invoke-WebRequest -Uri $7zrUrl -OutFile $7zrPath -UseBasicParsing -ErrorAction Stop
            $7zExe = $7zrPath
        }

        & $7zExe x $archivePath -o"$extractDir" -y 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  ✓ Archive extracted."
        } else {
            throw "Extraction failed with exit code $LASTEXITCODE"
        }

        # Find mpv.exe — it might be in a versioned subdirectory inside the archive
        $mpvExe = Get-ChildItem -Path $extractDir -Recurse -Filter "mpv.exe" -ErrorAction SilentlyContinue |
                  Select-Object -First 1

        if ($mpvExe) {
            $mpvDir = $mpvExe.DirectoryName
            # Flatten: if it's in a subdirectory, move everything up
            if ($mpvDir -ne $extractDir) {
                Get-ChildItem -Path $mpvDir -File | Move-Item -Destination $extractDir -Force
                Get-ChildItem -Path $mpvDir -Directory | Move-Item -Destination $extractDir -Force
                Remove-Item $mpvDir -Force -ErrorAction SilentlyContinue
            }
            Write-Host "  ✓ mpv.exe ready at $extractDir"
            Add-ToPath -Dir $extractDir
        } else {
            Write-Host "  ⚠ Downloaded archive but mpv.exe not found inside."
        }
    } catch {
        Write-Host "  ⚠ Direct download failed: $_"
    }

    # Clean up temp files
    Remove-Item $archivePath -Force -ErrorAction SilentlyContinue
    Remove-Item "$env:TEMP\7zr.exe" -Force -ErrorAction SilentlyContinue

    # ── Final check: search for installed exe and add to PATH ──────────
    $dir = Find-InstalledExe -Name "mpv"
    if ($dir) {
        Add-ToPath -Dir $dir
        return
    }

    Write-Host "  ⚠ mpv could not be installed automatically."
    Write-Host "    Install manually from: https://mpv.io/install/"
    Write-Host "    Or try: winget install shinchiro.mpv"
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
