# z42 installer — Windows.
#
#   irm https://z42-lang.github.io/z42/install-z42.ps1 | iex
#   $env:Z42_VERSION = "0.6.0"; irm https://z42-lang.github.io/z42/install-z42.ps1 | iex
#   .\scripts\install-z42.ps1                     # in a z42 checkout (cmd: scripts\install-z42.bat)
#
# One file, two default sets, chosen by where the script itself lives (never by the current directory):
#   repository — run as a file inside a checkout (<repo>\scripts\install-z42.ps1, next to xtask.z42.toml,
#                with <repo>\src\compiler\): version pinned in scripts\versions.toml ([toolchain.z42].launcher),
#                installs into <repo>\.z42, never touches PATH.
#   user       — anything else (downloaded copy, `irm | iex`): $env:Z42_VERSION or nightly,
#                $env:Z42_HOME or %USERPROFILE%\.z42, adds z42 to the user PATH.
# Explicit -Version / -Dest override either set; piped into iex there are no parameters, so use the
# environment variables. Re-run to update. macOS / Linux: see install-z42.sh.
param(
    [string]$Version = "",
    [string]$Dest = "",
    [switch]$NoModifyPath,
    [string]$Archive = "",
    [switch]$Force,
    [switch]$DryRun
)

# Piped into iex the script runs in the caller's session: `exit` would close it and preferences set at
# script level would stick, so everything lives in this function and errors are thrown.
function Install-Z42([string]$ScriptFile, [string]$Version, [bool]$HasVersion, [string]$Dest, [bool]$HasDest,
                     [bool]$NoModifyPath, [string]$Archive, [bool]$Force, [bool]$DryRun) {
    $ErrorActionPreference = "Stop"
    $ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest's progress bar is extremely slow
    $RepoSlug = "z42-lang/z42"
    $DocsUrl = "https://z42-lang.github.io/z42/learn/"
    function Say([string]$msg) { Write-Host "z42-install: $msg" }

    # ── defaults: repository or user ───────────────────────────────────────────
    $InstallMode = "user"
    $RepoRoot = ""
    if ($ScriptFile -and (Split-Path -Leaf $ScriptFile) -eq "install-z42.ps1") {
        $ScriptDir = Split-Path -Parent $ScriptFile
        $Candidate = Split-Path -Parent $ScriptDir
        if ((Split-Path -Leaf $ScriptDir) -eq "scripts" -and
            (Test-Path -LiteralPath (Join-Path $ScriptDir "xtask.z42.toml") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $Candidate "src/compiler") -PathType Container)) {
            $InstallMode = "repository"
            $RepoRoot = $Candidate
        }
    }
    if ($InstallMode -eq "repository") {
        $Pins = Join-Path $RepoRoot "scripts/versions.toml"
        if (-not (Test-Path -LiteralPath $Pins -PathType Leaf)) { throw "repository pin file is missing: $Pins" }
        # Reads `launcher = "<v>"` under [toolchain.z42]; `xtask package sdk --verify` checks this
        # reading against the real TOML parser on the checked-in file.
        $PinnedVersion = ""
        $InToolchain = $false
        foreach ($Line in Get-Content -LiteralPath $Pins) {
            if ($Line -match '^\s*\[toolchain\.z42\]\s*(#.*)?$') { $InToolchain = $true; continue }
            if ($Line -match '^\s*\[') { $InToolchain = $false }
            if ($InToolchain -and $Line -match '^\s*launcher\s*=\s*"([^"]+)"\s*(#.*)?$') {
                $PinnedVersion = $Matches[1]; break
            }
        }
        if (-not $PinnedVersion) { throw "repository versions.toml has no valid toolchain.z42.launcher" }
        if (-not $HasVersion) { $Version = $PinnedVersion }
        if (-not $HasDest) { $Dest = Join-Path $RepoRoot ".z42" }
        $NoModifyPath = $true
    } else {
        if (-not $HasVersion) {
            $Version = if ($env:Z42_VERSION) { $env:Z42_VERSION } else { "nightly" }
        }
        if (-not $HasDest) {
            $Dest = if ($env:Z42_HOME) { $env:Z42_HOME }
                    elseif ($env:USERPROFILE) { Join-Path $env:USERPROFILE ".z42" }
                    else { "" }
        }
    }
    if (-not $Version) { throw "version must not be empty" }
    if (-not $Dest) { throw "cannot determine install directory (USERPROFILE unset); pass -Dest" }
    if ($InstallMode -eq "repository") { Say "mode: repository ($RepoRoot)" } else { Say "mode: user" }
    Say "version: $Version"
    Say "install: $Dest"
    Say "modify PATH: $(if ($NoModifyPath) { 'no' } else { 'yes' })"

    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($arch -ne "AMD64") { throw "unsupported architecture $arch (supported: Windows x64)" }
    $Rid = "windows-x64"

    $Tag = if ($Version -eq "nightly") { "nightly" } else { "v$Version" }
    $Asset = "z42-sdk-$Version-$Rid.zip"
    $BaseUrl = "https://github.com/$RepoSlug/releases/download/$Tag"

    if ($DryRun) {
        Say "dry run - nothing will be changed"
        Say "  platform: $Rid"
        if ($Archive) { Say "  archive: $Archive" } else { Say "  download: $BaseUrl/$Asset" }
        return
    }

    $InstallToml = Join-Path $Dest "install.toml"
    $Tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("z42-install-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $Tmp | Out-Null
    try {
        # ── fetch + verify ─────────────────────────────────────────────────────
        $Sha = ""
        if ($Archive) {
            if (-not (Test-Path -LiteralPath $Archive -PathType Leaf)) { throw "archive not found: $Archive" }
            $Pkg = $Archive
            Say "installing from $Archive"
        } else {
            $Sums = Join-Path $Tmp "SHA256SUMS"
            Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/SHA256SUMS" -OutFile $Sums
            foreach ($line in Get-Content -LiteralPath $Sums) {
                $parts = $line -split '\s+', 2
                if ($parts.Length -eq 2 -and $parts[1].TrimStart('*') -eq $Asset) { $Sha = $parts[0].ToLower() }
            }
            if (-not $Sha) { throw "no checksum for $Asset in SHA256SUMS" }
            if (-not $Force -and (Test-Path -LiteralPath $InstallToml) -and
                    (Test-Path -LiteralPath (Join-Path $Dest "z42.exe")) -and
                    ((Get-Content -LiteralPath $InstallToml -Raw) -match "sha256 = `"$Sha`"")) {
                Say "z42 $Version is already up to date in $Dest"
                return
            }
            $Pkg = Join-Path $Tmp $Asset
            Say "downloading z42 $Version for $Rid"
            Invoke-WebRequest -UseBasicParsing -Uri "$BaseUrl/$Asset" -OutFile $Pkg
            $got = (Get-FileHash -Algorithm SHA256 -LiteralPath $Pkg).Hash.ToLower()
            if ($got -ne $Sha) { throw "checksum mismatch for $Asset" }
        }

        # ── install ────────────────────────────────────────────────────────────
        # Replace exactly the top-level entries the archive carries; everything else in Dest (installed
        # workloads under runtimes\, cache\, install.toml) is kept. The SDK never ships those state
        # entries — `xtask package sdk --verify` checks it.
        New-Item -ItemType Directory -Force -Path $Dest | Out-Null
        $Stage = Join-Path $Dest ".install-staging"
        if (Test-Path -LiteralPath $Stage) { Remove-Item -LiteralPath $Stage -Recurse -Force }
        Expand-Archive -LiteralPath $Pkg -DestinationPath $Stage
        if (-not (Test-Path -LiteralPath (Join-Path $Stage "z42.exe"))) { throw "archive does not look like a z42 SDK (no z42.exe)" }
        foreach ($item in @(Get-ChildItem -LiteralPath $Stage)) {
            $dst = Join-Path $Dest $item.Name
            if (Test-Path -LiteralPath $dst) {
                # A running z42.exe cannot be deleted but can be renamed.
                try { Remove-Item -LiteralPath $dst -Recurse -Force }
                catch { Move-Item -LiteralPath $dst -Destination "$dst.old-$([guid]::NewGuid())" -Force }
            }
            Move-Item -LiteralPath $item.FullName -Destination $dst
        }
        Remove-Item -LiteralPath $Stage -Recurse -Force
        Set-Content -LiteralPath $InstallToml -Value "version = `"$Version`"`nrid = `"$Rid`"`nsha256 = `"$Sha`""

        $installed = ""
        try { $installed = (& (Join-Path $Dest "z42.exe") --version 2>$null) } catch { }
        if ($LASTEXITCODE -ne 0) { $installed = "" }
        if (-not $installed) { $installed = "z42" }
        Say "installed $installed to $Dest"

        # ── PATH (user scope) ──────────────────────────────────────────────────
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
        Remove-Item -LiteralPath $Tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

$Z42InstallScript = $MyInvocation.MyCommand.Path
try {
    Install-Z42 -ScriptFile $Z42InstallScript -Version $Version -HasVersion ($PSBoundParameters -and $PSBoundParameters.ContainsKey("Version")) `
        -Dest $Dest -HasDest ($PSBoundParameters -and $PSBoundParameters.ContainsKey("Dest")) `
        -NoModifyPath $NoModifyPath.IsPresent -Archive $Archive -Force $Force.IsPresent -DryRun $DryRun.IsPresent
} catch {
    # Run as a file: report and set the exit code. Piped into iex: rethrow so the caller's session stays open.
    if (-not $Z42InstallScript) { throw "z42-install: error: $($_.Exception.Message)" }
    Write-Host "z42-install: error: $($_.Exception.Message)" -ForegroundColor Red
    exit 1
}
