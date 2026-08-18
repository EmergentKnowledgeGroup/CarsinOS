@echo off
setlocal

set "SCRIPT_DIR=%~dp0"
set "MODE=-Tauri"

echo Launch mode:
echo   1^) Desktop app ^(Tauri^) [default]
echo   2^) Browser ^(web^)
set /p "CHOICE=Choose 1 or 2 [Enter=1]: "
if "%CHOICE%"=="2" set "MODE=-Web"

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%one_click_launch.ps1" %MODE%
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo.
  echo Launch failed. Check logs under runtime\oneclick-state\logs.
  pause
)
exit /b %EXIT_CODE%
