@echo off
setlocal

powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-windows.ps1" %*
set "EXIT_CODE=%ERRORLEVEL%"

echo.
if not "%EXIT_CODE%"=="0" (
    echo SoundGIF installation failed.
) else (
    echo SoundGIF installation finished.
)
pause
exit /b %EXIT_CODE%
