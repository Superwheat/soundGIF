@echo off
setlocal EnableExtensions
title SoundGIF for Vencord

set "SOUNDGIF_REPOSITORY=https://github.com/Superwheat/soundGIF.git"
set "VENCORD_REPOSITORY=https://github.com/Vendicated/Vencord.git"
set "TASK_NAME=SoundGIF Vencord Auto Repair"
set "MANAGED_ROOT=%LOCALAPPDATA%\SoundGIF"
if "%LOCALAPPDATA%"=="" set "MANAGED_ROOT=%USERPROFILE%\AppData\Local\SoundGIF"
set "AUTO_DIR=%MANAGED_ROOT%\auto"
set "AUTO_SCRIPT=%AUTO_DIR%\install-windows.cmd"
set "AUTO_LOG=%MANAGED_ROOT%\auto-update.log"

if /i not "%~1"=="--auto-run" goto :menu
if not exist "%MANAGED_ROOT%\" mkdir "%MANAGED_ROOT%" >nul 2>&1
call :perform_update auto "%~2" >> "%AUTO_LOG%" 2>&1
exit /b %ERRORLEVEL%

:menu
cls
echo SoundGIF for Vencord
echo.
echo [1] Install or update now
echo [2] Install or update and enable automatic updates/repatching
echo [3] Disable automatic updates/repatching
echo [4] Exit
echo.
set "MENU_CHOICE="
set /p "MENU_CHOICE=Choose an option: "

if "%MENU_CHOICE%"=="1" goto :menu_install
if "%MENU_CHOICE%"=="2" goto :menu_install_auto
if "%MENU_CHOICE%"=="3" goto :menu_disable_auto
if "%MENU_CHOICE%"=="4" exit /b 0
goto :menu

:menu_install
call :perform_update interactive ""
goto :finish

:menu_install_auto
call :choose_branch
if errorlevel 1 goto :menu
call :perform_update direct "%AUTO_BRANCH%"
if errorlevel 1 goto :finish
call :enable_auto "%AUTO_BRANCH%"
goto :finish

:menu_disable_auto
call :disable_auto
goto :finish

:choose_branch
echo.
echo Which Discord version should be kept patched?
echo [1] Stable
echo [2] PTB
echo [3] Canary
echo [4] Cancel
echo.
set "BRANCH_CHOICE="
set /p "BRANCH_CHOICE=Choose a version: "
if "%BRANCH_CHOICE%"=="1" goto :branch_stable
if "%BRANCH_CHOICE%"=="2" goto :branch_ptb
if "%BRANCH_CHOICE%"=="3" goto :branch_canary
exit /b 1

:branch_stable
set "AUTO_BRANCH=stable"
exit /b 0

:branch_ptb
set "AUTO_BRANCH=ptb"
exit /b 0

:branch_canary
set "AUTO_BRANCH=canary"
exit /b 0

:perform_update
set "RUN_MODE=%~1"
set "PATCH_BRANCH=%~2"
if "%PATCH_BRANCH%"=="" set "PATCH_BRANCH=stable"

call :require_tools
if errorlevel 1 exit /b 1
call :resolve_paths

set "OLD_VENCORD_HEAD="
set "OLD_SOUNDGIF_HEAD="
if exist "%VENCORD_DIR%\.git\" for /f "delims=" %%H in ('git -C "%VENCORD_DIR%" rev-parse HEAD 2^>nul') do set "OLD_VENCORD_HEAD=%%H"
if exist "%SOUNDGIF_CHECKOUT%\.git\" for /f "delims=" %%H in ('git -C "%SOUNDGIF_CHECKOUT%" rev-parse HEAD 2^>nul') do set "OLD_SOUNDGIF_HEAD=%%H"

echo Vencord source: %VENCORD_DIR%
if not exist "%VENCORD_DIR%\.git\" (
    if exist "%VENCORD_DIR%\" (
        for /f "delims=" %%F in ('dir /b /a "%VENCORD_DIR%" 2^>nul') do (
            echo %VENCORD_DIR% exists but is not a Vencord Git checkout.
            exit /b 1
        )
    )
    if not exist "%MANAGED_ROOT%\" mkdir "%MANAGED_ROOT%" || exit /b 1
    git clone "%VENCORD_REPOSITORY%" "%VENCORD_DIR%" || exit /b 1
) else (
    git -C "%VENCORD_DIR%" pull --ff-only
    if errorlevel 1 if /i not "%RUN_MODE%"=="auto" exit /b 1
)

if not exist "%SOUNDGIF_CHECKOUT%\.git\" (
    if not exist "%MANAGED_ROOT%\" mkdir "%MANAGED_ROOT%" || exit /b 1
    git clone --depth 1 "%SOUNDGIF_REPOSITORY%" "%SOUNDGIF_CHECKOUT%" || exit /b 1
) else (
    git -C "%SOUNDGIF_CHECKOUT%" pull --ff-only
    if errorlevel 1 if /i not "%RUN_MODE%"=="auto" exit /b 1
)

