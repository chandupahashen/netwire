[CmdletBinding()]
param(
    # Set this when the checkout has no GitHub origin, for example: owner/repo
    [string]$GitHubRepository,

    # Skip pnpm install when dependencies are already installed.
    [switch]$SkipInstall
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

    if (-not $SkipInstall) {
        Write-Host 'Installing locked dependencies…'
        pnpm install --frozen-lockfile
        if ($LASTEXITCODE -ne 0) {
            throw "pnpm install failed with exit code $LASTEXITCODE."
        }
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
        pnpm exec tauri build --config ".tauri/package-updater-$PID.json"
    }
    else {
        Write-Warning 'No GitHub origin found. Building a signed installer without a release endpoint; pass -GitHubRepository owner/repo to enable update checks.'
        pnpm exec tauri build
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
