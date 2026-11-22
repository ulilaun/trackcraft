@echo off
rem TrackCraft installieren oder aktualisieren (RFC-0005). Doppelklick genuegt.
rem Aus dem entpackten ZIP starten, z. B. im Download-Ordner. Optionen gehen an "trackcraft.exe setup":
rem   setup.cmd --install-dir "D:\Programme\TrackCraft"
rem   setup.cmd --names pc.home.arpa,localhost --display-name Alex --backup-dir E:\TrackCraft-Sicherung --port 443
setlocal
chcp 65001 >nul
cd /d "%~dp0"

net session >nul 2>&1
if errorlevel 1 (
    echo Administratorrechte werden angefordert ...
    set "TC_SETUP=%~f0"
    set "TC_ARGS=%*"
    powershell -NoProfile -ExecutionPolicy Bypass -Command "if ($env:TC_ARGS) { Start-Process -FilePath $env:TC_SETUP -ArgumentList $env:TC_ARGS -Verb RunAs } else { Start-Process -FilePath $env:TC_SETUP -Verb RunAs }"
    exit /b
)

"%~dp0trackcraft.exe" setup --interactive %*
set "TC_EXIT=%errorlevel%"
echo.
if not "%TC_EXIT%"=="0" echo Einrichtung mit Fehler beendet (Code %TC_EXIT%).
pause
exit /b %TC_EXIT%
