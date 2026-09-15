@echo off
setlocal enabledelayedexpansion
REM install-z42.bat — z42 bootstrap / system install (Windows, 2026-06-13).
REM Downloads the prebuilt z42 launcher package (windows-x64 .zip) and installs it.
REM
REM Usage:
REM   install-z42.bat                          portable install → <repo>\.z42
REM   install-z42.bat --dest <dir>             portable install → <dir>
REM   install-z42.bat --system                 managed install → %Z42_HOME% or %USERPROFILE%\.z42
REM   install-z42.bat --dest <dir> --system    managed install → <dir>
REM   install-z42.bat --help                   show this message
REM
REM Mirrors scripts/install-z42.sh. Thin by design: bootstrap only; re-run it to update.
REM Version from versions.toml [toolchain.z42].launcher (default nightly).
REM Download strategy: release-index.json manifest-first; SHA256SUMS fallback.

set "REPO=%~dp0.."
set "SYSTEM_INSTALL=0"
set "USER_DEST="

:parse_args
if "%~1"=="" goto args_done
if /i "%~1"=="--system" ( set "SYSTEM_INSTALL=1" & shift & goto parse_args )
if /i "%~1"=="--dest"   ( set "USER_DEST=%~2" & shift & shift & goto parse_args )
if /i "%~1"=="--help"   ( goto show_help )
echo install-z42: unknown flag: %~1  (try --help) 1>&2
exit /b 1

:show_help
echo Usage: install-z42.bat [--dest ^<dir^>] [--system] [--help]
echo.
echo   --dest ^<dir^>   install to ^<dir^> (default: .z42 portable / %%Z42_HOME%% system)
echo   --system        managed install: bin\launcher\runtimes layout + PATH hint
echo   --help          show this message
exit /b 0

