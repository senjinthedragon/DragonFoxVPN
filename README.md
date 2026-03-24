# DragonFoxVPN Tray

A professional-grade system tray utility for managing VPN connections with a modern dark UI.

Designed to work on **Windows 10/11** and **Linux (Garuda/Arch/Debian)**. Built around a
**Raspberry Pi gateway** architecture where a Pi running OpenVPN sits between your LAN and the
internet, with this tray app managing routing on each client machine.

> **VPN provider**: The included backend is written for **ExpressVPN** `.ovpn` configs, but the
> shell script and web UI are straightforward to adapt to any OpenVPN-compatible provider.

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
    - **Drop Debouncing**: Requires two consecutive failed checks before triggering the kill switch, avoiding false positives.
    - **DNS Leak Protection**: Automatically flushes DNS caches and enforces VPN DNS.
- **📊 Real-time Monitoring**:
    - Dashboard showing connection status, gateway IP, and session duration.
    - Tray icon changes color based on status (🟢 Connected, 🟡 Disabled, 🔴 Dropped, ⚫ Server Unreachable).

## 🏗️ Architecture

```
[Client PC]  ──→  [Raspberry Pi (OpenVPN gateway)]  ──→  [Internet via VPN]
  tray app            backend web UI + switch script
```

The tray app modifies the client's routing table to send all traffic through the Pi.
The Pi runs OpenVPN and a small PHP web UI (`backend/`) that the tray app queries to
fetch available locations and trigger server switches.

## 🖥️ Backend Setup (Raspberry Pi)

### Prerequisites

- Raspberry Pi running Debian/Raspberry Pi OS
- Apache2 + PHP 8.x + php-fpm
- OpenVPN with `.ovpn` config files from your provider

### 1. Place your `.ovpn` files

Put all your provider's `.ovpn` configs in `/etc/openvpn/express/`.

### 2. Install the switch script

```bash
sudo cp backend/switch-openvpn.sh /usr/local/bin/
sudo chmod +x /usr/local/bin/switch-openvpn.sh
```

Edit the configuration variables at the top of the script:

| Variable | Description |
|---|---|
| `EXPRESS_DIR` | Directory containing your `.ovpn` files |
| `CLIENT_LINK` | Symlink path the OpenVPN service follows |
| `OPENVPN_SERVICE` | Your systemd service name (e.g. `openvpn-client@active`) |
| `CONF_OVERLAY` | Path to a shared credentials/options file, or `""` to disable |

Allow the web server to run it as root:

```bash
echo "www-data ALL=(root) NOPASSWD: /usr/local/bin/switch-openvpn.sh" \
  | sudo tee /etc/sudoers.d/switch-openvpn
```

### 3. Deploy the web UI

```bash
sudo cp -r backend/ /var/www/vpn/
sudo chown -R www-data:www-data /var/www/vpn/
```

Download the [flag-icons](https://github.com/lipis/flag-icons) library and place it at
`/var/www/vpn/flag-icons/`.

Edit `$CONF_PREFIX` at the top of `/var/www/vpn/index.php` to match your provider's filename prefix.
For example, ExpressVPN files are named `my_expressvpn_france_udp.ovpn`, so the prefix is `my_expressvpn_`.

### 4. Configure Apache

Copy and customise the example vhost:

```bash
sudo cp backend/apache-vhost.conf.example /etc/apache2/sites-available/vpn.conf
# Edit ServerName and the Require ip subnet to match your LAN
sudo a2ensite vpn
sudo systemctl reload apache2
```

---

## 💻 Tray App Setup

### Prerequisites

- **Python 3.10+**
- **pip** package manager

### Dependencies

```bash
pip install PyQt5 requests beautifulsoup4 pyinstaller pycountry
```

*On Linux, installing `python-pyqt5` via your package manager is recommended for better system integration.*

### First Run

On first launch the app will show a setup dialog. Enter:

| Field | Description |
|---|---|
| **VPN Gateway IP** | Your Pi's LAN IP address |
| **ISP Gateway IP** | Your router's LAN IP address |
| **DNS Server** | Usually the same as the VPN Gateway |
| **VPN Switcher URL** | URL of the backend web UI on the Pi |

Settings are saved to the config file and can be changed later via **⚙️ Settings...** in the tray menu.

### Building for Windows

The project includes a fully automated build script:

1. Open PowerShell in the project directory.
2. Run:
    ```powershell
    .\build_windows.ps1
    ```
3. The output executable will be in `dist\DragonFoxVPN Tray.exe`.

The build script auto-increments the build number and embeds version metadata. Requires `app.ico` and `version_info.txt` in the root directory.

## 🚀 Running

### Windows
Run `DragonFoxVPN Tray.exe` as **Administrator** (required to modify the routing table and network settings).

### Linux
```bash
sudo python dragonfox_vpn.py
```
Root is required for `ip` and `resolvectl` commands.

## ⚙️ Configuration

The application stores its configuration in:

- **Windows**: `%APPDATA%\DragonFoxVPN\dragonfox_vpn.json`
- **Linux**: `~/.config/dragonfox_vpn.json`

Flag icons are cached locally in a `flags` subdirectory to reduce bandwidth.

## 🤝 Contributing

### Versioning Strategy
- **Major.Minor.Patch.Build** (e.g., `1.0.1.35`)
- The **Build** number is automatically incremented by `increment_version.py` on every Windows build.
- **Major/Minor/Patch** are manually controlled in `version_info.txt`.

## 📜 License

Copyright (c) 2026 DragonFox Studios. All rights reserved.
