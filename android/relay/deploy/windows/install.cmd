@echo off
setlocal
cd /d "%~dp0"
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-CoucouManaged.ps1"
if errorlevel 1 (
  echo.
  echo A instalacao nao terminou. Veja o erro acima.
  pause
  exit /b 1
)
echo.
echo Coucou instalado e conectado ao VPS. Este pacote contem uma chave revogavel; apague o ZIP depois de usar.
pause