:args_done
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference='Stop';" ^
  "$repo=(Resolve-Path '%REPO%').Path;" ^
  "$systemInstall=[int]'%SYSTEM_INSTALL%';" ^
  "$userDest='%USER_DEST%';" ^
  "$slug='codesigner-ui/z42';" ^
  "if($systemInstall){$dest=if($userDest){'%USER_DEST%'}elseif($env:Z42_HOME){$env:Z42_HOME}else{Join-Path $env:USERPROFILE '.z42'}}else{$dest=if($userDest){'%USER_DEST%'}else{Join-Path $repo '.z42'}};" ^
  "$stamp=Join-Path $dest '.bootstrap-stamp';" ^
  "$ver='nightly'; $intoml=$false;" ^
  "foreach($l in Get-Content (Join-Path $repo 'versions.toml')){" ^
  "  if($l -match '^\[toolchain\.z42\]'){$intoml=$true;continue}" ^
  "  if($l -match '^\['){$intoml=$false}" ^
  "  if($intoml -and $l -match '^launcher.*\"([^\"]+)\"'){$ver=$Matches[1];break}}" ^
  "$tag=if($ver -eq 'nightly'){'nightly'}else{'v'+$ver};" ^
  "$rid='windows-x64';" ^
  "if($ver -ne 'nightly' -and (Test-Path $stamp) -and" ^
  "   (Get-Content $stamp -Raw) -match [regex]::Escape(\"$ver`:$rid`:\"))" ^
  "  {Write-Host \"install-z42: $ver / $rid already installed\"; exit 0}" ^
  "$manifestUrl=\"https://github.com/$slug/releases/download/$tag/release-index.json\";" ^
  "$manifest=$null; $asset=$null; $manifestSha=$null; $want=$null;" ^
  "try{$manifest=Invoke-RestMethod -Uri $manifestUrl -ErrorAction Stop}catch{$manifest=$null}" ^
  "if($manifest -and $manifest.runtimes -and $manifest.runtimes.$rid){" ^
  "  $ridInfo=$manifest.runtimes.$rid;" ^
  "  $asset=$ridInfo.archive; $manifestSha=$ridInfo.sha256;" ^
  "  $published=$manifest.published;" ^
  "  $want=\"$ver`:$rid`:$published\";" ^
  "  if((Test-Path $stamp) -and $published -and" ^
  "     ((Get-Content $stamp -Raw).Trim() -eq $want))" ^
  "    {Write-Host \"install-z42: already up to date ($ver / $rid)\"; exit 0}" ^
  "  Write-Host \"install-z42: fetching $asset ($tag) -> $dest  [manifest]\"" ^
  "}else{" ^
  "  $asset=\"z42-sdk-$ver-$rid.zip\"; $manifestSha=$null;" ^
  "  $id=if($ver -eq 'nightly'){try{(Invoke-RestMethod \"https://api.github.com/repos/$slug/releases/tags/nightly\").published_at}catch{''}}else{$tag};" ^
  "  $want=\"$ver`:$rid`:$id\";" ^
  "  if((Test-Path $stamp) -and $id -and ((Get-Content $stamp -Raw).Trim() -eq $want))" ^
  "    {Write-Host \"install-z42: already up to date ($ver / $rid)\"; exit 0}" ^
  "  Write-Host \"install-z42: fetching $asset ($tag) -> $dest  [SHA256SUMS fallback]\"}" ^
  "$url=\"https://github.com/$slug/releases/download/$tag/$asset\";" ^
  "$tmp=Join-Path $env:TEMP ('z42boot-'+[guid]::NewGuid());" ^
  "New-Item -ItemType Directory -Path $tmp | Out-Null;" ^
  "$zip=Join-Path $tmp $asset;" ^
  "Invoke-WebRequest -Uri $url -OutFile $zip;" ^
  "if($manifestSha){" ^
  "  $got=(Get-FileHash $zip -Algorithm SHA256).Hash.ToLower();" ^
  "  if($manifestSha.ToLower() -ne $got){Write-Error 'install-z42: SHA256 mismatch'; exit 1}" ^
  "  Write-Host 'install-z42: SHA256 ok'" ^
  "}else{" ^
  "  try{$sums=(Invoke-WebRequest -Uri \"https://github.com/$slug/releases/download/$tag/SHA256SUMS\").Content;" ^
  "    $wh=($sums -split \"`n\"|Where-Object{$_ -match [regex]::Escape($asset)+'$'}|Select-Object -First 1).Split(' ')[0];" ^
  "    if($wh){$got=(Get-FileHash $zip -Algorithm SHA256).Hash.ToLower();" ^
  "      if($wh.ToLower() -ne $got){Write-Error 'install-z42: SHA256 mismatch'; exit 1}; Write-Host 'install-z42: SHA256 ok'}}catch{}}" ^
  "$inner=Join-Path $tmp 'pkg'; New-Item -ItemType Directory -Path $inner | Out-Null;" ^
  "Expand-Archive -Path $zip -DestinationPath $inner -Force;" ^
  "if($systemInstall){" ^
  "  # Managed install: bin\launcher\runtimes layout" ^
  "  $vmName=if(Test-Path (Join-Path $inner 'bin\z42vm.exe')){'z42vm.exe'}else{'z42vm'};" ^
  "  $tramp=if($vmName -eq 'z42vm.exe'){'z42.exe'}else{'z42'};" ^
  "  New-Item -ItemType Directory -Path (Join-Path $dest 'bin') -Force | Out-Null;" ^
  "  New-Item -ItemType Directory -Path (Join-Path $dest 'launcher') -Force | Out-Null;" ^
  "  Copy-Item (Join-Path $inner $tramp) (Join-Path $dest \"bin\\$tramp\") -Force;" ^
  "  $z42cSrc=Join-Path $inner 'bin\z42c.exe';" ^
  "  if(Test-Path $z42cSrc){Copy-Item $z42cSrc (Join-Path $dest 'bin\z42c.exe') -Force};" ^
  "  Copy-Item (Join-Path $inner \"bin\\$vmName\") (Join-Path $dest \"launcher\\$vmName\") -Force;" ^
  "  Copy-Item (Join-Path $inner 'launcher.zpkg') (Join-Path $dest 'launcher\launcher.zpkg') -Force;" ^
  "  $libsDest=Join-Path $dest 'launcher\libs';" ^
  "  if(Test-Path $libsDest){Remove-Item $libsDest -Recurse -Force};" ^
  "  Copy-Item (Join-Path $inner 'libs') $libsDest -Recurse -Force;" ^
  "  $apphostSrc=Join-Path $inner 'bin\apphost.exe';" ^
  "  if(Test-Path $apphostSrc){Copy-Item $apphostSrc (Join-Path $dest 'launcher\apphost.exe') -Force};" ^
  "  $trampBin=Join-Path $dest \"bin\\$tramp\";" ^
  "  $env:Z42_HOME=$dest; & $trampBin link (Join-Path $dest 'launcher') --as $ver | Out-Null;" ^
  "  $env:Z42_HOME=$dest; & $trampBin default $ver | Out-Null;" ^
  "  Set-Content -Path $stamp -Value $want -NoNewline;" ^
  "  Write-Host \"install-z42: installed $ver / $rid -> $dest (managed)\";" ^
  "  $inPath=$env:PATH -split ';' | Where-Object{$_ -eq (Join-Path $dest 'bin')};" ^
  "  if($inPath){Write-Host \"  PATH already contains $dest\bin\"}else{Write-Host \"  Add to PATH: $dest\bin\"}" ^
  "}else{" ^
  "  if(Test-Path $dest){Remove-Item $dest -Recurse -Force}; New-Item -ItemType Directory -Path $dest | Out-Null;" ^
  "  Copy-Item (Join-Path $inner '*') $dest -Recurse -Force;" ^
  "  Set-Content -Path $stamp -Value $want -NoNewline;" ^
  "  $entry=if(Test-Path (Join-Path $dest 'z42.exe')){Join-Path $dest 'z42.exe'}else{Join-Path $dest 'bin\z42.exe'};" ^
  "  Write-Host \"install-z42: installed $ver / $rid -> $dest\";" ^
  "  Write-Host \"  entry: $entry\"}" ^
  "Remove-Item $tmp -Recurse -Force"
endlocal
