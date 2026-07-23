param(
    [string]$VencordDir,
    [switch]$NoInject,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2

$SoundGifRepository = "https://github.com/Superwheat/soundGIF.git"
$VencordRepository = "https://github.com/Vendicated/Vencord.git"
$ScriptDirectory = Split-Path -Parent $MyInvocation.MyCommand.Path
$BundledPlugin = Join-Path $ScriptDirectory "soundGif"

function Require-Command {
    param([string]$Name)

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name is required and was not found in PATH."
    }
}

function Run-Command {
    param(
        [string]$File,
        [string[]]$Arguments,
        [string]$WorkingDirectory
    )

    $display = "$File " + ($Arguments -join " ")
    Write-Host "> $display"
    if ($DryRun) {
        return
    }

    Push-Location $WorkingDirectory
    try {
        & $File @Arguments
        if ($LASTEXITCODE -ne 0) {
            throw "$File exited with code $LASTEXITCODE."
        }
    } finally {
        Pop-Location
    }
}

function Try-UpdateSoundGif {
    param([string]$Checkout)

    if ($DryRun) {
        Write-Host "> git clone or update $SoundGifRepository"
        return $false
    }

    try {
        if (Test-Path (Join-Path $Checkout ".git")) {
            Run-Command "git" @("-C", $Checkout, "pull", "--ff-only") $ScriptDirectory
        } else {
            $parent = Split-Path -Parent $Checkout
            New-Item -ItemType Directory -Force -Path $parent | Out-Null
            Run-Command "git" @("clone", "--depth", "1", $SoundGifRepository, $Checkout) $parent
        }
        return $true
    } catch {
        Write-Warning "Could not update SoundGIF source: $($_.Exception.Message)"
        return $false
    }
}

Require-Command "git"
Require-Command "node"

$pnpm = Get-Command "pnpm" -ErrorAction SilentlyContinue
$corepack = Get-Command "corepack" -ErrorAction SilentlyContinue
if ($pnpm) {
    $PnpmFile = $pnpm.Source
    $PnpmPrefix = @()
} elseif ($corepack) {
    $PnpmFile = $corepack.Source
    $PnpmPrefix = @("pnpm")
} else {
    throw "pnpm is required. Install it from https://pnpm.io/installation and run this file again."
}

$localData = [Environment]::GetFolderPath("LocalApplicationData")
if (-not $localData) {
    $localData = Join-Path $HOME "AppData\Local"
}
$managedRoot = Join-Path $localData "SoundGIF"
$soundGifCheckout = Join-Path $managedRoot "source"
$managedVencord = Join-Path $managedRoot "Vencord"

if (-not $VencordDir -and $env:VENCORD_DIR) {
    $VencordDir = $env:VENCORD_DIR
}

if (-not $VencordDir) {
    $documents = [Environment]::GetFolderPath("MyDocuments")
    $candidates = @(
        (Join-Path $documents "Vencord"),
        (Join-Path $HOME "Documents\Vencord"),
        (Join-Path $HOME "Vencord"),
        $managedVencord
    )

    foreach ($candidate in $candidates) {
        if (Test-Path (Join-Path $candidate "package.json")) {
            $VencordDir = $candidate
            break
        }
    }
}

if (-not $VencordDir) {
    $VencordDir = $managedVencord
}

$VencordDir = [IO.Path]::GetFullPath($VencordDir)
Write-Host "Vencord source: $VencordDir"

if (-not (Test-Path (Join-Path $VencordDir ".git"))) {
    if ((Test-Path $VencordDir) -and (Get-ChildItem -Force $VencordDir | Select-Object -First 1)) {
        throw "$VencordDir exists but is not a Vencord Git checkout."
    }

    $vencordParent = Split-Path -Parent $VencordDir
    if (-not $DryRun) {
        New-Item -ItemType Directory -Force -Path $vencordParent | Out-Null
    }
    Run-Command "git" @("clone", $VencordRepository, $VencordDir) $vencordParent
} else {
    Run-Command "git" @("-C", $VencordDir, "pull", "--ff-only") $ScriptDirectory
}

$sourceUpdated = Try-UpdateSoundGif $soundGifCheckout
$cachedPlugin = Join-Path $soundGifCheckout "plugins\vencord\soundGif"
if ($sourceUpdated -and (Test-Path (Join-Path $cachedPlugin "index.tsx"))) {
    $pluginSource = $cachedPlugin
} elseif (Test-Path (Join-Path $BundledPlugin "index.tsx")) {
    $pluginSource = $BundledPlugin
    Write-Host "Using the SoundGIF source bundled with this installer."
} elseif ($DryRun) {
    $pluginSource = $cachedPlugin
} else {
    throw "No SoundGIF plugin source is available."
}

$userPlugins = [IO.Path]::GetFullPath((Join-Path $VencordDir "src\userplugins"))
$pluginTarget = [IO.Path]::GetFullPath((Join-Path $userPlugins "soundGif"))
if ($pluginTarget -ne [IO.Path]::GetFullPath((Join-Path $VencordDir "src\userplugins\soundGif"))) {
    throw "Refusing to replace an unexpected plugin path: $pluginTarget"
}

Write-Host "Installing plugin: $pluginTarget"
if (-not $DryRun) {
    New-Item -ItemType Directory -Force -Path $userPlugins | Out-Null
    if (Test-Path -LiteralPath $pluginTarget) {
        Remove-Item -LiteralPath $pluginTarget -Recurse -Force
    }
    Copy-Item -LiteralPath $pluginSource -Destination $pluginTarget -Recurse
}

Run-Command $PnpmFile ($PnpmPrefix + @("install", "--frozen-lockfile")) $VencordDir
Run-Command $PnpmFile ($PnpmPrefix + @("build")) $VencordDir

if (-not $NoInject) {
    Write-Host ""
    Write-Host "Vencord's installer will ask which Discord client to patch."
    Run-Command $PnpmFile ($PnpmPrefix + @("inject")) $VencordDir
}

Write-Host ""
Write-Host "Restart Discord, then enable SoundGIF in Vencord's plugin settings."
Write-Host "Run this installer again whenever SoundGIF or Vencord needs updating or repair."
