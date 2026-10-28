@echo off
setlocal
cd /d "%~dp0"

for /f "usebackq delims=" %%v in (`powershell -NoProfile -Command "((Get-Content Cargo.toml) -match '^version')[0].Split([char]34)[1]"`) do set "VERSION=%%v"
if not defined VERSION goto :noversion

echo.
echo Coverly %VERSION%
echo.
echo [1/3] Running tests...
cargo test --release
if errorlevel 1 goto :failed

echo.
echo [2/3] Building Coverly (release)...
cargo build --release
if errorlevel 1 goto :failed

echo.
echo [3/3] Building the installer...
set "NSIS="
where makensis >nul 2>nul && set "NSIS=makensis"
if not defined NSIS if exist "C:\Program Files (x86)\NSIS\makensis.exe" set "NSIS=C:\Program Files (x86)\NSIS\makensis.exe"
if not defined NSIS if exist "C:\Program Files\NSIS\makensis.exe" set "NSIS=C:\Program Files\NSIS\makensis.exe"
if not defined NSIS goto :nonsis

if not exist dist mkdir dist
"%NSIS%" /V2 /DVERSION=%VERSION% installer\installer.nsi
if errorlevel 1 goto :failed

echo.
echo Done: %~dp0dist\Coverly-Setup-%VERSION%.exe
explorer "%~dp0dist"
exit /b 0

:noversion
echo Couldn't read the version from Cargo.toml.
pause
exit /b 1

:nonsis
echo.
echo NSIS was not found. Install it from https://nsis.sourceforge.io/Download
echo The program itself is ready at: %~dp0target\release\coverly.exe
pause
exit /b 1

:failed
echo.
echo Build failed. Copy the error above and send it.
pause
exit /b 1
