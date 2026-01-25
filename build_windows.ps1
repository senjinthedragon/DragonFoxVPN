# Build script for DragonFoxVPN Tray on Windows
# Prerequisites: pip install PyQt5 requests beautifulsoup4 pyinstaller

$ErrorActionPreference = "Stop"

Write-Host "Building DragonFoxVPN Tray..."
pyinstaller --clean --noconsole --onefile --uac-admin --icon="app.ico" --version-file="version_info.txt" --name="DragonFoxVPN Tray" dragonfox_vpn.py

if ($LASTEXITCODE -eq 0) {
    Write-Host "Build successful! Executable is in dist\DragonFoxVPN Tray.exe"
} else {
    Write-Host "Build failed!"
}
