# DragonFoxVPN Tray

A professional-grade system tray utility for managing VPN connections with a modern dark UI. 

Designed to work seamlessly on **Linux (Garuda/Arch)** and **Windows 11**.

## Features

- 🌍 **Location Switching**: Easily switch between VPN nodes via a searchable dialog.
- ⚡ **Auto-Connect**: Automatically connects to the last used location on startup.
- 🔒 **Kill Switch**: Automatically disables internet access if the VPN connection drops.
- 📊 **Status Dashboard**: Real-time monitoring of connection state and duration.
- 🎨 **Modern UI**: Dark-themed, high-resolution icons and smooth transitions.
- 🐧🪟 **Cross-Platform**: Fully supports Linux (iproute2/resolvectl) and Windows (route/netsh).

## Installation

### Dependencies

Ensure you have Python 3.8+ installed.

#### Linux
```bash
sudo pacman -S python-pyqt5 python-requests python-beautifulsoup4 python-urllib3 traceroute
```

#### Windows
```powershell
pip install PyQt5 requests beautifulsoup4 urllib3
```

### Configuration

The application automatically detects your active network adapter. However, you can modify the `Config` class at the top of `dragonfox_vpn.py` to hardcode specific gateways or DNS servers if needed.

- **Linux Config**: `~/.config/dragonfox_vpn.json`
- **Windows Config**: `%APPDATA%\DragonFoxVPN\config.json`

## Usage

1. **Run the script**:
   - **Linux**: `python dragonfox_vpn.py` (May require root for routing commands, though the script uses `sudo`).
   - **Windows**: Run your terminal (PowerShell/CMD) as **Administrator**, then run `python dragonfox_vpn.py`.
2. **Tray Icon**:
   - Left-click or right-click the Dragon icon in your system tray to access the menu.
   - Double-click to open the **Status Dashboard**.
3. **Switching Locations**:
   - Select "Change Location..." from the menu. Use the search bar to find your desired node. Right-click locations to add them to your favorites.

## Troubleshooting

- **Permissions**: On Windows, the application **must** be run as Administrator to modify routing tables.
- **DNS Issues**: If DNS doesn't resolve after disconnecting, the script attempts to flush the cache. You can manually run `ipconfig /flushdns` (Windows) or `resolvectl flush-caches` (Linux).
- **Kill Switch**: If the tray icon turns red, your internet access is likely blocked by the kill switch. Reconnect or restart the app to restore access.

## License

Copyright (c) 2026 DragonFox Studios. All rights reserved.
