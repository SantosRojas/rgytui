@echo off
REM rgytui installer for Windows
REM Double-click this file to install rgytui and all dependencies.
REM This script is self-contained — no other files needed.

REM Check if running as admin using net session
>nul 2>&1 net session
if '%errorlevel%' NEQ '0' (
    echo Requesting administrator privileges...
    powershell -Command "Start-Process '%~f0' -Verb RunAs"
    exit /b
)
cd /d "%~dp0"

echo === rgytui installer for Windows ===
echo.
echo Step 1 of 5: Checking dependencies...
echo.

where cargo >nul 2>&1
if %errorlevel% neq 0 (
    echo Rust not found. Downloading rustup-init.exe...
    powershell -NoProfile -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri 'https://win.rustup.rs/x86_64' -OutFile '%TEMP%\rustup-init.exe' -UseBasicParsing"
    echo Installing Rust (this may take a few minutes)...
    "%TEMP%\rustup-init.exe" -y --default-toolchain stable --profile default
    set PATH=%USERPROFILE%\.cargo\bin;%PATH%
    echo ✓ Rust installed.
) else (
    echo ✓ Rust already installed.
)

where yt-dlp >nul 2>&1
if %errorlevel% neq 0 (
    echo yt-dlp not found. Downloading...
    if not exist "%LOCALAPPDATA%\rgytui\bin" mkdir "%LOCALAPPDATA%\rgytui\bin"
    powershell -NoProfile -Command "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' -OutFile '%LOCALAPPDATA%\rgytui\bin\yt-dlp.exe' -UseBasicParsing"
    echo ✓ yt-dlp downloaded.
) else (
    echo ✓ yt-dlp already installed.
)

where mpv >nul 2>&1
if %errorlevel% neq 0 (
    echo mpv not found. Skipping (optional — install manually for video mode).
    echo   Download from: https://mpv.io/install/
) else (
    echo ✓ mpv already installed.
)

echo.
echo Step 2 of 5: Cloning rgytui repository...
if not exist "%LOCALAPPDATA%\rgytui\repo" (
    mkdir "%LOCALAPPDATA%\rgytui"
    git clone --depth=1 https://github.com/SantosRojas/rgytui.git "%LOCALAPPDATA%\rgytui\repo"
) else (
    echo Repository exists, updating...
    cd /d "%LOCALAPPDATA%\rgytui\repo"
    git pull --ff-only
)

echo.
echo Step 3 of 5: Building rgytui (this may take a while)...
cd /d "%LOCALAPPDATA%\rgytui\repo"
cargo build --release
if %errorlevel% neq 0 (
    echo ✗ Build failed. Check the error messages above.
    pause
    exit /b 1
)

echo.
echo Step 4 of 5: Installing rgytui...
if not exist "%LOCALAPPDATA%\rgytui\bin" mkdir "%LOCALAPPDATA%\rgytui\bin"
copy /y "%LOCALAPPDATA%\rgytui\repo\target\release\rgytui.exe" "%LOCALAPPDATA%\rgytui\bin\rgytui.exe" >nul

echo.
echo Step 5 of 5: Adding to PATH...
powershell -NoProfile -Command ^
    "$p = [Environment]::GetEnvironmentVariable('Path', 'User');" ^
    "if ($p -notlike '*%LOCALAPPDATA:\=\\%\\rgytui\\bin*') {" ^
    "  [Environment]::SetEnvironmentVariable('Path', "$p;%LOCALAPPDATA%\rgytui\bin", 'User')" ^
    "}"

echo.
echo ========================================
echo  ✓ rgytui installed successfully!
echo ========================================
echo.
echo  Run 'rgytui' from any terminal.
echo  To update later, run: rgytui update
echo.
echo  You may need to restart your terminal
echo  for PATH changes to take effect.
echo.
pause
