@echo off
setlocal EnableExtensions
title SoundGIF for Vencord

set "SOUNDGIF_REPOSITORY=https://github.com/Superwheat/soundGIF.git"
set "VENCORD_REPOSITORY=https://github.com/Vendicated/Vencord.git"

where git >nul 2>&1 || (
    echo Git is required: https://git-scm.com/download/win
    goto :fail
)
where node >nul 2>&1 || (
    echo Node.js is required: https://nodejs.org/
    goto :fail
)

where pnpm >nul 2>&1
if not errorlevel 1 (
    set "PNPM_MODE=pnpm"
) else (
    where corepack >nul 2>&1 || (
        echo pnpm is required: https://pnpm.io/installation
        goto :fail
    )
    set "PNPM_MODE=corepack"
)

set "MANAGED_ROOT=%LOCALAPPDATA%\SoundGIF"
if "%LOCALAPPDATA%"=="" set "MANAGED_ROOT=%USERPROFILE%\AppData\Local\SoundGIF"

if not defined VENCORD_DIR for /f "usebackq delims=" %%D in (`powershell.exe -NoLogo -NoProfile -Command "[Environment]::GetFolderPath('MyDocuments')"`) do set "DOCUMENTS_DIR=%%D"
if not defined VENCORD_DIR if defined DOCUMENTS_DIR if exist "%DOCUMENTS_DIR%\Vencord\package.json" set "VENCORD_DIR=%DOCUMENTS_DIR%\Vencord"
if not defined VENCORD_DIR if exist "%USERPROFILE%\Documents\Vencord\package.json" set "VENCORD_DIR=%USERPROFILE%\Documents\Vencord"
if not defined VENCORD_DIR if exist "%USERPROFILE%\Vencord\package.json" set "VENCORD_DIR=%USERPROFILE%\Vencord"
if not defined VENCORD_DIR set "VENCORD_DIR=%MANAGED_ROOT%\Vencord"

set "SOUNDGIF_CHECKOUT=%MANAGED_ROOT%\source"
set "PLUGIN_SOURCE=%SOUNDGIF_CHECKOUT%\plugins\vencord\soundGif"
set "PLUGIN_TARGET=%VENCORD_DIR%\src\userplugins\soundGif"

echo Vencord source: %VENCORD_DIR%

if not exist "%VENCORD_DIR%\.git\" (
    if exist "%VENCORD_DIR%\" (
        for /f "delims=" %%F in ('dir /b /a "%VENCORD_DIR%" 2^>nul') do goto :bad_vencord_dir
    )
    if not exist "%MANAGED_ROOT%\" mkdir "%MANAGED_ROOT%" || goto :fail
    git clone "%VENCORD_REPOSITORY%" "%VENCORD_DIR%" || goto :fail
) else (
    git -C "%VENCORD_DIR%" pull --ff-only || goto :fail
)

if not exist "%SOUNDGIF_CHECKOUT%\.git\" (
    if not exist "%MANAGED_ROOT%\" mkdir "%MANAGED_ROOT%" || goto :fail
    git clone --depth 1 "%SOUNDGIF_REPOSITORY%" "%SOUNDGIF_CHECKOUT%" || goto :fail
) else (
    git -C "%SOUNDGIF_CHECKOUT%" pull --ff-only || goto :fail
)

if not exist "%PLUGIN_SOURCE%\index.tsx" (
    echo SoundGIF plugin source was not found after updating.
    goto :fail
)
if not exist "%VENCORD_DIR%\package.json" (
    echo The selected folder is not a Vencord source checkout.
    goto :fail
)

echo Installing plugin: %PLUGIN_TARGET%
if not exist "%VENCORD_DIR%\src\userplugins\" mkdir "%VENCORD_DIR%\src\userplugins" || goto :fail
if exist "%PLUGIN_TARGET%\" rmdir /s /q "%PLUGIN_TARGET%" || goto :fail
xcopy "%PLUGIN_SOURCE%" "%PLUGIN_TARGET%\" /e /i /q /y >nul || goto :fail

pushd "%VENCORD_DIR%" || goto :fail
call :run_pnpm install --frozen-lockfile
if errorlevel 1 goto :fail_from_vencord
call :run_pnpm build
if errorlevel 1 goto :fail_from_vencord

if not "%SOUNDGIF_NO_INJECT%"=="1" (
    echo.
    echo Vencord's installer will ask which Discord client to patch.
    call :run_pnpm inject
    if errorlevel 1 goto :fail_from_vencord
)
popd

echo.
echo Restart Discord, then enable SoundGIF in Vencord's plugin settings.
echo Run this file again to update or repair the custom Vencord build.
echo.
pause
exit /b 0

:run_pnpm
if "%PNPM_MODE%"=="pnpm" (
    call pnpm %*
) else (
    call corepack pnpm %*
)
exit /b %ERRORLEVEL%

:bad_vencord_dir
echo %VENCORD_DIR% exists but is not a Vencord Git checkout.
goto :fail

:fail_from_vencord
popd

:fail
echo.
echo SoundGIF installation failed.
echo.
pause
exit /b 1
