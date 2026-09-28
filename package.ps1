[CmdletBinding()]
param(
    # Set this when the checkout has no GitHub origin, for example: owner/repo
    [string]$GitHubRepository,

    # Reinstall dependencies even when node_modules already exists.
    [switch]$RefreshDependencies
)

$ErrorActionPreference = 'Stop'
$repoRoot = $PSScriptRoot
$previousLocation = Get-Location
$overlayPath = $null

function Get-GitHubRepositoryFromRemote {
    $remoteUrl = (& git -C $repoRoot remote get-url origin 2>$null | Select-Object -First 1)
    if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($remoteUrl)) {
        return $null
    }

    $match = [regex]::Match(
        $remoteUrl.Trim(),
        '^(?:https?://github\.com/|git@github\.com:|ssh://git@github\.com/)(?<repo>[^/]+/[^/]+?)(?:\.git)?/?$'
    )
    if ($match.Success) {
        return $match.Groups['repo'].Value
    }

    return $null
}

try {
    Set-Location $repoRoot

    if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
        throw 'pnpm is required. Install the version listed in package.json, then run this script again.'
    }

    $nodeModulesPath = Join-Path $repoRoot 'node_modules'
    if ($RefreshDependencies -or -not (Test-Path -LiteralPath $nodeModulesPath -PathType Container)) {
        Write-Host 'Installing locked dependencies…'
        pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) {
            throw "pnpm install failed with exit code $LASTEXITCODE. Close NetWire, Vite, and other Node processes that may be using node_modules, then retry."
        }
    }
    else {
        Write-Host 'Reusing installed dependencies. Pass -RefreshDependencies to reinstall them.'
    }

    $tscBin = Join-Path $nodeModulesPath '.bin\tsc.cmd'
    $viteBin = Join-Path $nodeModulesPath '.bin\vite.cmd'
    if (-not (Test-Path -LiteralPath $tscBin -PathType Leaf) -or
        -not (Test-Path -LiteralPath $viteBin -PathType Leaf)) {
        throw 'Frontend build tools are missing from node_modules. Close NetWire, Vite, and other Node processes, then run .\package.ps1 -RefreshDependencies.'
    }

    if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY) -and
        [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PATH)) {
        $localKey = Join-Path $repoRoot '.tauri\netwire-updater.key'
        if (Test-Path -LiteralPath $localKey -PathType Leaf) {
            $env:TAURI_SIGNING_PRIVATE_KEY_PATH = $localKey
        }
        else {
            throw @'
The Tauri config builds signed updater artifacts, but no signing key was found.
Set TAURI_SIGNING_PRIVATE_KEY or TAURI_SIGNING_PRIVATE_KEY_PATH, or place the
maintainer key at .tauri/netwire-updater.key. Never commit the private key.
'@
        }
    }

    if ([string]::IsNullOrWhiteSpace($GitHubRepository)) {
        $GitHubRepository = Get-GitHubRepositoryFromRemote
    }

    $tauriBin = Join-Path $nodeModulesPath '.bin\tauri.cmd'
    if (Test-Path -LiteralPath $tauriBin -PathType Leaf) {
        $tauriRunner = 'pnpm'
        $tauriCommand = @('exec', 'tauri')
    }
    else {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            throw 'The local Tauri CLI is missing and Cargo is unavailable. Close NetWire and run pnpm install --frozen-lockfile --prod=false, then retry.'
        }
        $tauriRunner = 'cargo'
        $tauriCommand = @('tauri')
        Write-Host 'Using the installed Rust Tauri CLI because the local JavaScript CLI is unavailable.'
    }

    if ($GitHubRepository) {
        if ($GitHubRepository -notmatch '^[^/\s]+/[^/\s]+$') {
            throw 'GitHubRepository must use the owner/repository format.'
        }

        $configDirectory = Join-Path $repoRoot '.tauri'
        New-Item -ItemType Directory -Path $configDirectory -Force | Out-Null
        $overlayPath = Join-Path $configDirectory "package-updater-$PID.json"
        $releaseConfig = @{
            plugins = @{
                updater = @{
                    endpoints = @("https://github.com/$GitHubRepository/releases/latest/download/latest.json")
                }
            }
        } | ConvertTo-Json -Depth 5
        Set-Content -LiteralPath $overlayPath -Value $releaseConfig -Encoding utf8

        Write-Host "Building signed installers with updates from GitHub repository $GitHubRepository…"
        $tauriArgs = @($tauriCommand + @('build', '--config', ".tauri/package-updater-$PID.json"))
        & $tauriRunner @tauriArgs
    }
    else {
        Write-Warning 'No GitHub origin found. Building a signed installer without a release endpoint; pass -GitHubRepository owner/repo to enable update checks.'
        $tauriArgs = @($tauriCommand + @('build'))
        & $tauriRunner @tauriArgs
    }

    if ($LASTEXITCODE -ne 0) {
        throw "Tauri packaging failed with exit code $LASTEXITCODE."
    }

    $bundleDirectory = Join-Path $repoRoot 'src-tauri\target\release\bundle'
    Write-Host "Package build completed. Installer files are under: $bundleDirectory"
}
finally {
    if ($overlayPath -and (Test-Path -LiteralPath $overlayPath)) {
        Remove-Item -LiteralPath $overlayPath -Force
    }
    Set-Location $previousLocation
}
