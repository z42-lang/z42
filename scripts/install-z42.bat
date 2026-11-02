@echo off
REM CMD entry; defaults and installation are owned by the PowerShell installer.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-z42.ps1" %*
exit /b %ERRORLEVEL%
