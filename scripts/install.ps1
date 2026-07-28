<#
.SYNOPSIS
    rgytui installer — Windows
.DESCRIPTION
    Downloads pre-built binary from GitHub Releases.
    Falls back to source build with -BuildFromSource switch.

    Runtime dependencies (yt-dlp + mpv) are installed via winget or direct download.
    The binary is installed to %LOCALAPPDATA%\rgytui\bin\ and added to PATH.
#>

param(
    [switch]$BuildFromSource,
    [switch]$Nightly,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$RepoOwner = "SantosRojas"
$RepoName  = "rgytui"
$RepoUrl   = if ($env:RGYTUI_REPO) { $env:RGYTUI_REPO } else { "https://github.com/${RepoOwner}/${RepoName}.git" }
$HomeDir   = "$env:LOCALAPPDATA\rgytui"
$RepoDir   = "$HomeDir\repo"
$BinDir    = "$HomeDir\bin"
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
        "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\**\$Name.com"
        "${env:ProgramFiles}\**\$Name.com"
        "${env:ProgramFiles(x86)}\**\$Name.com"
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
        winget list --exact --id "yt-dlp.yt-dlp" 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  ✓ yt-dlp already installed via winget"
        } else {
            Write-Host "  :: Installing yt-dlp via winget..."
            winget install --exact --id "yt-dlp.yt-dlp" --accept-package-agreements --accept-source-agreements
            if ($LASTEXITCODE -ne 0) {
                Write-Host "  ⚠ winget install reported an issue — checking fallback..."
            }
        }
    } else {
        Write-Host "  :: winget not found. Downloading yt-dlp directly..."
        New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
        $url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
        Invoke-WebRequest -Uri $url -OutFile "$BinDir\yt-dlp.exe" -UseBasicParsing
        Write-Host "  ✓ yt-dlp.exe downloaded to $BinDir"
    }

    $found = Get-Command "yt-dlp" -ErrorAction SilentlyContinue
    if (-not $found) {
        Write-Host "  :: Adding yt-dlp to PATH..."
        $dir = Find-InstalledExe -Name "yt-dlp"
        if (-not $dir) {
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

    if ($wingetAvailable) {
        Write-Host "  :: Updating winget sources..."
        winget source update 2>&1 | Out-Null

        $wingetIds = @(
            "shinchiro.mpv"
            "mpv-player.mpv-CI.MSVC"
        )

        foreach ($id in $wingetIds) {
            Write-Host "  :: Installing mpv via winget ($id)..."
            winget install --exact --id $id --accept-package-agreements --accept-source-agreements 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0) {
                Write-Host "  ✓ mpv installed via winget ($id)"
                $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                            [Environment]::GetEnvironmentVariable("Path", "User")
                $mpvDir = Find-InstalledExe -Name "mpv"
                if (-not $mpvDir) {
                    $candidates = @(
                        "${env:ProgramFiles}\MPV Player"
                        "${env:ProgramFiles(x86)}\MPV Player"
                        "$env:LOCALAPPDATA\Programs\mpv-player"
                    )
                    foreach ($c in $candidates) {
                        if (Test-Path "$c\mpv.com" -or Test-Path "$c\mpv.exe") {
                            $mpvDir = $c; break
                        }
                    }
                }
                if ($mpvDir) {
                    Add-ToPath -Dir $mpvDir
                    Write-Host "  ✓ mpv ready at $mpvDir"
                }
                return
            }
            Write-Host "  ⚠ winget install failed for $id"
        }
    } else {
        Write-Host "  ⚠ winget not found. Will try direct download..."
    }

    Write-Host "  :: Trying direct download of mpv portable..."

    $mpvVersion = "0.41.0"
    $url = "https://downloads.sourceforge.net/project/mpv-player-windows/release/mpv-${mpvVersion}-x86_64.7z"
    $archivePath = "$env:TEMP\mpv.7z"
    $extractDir = "$BinDir\mpv"

    try {
        Invoke-WebRequest -Uri $url -OutFile $archivePath -UseBasicParsing -ErrorAction Stop
        Write-Host "  ✓ Downloaded mpv archive. Extracting..."

        New-Item -ItemType Directory -Path $extractDir -Force | Out-Null

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

        $mpvExe = Get-ChildItem -Path $extractDir -Recurse -Filter "mpv.exe" -ErrorAction SilentlyContinue |
                  Select-Object -First 1

        if ($mpvExe) {
            $mpvDirLocal = $mpvExe.DirectoryName
            if ($mpvDirLocal -ne $extractDir) {
                Get-ChildItem -Path $mpvDirLocal -File | Move-Item -Destination $extractDir -Force
                Get-ChildItem -Path $mpvDirLocal -Directory | Move-Item -Destination $extractDir -Force
                Remove-Item $mpvDirLocal -Force -ErrorAction SilentlyContinue
            }
            Write-Host "  ✓ mpv.exe ready at $extractDir"
            Add-ToPath -Dir $extractDir
        } else {
            Write-Host "  ⚠ Downloaded archive but mpv.exe not found inside."
        }
    } catch {
        Write-Host "  ⚠ Direct download failed: $_"
    }

    Remove-Item $archivePath -Force -ErrorAction SilentlyContinue
    Remove-Item "$env:TEMP\7zr.exe" -Force -ErrorAction SilentlyContinue

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

    try {
        $vcLibs = "https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx"
        $vcOut = "$env:TEMP\VCLibs.appx"
        Invoke-WebRequest -Uri $vcLibs -OutFile $vcOut -UseBasicParsing
        Add-AppxPackage -Path $vcOut -ErrorAction SilentlyContinue
    } catch {
        Write-Host "  ⚠ Could not install VCLibs dependency: $_"
    }

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
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
    Write-Host "  ✓ Rust installed."
}

