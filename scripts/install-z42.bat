@echo off
REM install-z42.bat - cmd / Explorer entry: runs install-z42.ps1 next to it with the same arguments
REM and returns its exit code. Defaults and the install logic live in the PowerShell script.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-z42.ps1" %*
exit /b %ERRORLEVEL%
