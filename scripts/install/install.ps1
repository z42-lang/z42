# z42 installer — Windows.
#
#   irm https://z42-lang.github.io/z42/install.ps1 | iex
#
# Installs the z42 SDK into %USERPROFILE%\.z42 (or $env:Z42_HOME) and adds it to the
# user PATH. Re-run to update. macOS / Linux: see install.sh.
#
# Parameters work when the script is saved and run directly:
#   .\install.ps1 -Version 0.6.0 -Dest D:\z42 -NoModifyPath
# When piped into iex, use environment variables instead:
#   $env:Z42_VERSION = "0.6.0"; irm https://z42-lang.github.io/z42/install.ps1 | iex
param(
    [string]$Version = $(if ($env:Z42_VERSION) { $env:Z42_VERSION } else { "nightly" }),
    [string]$Dest = $(if ($env:Z42_HOME) { $env:Z42_HOME } else { Join-Path $env:USERPROFILE ".z42" }),
    [switch]$NoModifyPath,
    [string]$Archive = "",
    [switch]$Force,
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest's progress bar is extremely slow

$RepoSlug = "z42-lang/z42"
$DocsUrl = "https://z42-lang.github.io/z42/learn/"

function Say([string]$msg) { Write-Host "z42-install: $msg" }
function Fail([string]$msg) { Write-Host "z42-install: error: $msg" -ForegroundColor Red; exit 1 }

$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -ne "AMD64") { Fail "unsupported architecture $arch (supported: Windows x64)" }
$Rid = "windows-x64"

$Tag = if ($Version -eq "nightly") { "nightly" } else { "v$Version" }
$Asset = "z42-sdk-$Version-$Rid.zip"
$BaseUrl = "https://github.com/$RepoSlug/releases/download/$Tag"

if ($DryRun) {
    Say "dry run - nothing will be changed"
    Say "  version: $Version ($Rid)"
    if ($Archive) { Say "  archive: $Archive" } else { Say "  download: $BaseUrl/$Asset" }
    Say "  install: $Dest"
    exit 0
}

$InstallToml = Join-Path $Dest "install.toml"
$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("z42-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    # ── fetch + verify ─────────────────────────────────────────────────────────
    $Sha = ""
    if ($Archive) {
        if (-not (Test-Path $Archive)) { Fail "archive not found: $Archive" }
        $Pkg = $Archive
        Say "installing from $Archive"
    } else {
        $Sums = Join-Path $Tmp "SHA256SUMS"
        Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/SHA256SUMS" -OutFile $Sums
        foreach ($line in Get-Content $Sums) {
            $parts = $line -split '\s+', 2
            if ($parts.Length -eq 2 -and $parts[1].TrimStart('*') -eq $Asset) { $Sha = $parts[0].ToLower() }
        }
        if (-not $Sha) { Fail "no checksum for $Asset in SHA256SUMS" }
        if (-not $Force -and (Test-Path $InstallToml) -and (Test-Path (Join-Path $Dest "z42.exe")) `
                -and ((Get-Content $InstallToml -Raw) -match "sha256 = `"$Sha`"")) {
            Say "z42 $Version is already up to date in $Dest"
            exit 0
        }
        $Pkg = Join-Path $Tmp $Asset
        Say "downloading z42 $Version for $Rid"
        Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$Asset" -OutFile $Pkg
        $got = (Get-FileHash -Algorithm SHA256 $Pkg).Hash.ToLower()
        if ($got -ne $Sha) { Fail "checksum mismatch for $Asset" }
    }

    # ── install: replace only the SDK's own top-level entries ──────────────────
    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    $Stage = Join-Path $Dest ".install-staging"
    if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
    Expand-Archive -Path $Pkg -DestinationPath $Stage
    if (-not (Test-Path (Join-Path $Stage "z42.exe"))) { Fail "archive does not look like a z42 SDK (no z42.exe)" }
    foreach ($entry in @("z42.exe", "bin", "programs", "libs", "native", "manifest.toml")) {
        $src = Join-Path $Stage $entry
        if (Test-Path $src) {
            $dst = Join-Path $Dest $entry
            if (Test-Path $dst) {
                # A running z42.exe cannot be deleted but can be renamed.
                try { Remove-Item -Recurse -Force $dst }
                catch { Move-Item -Force $dst "$dst.old-$([guid]::NewGuid())" }
            }
            Move-Item $src $dst
        }
    }
    Remove-Item -Recurse -Force $Stage
    Set-Content -Path $InstallToml -Value "version = `"$Version`"`nrid = `"$Rid`"`nsha256 = `"$Sha`""

    $installed = ""
    try { $installed = (& (Join-Path $Dest "z42.exe") --version 2>$null) } catch { }
    if ($LASTEXITCODE -ne 0) { $installed = "" }
    if (-not $installed) { $installed = "z42" }
    Say "installed $installed to $Dest"

    # ── PATH (user scope) ──────────────────────────────────────────────────────
    $bin = Join-Path $Dest "bin"
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if ($entries -contains $Dest) {
        # already on PATH
    } elseif (-not $NoModifyPath) {
        [Environment]::SetEnvironmentVariable("Path", (@($Dest, $bin) + $entries) -join ';', "User")
        Say "added $Dest and $bin to your user PATH; open a new terminal, then:"
    } else {
        Say "add $Dest and $bin to PATH, then:"
    }
    Write-Host ""
    Write-Host "    z42 --version"
    Write-Host "    z42 new hello; cd hello; z42 run"
    Write-Host ""
    Write-Host "  learn more: $DocsUrl"
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
