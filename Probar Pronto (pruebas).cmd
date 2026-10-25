@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\probar-pronto.ps1" %*
pause