# ── Build from source ────────────────────────────────────────────────────────

function Build-FromSource {
    if (-not (Test-Path -Path $RepoDir)) {
        Write-Host ":: Cloning rgytui into $RepoDir..."
        New-Item -ItemType Directory -Path $HomeDir -Force | Out-Null
        git clone $RepoUrl $RepoDir
    } else {
        Write-Host ":: Repository exists, updating..."
        Push-Location $RepoDir
        git fetch
        git pull --ff-only
        Pop-Location
    }

    Ensure-Rust

    Write-Host ":: Building rgytui (release)..."
    Push-Location $RepoDir
    cargo build --release
    Pop-Location

    Write-Host ":: Installing rgytui.exe to $BinDir..."
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    Copy-Item "$RepoDir\target\release\rgytui.exe" -Destination $RgytuiExe -Force

    Write-Host ":: Adding rgytui to PATH..."
    Add-ToPath -Dir $BinDir
}

# ── Pre-built binary download ────────────────────────────────────────────────

function Install-PreBuilt {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -ne "AMD64") {
        Write-Host "ERROR: unsupported architecture ($arch)."
        Write-Host "       Supported: AMD64 (x86_64)."
        Write-Host "       Try -BuildFromSource to compile for your architecture."
        exit 1
    }

    $target = "x86_64-pc-windows-msvc"
    $archiveName = "rgytui-$target.zip"

    $label = if ($Nightly) { 'nightly' } else { 'latest' }
    $releasesUrl = if ($Nightly) {
        "https://api.github.com/repos/${RepoOwner}/${RepoName}/releases/tags/nightly"
    } else {
        "https://api.github.com/repos/${RepoOwner}/${RepoName}/releases/latest"
    }

    Write-Host ":: Querying $label release..."
    try {
        $release = Invoke-RestMethod -Uri $releasesUrl -UseBasicParsing -ErrorAction Stop
    } catch {
        Write-Host "ERROR: Failed to query GitHub API."
        Write-Host "       No release found or network error."
        Write-Host "       Try building from source:"
        Write-Host "         .\install.ps1 -BuildFromSource"
        exit 1
    }

    $asset = $release.assets | Where-Object { $_.name -eq $archiveName } | Select-Object -First 1
    if (-not $asset) {
        Write-Host "ERROR: Could not find asset '$archiveName' in the release."
        Write-Host "       Available assets:"
        $release.assets | ForEach-Object { Write-Host "         - $($_.name)" }
        Write-Host ""
        Write-Host "       Try building from source:"
        Write-Host "         .\install.ps1 -BuildFromSource"
        exit 1
    }

    Write-Host ":: Downloading $archiveName..."
    $archivePath = "$env:TEMP\$archiveName"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath -UseBasicParsing
    Write-Host "  ✓ Downloaded."

    Write-Host ":: Extracting..."
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    Expand-Archive -Path $archivePath -DestinationPath $BinDir -Force
    Remove-Item $archivePath -Force -ErrorAction SilentlyContinue
    Write-Host "  ✓ Installed to $RgytuiExe"

    Write-Host ":: Adding rgytui to PATH..."
    Add-ToPath -Dir $BinDir
}

# ── Main ────────────────────────────────────────────────────────────────────

Write-Host "=== rgytui installer (Windows) ==="
Write-Host ""

if ($BuildFromSource) {
    Write-Host ":: Building from source..."
    Write-Host ""

    $WingetAvailable = Try-InstallWinget

    Write-Host ":: Installing yt-dlp..."
    Ensure-YtDlp

    Write-Host ":: Installing mpv..."
    Ensure-Mpv

    Build-FromSource
} else {
    if ((Test-Path -Path $RgytuiExe) -and (-not $Force)) {
        Write-Host ":: rgytui is already installed at $RgytuiExe"
        try {
            $response = Read-Host "  Overwrite? [y/N]"
        } catch {
            Write-Host "  ERROR: Cannot prompt for input. Re-run with -Force to overwrite."
            exit 1
        }
        if ($response -notmatch '^[yY]') {
            Write-Host "  Installation cancelled."
            exit 0
        }
    }

    Install-PreBuilt

    Write-Host ":: Installing yt-dlp..."
    Ensure-YtDlp

    Write-Host ":: Installing mpv..."
    Ensure-Mpv
}

$env:Path = [Environment]::GetEnvironmentVariable("Path", "User")

Write-Host ""
Write-Host "✓ rgytui installed successfully!"
Write-Host "  Binary: $RgytuiExe"
Write-Host "  Run 'rgytui' to start."
Write-Host ""
Write-Host "Note: You may need to restart your terminal for PATH changes to take effect."