if not exist "%PLUGIN_SOURCE%\index.tsx" (
    echo SoundGIF plugin source was not found after updating.
    exit /b 1
)
if not exist "%VENCORD_DIR%\package.json" (
    echo The selected folder is not a Vencord source checkout.
    exit /b 1
)

set "NEW_VENCORD_HEAD="
set "NEW_SOUNDGIF_HEAD="
for /f "delims=" %%H in ('git -C "%VENCORD_DIR%" rev-parse HEAD 2^>nul') do set "NEW_VENCORD_HEAD=%%H"
for /f "delims=" %%H in ('git -C "%SOUNDGIF_CHECKOUT%" rev-parse HEAD 2^>nul') do set "NEW_SOUNDGIF_HEAD=%%H"

set "NEEDS_BUILD=0"
if /i not "%RUN_MODE%"=="auto" set "NEEDS_BUILD=1"
if not "%OLD_VENCORD_HEAD%"=="%NEW_VENCORD_HEAD%" set "NEEDS_BUILD=1"
if not "%OLD_SOUNDGIF_HEAD%"=="%NEW_SOUNDGIF_HEAD%" set "NEEDS_BUILD=1"
if not exist "%PLUGIN_TARGET%\index.tsx" set "NEEDS_BUILD=1"
if not exist "%PLUGIN_TARGET%\styles.css" set "NEEDS_BUILD=1"
if exist "%PLUGIN_TARGET%\index.tsx" fc /b "%PLUGIN_SOURCE%\index.tsx" "%PLUGIN_TARGET%\index.tsx" >nul 2>&1 || set "NEEDS_BUILD=1"
if exist "%PLUGIN_TARGET%\styles.css" fc /b "%PLUGIN_SOURCE%\styles.css" "%PLUGIN_TARGET%\styles.css" >nul 2>&1 || set "NEEDS_BUILD=1"

if "%NEEDS_BUILD%"=="1" (
    echo Installing plugin: %PLUGIN_TARGET%
    if not exist "%VENCORD_DIR%\src\userplugins\" mkdir "%VENCORD_DIR%\src\userplugins" || exit /b 1
    if exist "%PLUGIN_TARGET%\" rmdir /s /q "%PLUGIN_TARGET%" || exit /b 1
    xcopy "%PLUGIN_SOURCE%" "%PLUGIN_TARGET%\" /e /i /q /y >nul || exit /b 1

    pushd "%VENCORD_DIR%" || exit /b 1
    call :run_pnpm install --frozen-lockfile
    if errorlevel 1 (
        popd
        exit /b 1
    )
    call :run_pnpm build
    if errorlevel 1 (
        popd
        exit /b 1
    )
    popd
) else (
    echo Source is already current.
)

if /i not "%RUN_MODE%"=="interactive" goto :after_interactive_patch
pushd "%VENCORD_DIR%" || exit /b 1
call :run_pnpm inject
set "PATCH_EXIT=%ERRORLEVEL%"
popd
exit /b %PATCH_EXIT%

:after_interactive_patch
if /i "%RUN_MODE%"=="direct" (
    call :run_vencord_installer install "%PATCH_BRANCH%"
    if errorlevel 1 exit /b 1
    call :is_patched "%PATCH_BRANCH%"
    if errorlevel 1 (
        echo Vencord did not patch Discord %PATCH_BRANCH%.
        exit /b 1
    )
    exit /b 0
)

call :is_patched "%PATCH_BRANCH%"
if not errorlevel 1 (
    echo Discord %PATCH_BRANCH% is already patched.
    exit /b 0
)

echo Discord %PATCH_BRANCH% is not patched. Repairing it now.
call :run_vencord_installer repair "%PATCH_BRANCH%"
if errorlevel 1 exit /b 1
call :is_patched "%PATCH_BRANCH%"
if errorlevel 1 (
    echo Vencord did not repair Discord %PATCH_BRANCH%.
    exit /b 1
)
exit /b 0

:require_tools
where git >nul 2>&1 || (
    echo Git is required: https://git-scm.com/download/win
    exit /b 1
)
where node >nul 2>&1 || (
    echo Node.js is required: https://nodejs.org/
    exit /b 1
)
where pnpm >nul 2>&1
if not errorlevel 1 (
    set "PNPM_MODE=pnpm"
    exit /b 0
)
where corepack >nul 2>&1 || (
    echo pnpm is required: https://pnpm.io/installation
    exit /b 1
)
set "PNPM_MODE=corepack"
exit /b 0

