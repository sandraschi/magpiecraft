@echo off
REM Double-click launcher for MagpieCraft. Builds on first run, then opens the app.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start.ps1" %*
if errorlevel 1 pause
