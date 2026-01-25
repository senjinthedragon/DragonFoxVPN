# DragonFoxVPN Tray

![DragonFoxVPN](app.ico) <!-- You might want to upload an actual image to the repo and link it here -->

A professional-grade system tray utility for managing VPN connections with a modern dark UI. 

Designed to work seamlessly on **Windows 10/11** and **Linux (Garuda/Arch/Debian)**.

## ✨ Features

- **🌍 Location Switching**: 
    - Searchable dialog with **flags** 🇺🇸 🇬🇧 🇫🇷 🇩🇪
    - Grouped by continent (Europe, Asia, Americas, etc.)
    - Favorites system ⭐
- **⚡ Smart Automation**:
    - **Auto-Connect**: Connects to the last used location on app launch.
    - **Auto-Start**: (Windows) Option to launch automatically on system login.
- **🔒 Security & Safety**:
    - **Kill Switch**: Blocks internet access if the VPN connection drops unexpectedly.
    - **DNS Leak Protection**: Automatically flushes DNS caches and enforces VPN DNS.
- **📊 Real-time Monitoring**:
    - Dashboard showing connection status, gateway IP, and session duration.
    - Tray icon changes color based on status (🟢 Connected, 🟡 Disabled, 🔴 Dropped).

## 🛠️ Installation & Building

### Prerequisites

- **Python 3.10+**
- **pip** package manager

### 📦 Dependencies

Install the required Python packages:

```bash
pip install PyQt5 requests beautifulsoup4 pyinstaller pycountry
```

*Note: On Linux, it is recommended to install `python-pyqt5` via your package manager (e.g., `pacman` or `apt`) for better system integration.*

### 🏗️ Building for Windows

This project includes a fully automated build script for Windows that handles:
1.  **Versioning**: Auto-increments the build number (e.g., `1.0.1.30` -> `1.0.1.31`).
2.  **Metadata**: Embeds version info, company name, and copyright into the executable.
3.  **Compilation**: Compiles to a standalone single-file executable using PyInstaller.

To build the executable:

1.  Open PowerShell in the project directory.
2.  Run the build script:
    ```powershell
    .\build_windows.ps1
    ```
3.  The output executable will be placed in the `dist\DragonFoxVPN Tray.exe` folder.

**Note**: The build process requires the `app.ico` file and `version_info.txt` to be present in the root directory.

## 🚀 Usage

### Windows
- Run `DragonFoxVPN Tray.exe` as **Administrator**.
- *Why Admin?* The app needs permissions to modify the system routing table (`route add/delete`) and network interface settings (`netsh`).

### Linux
- Run with Python:
    ```bash
    sudo python dragonfox_vpn.py
    ```
- *Why Sudo?* Similar to Windows, `ip` and `resolvectl` commands require root privileges to redirect network traffic.

## ⚙️ Configuration

The application stores its configuration (last location, favorites, auto-connect settings) in:

- **Windows**: `%APPDATA%\DragonFoxVPN\dragonfox_vpn.json`
- **Linux**: `~/.config/dragonfox_vpn.json`

Flag icons are cached locally in a `flags` subdirectory to reduce bandwidth.

## 🤝 Contributing

### Versioning Strategy
- **Major.Minor.Patch.Build** (e.g., `1.0.1.35`)
- The **Build** number (4th digit) is automatically incremented by the `increment_version.py` script on every Windows build.
- **Major/Minor/Patch** are manually controlled in `version_info.txt` if needed.

## 📜 License

Copyright (c) 2026 DragonFox Studios. All rights reserved.
