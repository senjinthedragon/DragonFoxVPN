<#
.SYNOPSIS
    Automated Build Script for DragonFoxVPN Tray
.DESCRIPTION
    This script handles the complete build lifecycle:
    1. Increments the application version number via Python script.
    2. Invokes PyInstaller to compile a standalone executable.
    3. Excludes unnecessary standard libraries to optimize size.
    4. Embeds version information and icon resources.
.NOTES
    Author: DragonFox Studios
    Date: 2026-01-25
#>
# Prerequisites: pip install PyQt5 requests beautifulsoup4 pyinstaller pycountry

$ErrorActionPreference = "Stop"

<#
    Step 1: Version Increment
    Calls the external Python script to fetch, parse, and increment the build number
    in both version_info.txt and dragonfox_vpn.py.
#>
Write-Host "Updating version..."
python increment_version.py

<#
    Step 2: PyInstaller Build
    --clean: Nuke cache
    --noconsole: Hide terminal window
    --onefile: Bundle everything into one .exe
    --uac-admin: Manifest requiring Admin privileges
    --exclude-module: Strip unused modules to save space (~some MBs)
#>
Write-Host "Building DragonFoxVPN Tray..."
pyinstaller --clean --noconsole --onefile --uac-admin --icon="app.ico" --version-file="version_info.txt" --name="DragonFoxVPN Tray" --exclude-module tkinter --exclude-module unittest --exclude-module pydoc --exclude-module difflib --exclude-module doctest dragonfox_vpn.py

if ($LASTEXITCODE -eq 0) {
    Write-Host "Build successful! Executable is in dist\DragonFoxVPN Tray.exe"
}
else {
    Write-Host "Build failed!"
}