:resolve_paths
if not defined VENCORD_DIR for /f "usebackq delims=" %%D in (`powershell.exe -NoLogo -NoProfile -Command "[Environment]::GetFolderPath('MyDocuments')"`) do set "DOCUMENTS_DIR=%%D"
if not defined VENCORD_DIR if defined DOCUMENTS_DIR if exist "%DOCUMENTS_DIR%\Vencord\package.json" set "VENCORD_DIR=%DOCUMENTS_DIR%\Vencord"
if not defined VENCORD_DIR if exist "%USERPROFILE%\Documents\Vencord\package.json" set "VENCORD_DIR=%USERPROFILE%\Documents\Vencord"
if not defined VENCORD_DIR if exist "%USERPROFILE%\Vencord\package.json" set "VENCORD_DIR=%USERPROFILE%\Vencord"
if not defined VENCORD_DIR set "VENCORD_DIR=%MANAGED_ROOT%\Vencord"
set "SOUNDGIF_CHECKOUT=%MANAGED_ROOT%\source"
set "PLUGIN_SOURCE=%SOUNDGIF_CHECKOUT%\plugins\vencord\soundGif"
set "PLUGIN_TARGET=%VENCORD_DIR%\src\userplugins\soundGif"
exit /b 0

:run_pnpm
if "%PNPM_MODE%"=="pnpm" (
    call pnpm %*
) else (
    call corepack pnpm %*
)
exit /b %ERRORLEVEL%

:run_vencord_installer
pushd "%VENCORD_DIR%" || exit /b 1
node scripts\runInstaller.mjs -- --%~1 --branch "%~2"
set "INSTALLER_EXIT=%ERRORLEVEL%"
popd
exit /b %INSTALLER_EXIT%

:is_patched
set "DISCORD_FOLDER=Discord"
if /i "%~1"=="ptb" set "DISCORD_FOLDER=DiscordPTB"
if /i "%~1"=="canary" set "DISCORD_FOLDER=DiscordCanary"
set "DISCORD_BASE=%LOCALAPPDATA%\%DISCORD_FOLDER%"
set "LATEST_DISCORD_APP="
if not exist "%DISCORD_BASE%\" exit /b 1
for /f "delims=" %%D in ('dir /b /ad /o-n "%DISCORD_BASE%\app-*" 2^>nul') do if not defined LATEST_DISCORD_APP set "LATEST_DISCORD_APP=%%D"
if not defined LATEST_DISCORD_APP exit /b 1
if exist "%DISCORD_BASE%\%LATEST_DISCORD_APP%\resources\_app.asar" exit /b 0
exit /b 1

:enable_auto
set "AUTO_BRANCH=%~1"
if not exist "%AUTO_DIR%\" mkdir "%AUTO_DIR%" || exit /b 1
copy /y "%~f0" "%AUTO_SCRIPT%" >nul || exit /b 1
set "SOUNDGIF_AUTO_SCRIPT=%AUTO_SCRIPT%"
set "SOUNDGIF_AUTO_BRANCH=%AUTO_BRANCH%"
powershell.exe -NoLogo -NoProfile -Command "$q=[char]34; $user=$env:USERDOMAIN+'\'+$env:USERNAME; $arg='/d /c '+$q+$q+$env:SOUNDGIF_AUTO_SCRIPT+$q+' --auto-run '+$env:SOUNDGIF_AUTO_BRANCH+$q; $action=New-ScheduledTaskAction -Execute $env:COMSPEC -Argument $arg; $triggers=@((New-ScheduledTaskTrigger -AtLogOn -User $user),(New-ScheduledTaskTrigger -Once -At (Get-Date).AddMinutes(15) -RepetitionInterval (New-TimeSpan -Minutes 15) -RepetitionDuration (New-TimeSpan -Days 3650))); Register-ScheduledTask -TaskName 'SoundGIF Vencord Auto Repair' -Action $action -Trigger $triggers -Description 'Updates SoundGIF and repairs the Vencord patch when Discord replaces it.' -Force | Out-Null"
if errorlevel 1 (
    echo Failed to register the automatic repair task.
    exit /b 1
)
echo.
echo Automatic updates and repatching are enabled for Discord %AUTO_BRANCH%.
echo The check runs at sign-in and every 15 minutes.
exit /b 0

:disable_auto
powershell.exe -NoLogo -NoProfile -Command "Unregister-ScheduledTask -TaskName 'SoundGIF Vencord Auto Repair' -Confirm:$false -ErrorAction SilentlyContinue"
if exist "%AUTO_SCRIPT%" del /q "%AUTO_SCRIPT%" >nul 2>&1
echo.
echo Automatic updates and repatching are disabled.
exit /b 0

:finish
set "FINAL_EXIT=%ERRORLEVEL%"
if not "%FINAL_EXIT%"=="0" (
    echo.
    echo The operation failed.
) else (
    echo.
    echo Done. Restart Discord and enable SoundGIF in Vencord's plugin settings.
)
echo.
pause
exit /b %FINAL_EXIT%
