@echo off
rem TrackCraft-Demo mit erfundenen Daten auf Mallorca. Doppelklick genuegt, keine Installation, keine Administratorrechte.
rem Daten, Zertifikat und Einstellungen liegen in %LOCALAPPDATA%\TrackCraft-Demo; zum Entfernen diesen Ordner loeschen.
rem Optionen gehen an "trackcraft.exe demo start", z. B.:
rem   demo.cmd --reset        mit frischen Demo-Daten beginnen
rem   demo.cmd --port 9443    anderer Port, falls 8443 belegt ist
setlocal
chcp 65001 >nul
title TrackCraft-Demo
cd /d "%~dp0"
"%~dp0trackcraft.exe" demo start %*
set "TC_EXIT=%errorlevel%"
if "%TC_EXIT%"=="0" exit /b 0
echo.
echo Demo mit Fehler beendet, Code %TC_EXIT%.
pause
exit /b %TC_EXIT%
