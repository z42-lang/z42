@echo off
REM install-z42.bat — repository bootstrap (Windows).
REM
REM Installs the z42 toolchain version pinned in versions.toml ([toolchain.z42].launcher)
REM into <repo>\.z42 without touching PATH. Re-run to update.
REM
REM   scripts\install-z42.bat                  pinned version -> <repo>\.z42
REM   scripts\install-z42.bat -Version 0.6.0   override the version
REM   scripts\install-z42.bat -Force           reinstall even if up to date
REM
REM The install logic lives in scripts\install\install.ps1 (the user-facing installer);
REM this script only supplies repo defaults. Other install.ps1 parameters pass through.
setlocal
set "REPO=%~dp0.."
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ver = 'nightly'; $in = $false;" ^
  "foreach ($l in Get-Content (Join-Path '%REPO%' 'versions.toml')) {" ^
  "  if ($l -match '^\[toolchain\.z42\]') { $in = $true; continue }" ^
  "  if ($l -match '^\[') { $in = $false }" ^
  "  if ($in -and $l -match '^launcher\s*=\s*\"([^\"]+)\"') { $ver = $Matches[1]; break }" ^
  "}" ^
  "& (Join-Path '%REPO%' 'scripts\install\install.ps1') -Dest (Join-Path '%REPO%' '.z42') -Version $ver -NoModifyPath %*;" ^
  "exit $LASTEXITCODE"
exit /b %ERRORLEVEL%
