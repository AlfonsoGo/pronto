@echo off
title Pronto - Instalar requisitos de compilacion

REM ===========================================================================
REM  DOBLE-CLIC en este archivo para instalar lo necesario para compilar
REM  Pronto en Windows:
REM    - Visual Studio 2022 Build Tools  [carga Desktop development with C++]
REM    - Modo de desarrollador de Windows
REM
REM  Aparecera UNA ventana de Control de cuentas de usuario [UAC]: pulsa SI.
REM  Luego la instalacion va sola. La descarga es grande: 10-30 min.
REM  No cierres la ventana hasta que ponga FIN.
REM ===========================================================================

net session >nul 2>&1
if %errorlevel% neq 0 goto elevar
goto ejecutar

:elevar
echo Solicitando permisos de administrador. Acepta el UAC que va a aparecer...
powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
exit /b

:ejecutar
echo.
echo Ejecutando instalador [Visual Studio C++ + Developer Mode]...
echo No cierres esta ventana.
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install_windows_build_tools.ps1"
echo.
echo ============================================================
echo  Si arriba pone INSTALL_RESULT=SUCCESS, ya esta listo.
echo  Cuando termine, ya puedes compilar Pronto (ver docs\SETUP.md).
echo ============================================================
echo.
pause
