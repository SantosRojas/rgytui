@echo off
REM rgytui installer for Windows
REM
REM Downloads and runs scripts/install.ps1 from the rgytui repository.
REM
REM Usage:
REM   powershell -c "iwr -Uri https://raw.githubusercontent.com/SantosRojas/rgytui/master/scripts/install.cmd -OutFile install.cmd -UseBasicParsing; .\install.cmd"

setlocal
set "PS1_URL=https://raw.githubusercontent.com/SantosRojas/rgytui/master/scripts/install.ps1"
set "PS1_FILE=%TEMP%\rgytui-install.ps1"

echo :: Downloading installer script...
powershell -NoProfile -Command "iwr -Uri '%PS1_URL%' -OutFile '%PS1_FILE%' -UseBasicParsing"
if %errorlevel% neq 0 (
    echo Failed to download installer script.
    pause
    exit /b 1
)

echo :: Running installer...
powershell -NoProfile -ExecutionPolicy RemoteSigned -File "%PS1_FILE%"
set "EXIT_CODE=%errorlevel%"

del "%PS1_FILE%" >nul 2>&1

if %EXIT_CODE% neq 0 (
    echo.
    echo The installer encountered an error. Check the output above.
    pause
    exit /b 1
)
