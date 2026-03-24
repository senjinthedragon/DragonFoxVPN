<#
build_windows.ps1 - DragonFoxVPN: Windows build script
Copyright (c) 2026 Senjin the Dragon.
https://github.com/senjinthedragon/DragonFoxVPN
Licensed under the MIT License.
See LICENSE for full license information.

Automates the full Windows build lifecycle: increments the build number
via increment_version.py, then runs PyInstaller to produce a single
standalone executable with version metadata and icon embedded.
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
