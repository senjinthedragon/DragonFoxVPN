# DragonFoxVPN Tray

A system tray utility for managing VPN connections with a modern dark UI.

Designed to work on **Windows 10/11** and **Linux (Garuda/Arch/Debian)**. Built around a
**Raspberry Pi gateway** architecture where a Pi running OpenVPN sits between your LAN and the
internet, with this tray app managing routing on each client machine.

> **VPN provider**: The included backend is written for **ExpressVPN** `.ovpn` configs, but the
> shell script and web UI are straightforward to adapt to any OpenVPN-compatible provider.

## Features

- **Location Switching**:
  - Searchable dialog with flags
  - Grouped by continent (Europe, Asia, Americas, etc.)
  - Favorites system
- **Smart Automation** (configured in Settings):
  - **Auto-Connect**: Connects to the last used location on app launch.
  - **Auto-Reconnect**: Optionally re-enables the VPN automatically when the server comes back online after a drop. Disabled by default — see the security note below.
  - **Run on Startup**: (Windows) Option to launch automatically on system login.
- **Security & Safety**:
  - **Kill Switch**: Blocks internet access if the VPN connection drops unexpectedly. When the VPN drops, the route is immediately removed and all traffic stops until you manually re-enable it — or until the server returns if Auto-Reconnect is enabled.
  - **Drop Debouncing**: Requires two consecutive failed checks before triggering the kill switch, avoiding false positives.
  - **DNS Leak Protection**: Automatically flushes DNS caches and enforces VPN DNS.
- **Real-time Monitoring**:
  - Dashboard showing connection status, gateway IP, network adapter, and session duration.
  - Tray icon changes colour based on status (green=Connected, yellow=Disabled, red=Dropped, grey=Server Unreachable).
  - System notifications for critical events (kill switch, unexpected drops, server unreachable).
- **Localization**: Available in English, German, French, Spanish, Portuguese (Brazil), Italian, Russian, Simplified Chinese, Japanese, and Korean. Language is auto-detected from your system locale and can be changed in Settings.

## Security Note: Kill Switch and Auto-Reconnect

By default, when the VPN drops the kill switch removes the routing rule and **all internet traffic stops** until you manually click "Enable VPN". This is intentional — it ensures no unprotected traffic ever leaves your machine without your explicit action.

The **Auto-Reconnect** option (in the Settings window under Behavior) will automatically re-enable the VPN when the server comes back online. This is convenient for situations like a scheduled Pi reboot, but it comes with a trade-off: you are trusting that the reconnection will succeed and that no traffic slips through in the window between the kill switch firing and the VPN being restored. If you use the VPN for strict privacy or to protect sensitive downloads, leave Auto-Reconnect **off** and reconnect manually.

## Architecture

```text
[Client PC]  →  [Raspberry Pi (OpenVPN gateway)]  →  [Internet via VPN]
  tray app   ←   backend web UI + switch script
```

The tray app modifies the **client machine's** routing table to send all traffic through the Pi.
The Pi runs OpenVPN and a small PHP web UI (`backend/`) that the tray app queries to fetch
available locations and trigger server switches. Each client machine runs the tray app
independently — no router configuration required.

---

## Backend Setup (Raspberry Pi)

### Prerequisites

```bash
sudo apt install openvpn apache2 php8.2 libapache2-mod-php8.2
sudo systemctl restart apache2
```

### 1. Create your config file

All backend configuration lives in one place. Copy the example and edit it:

```bash
sudo mkdir -p /etc/dragonfoxvpn
sudo cp backend/dragonfox.conf.example /etc/dragonfoxvpn/config.conf
sudo nano /etc/dragonfoxvpn/config.conf
```

The file is well-commented. The values you'll need to change are:

| Setting       | How to find it                                                                  |
| ------------- | ------------------------------------------------------------------------------- |
| `LAN_IF`      | Run `ip link` — it's the interface with your LAN IP (usually `eth0`)            |
| `LAN_NET`     | Your router's subnet, e.g. `192.168.1.0/24` — check your router's DHCP settings |
| `PI_IP`       | The Pi's own LAN IP address                                                     |
| `CONF_PREFIX` | The common prefix of your `.ovpn` filenames, e.g. `my_expressvpn_`              |

Everything else can be left as the default unless you have a specific reason to change it.

### 2. Enable IP forwarding

```bash
echo "net.ipv4.ip_forward=1" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

### 3. Place your `.ovpn` files

```bash
sudo mkdir -p /etc/openvpn/client/configs
sudo cp *.ovpn /etc/openvpn/client/configs/
```

> **ExpressVPN users**: Log into your account → Downloads → Manual Config → OpenVPN.
> Download the configs for the locations you want and copy them to the Pi.
> Each user must download their own — the files contain account-specific credentials.

### 4. Set up the routing script

```bash
sudo cp backend/vpn-route-up.sh /etc/openvpn/client/
sudo chmod +x /etc/openvpn/client/vpn-route-up.sh
```

> You never run this script manually. OpenVPN calls it automatically every time the tunnel
> comes up, via the `up` directive in `common.conf`. It reads its settings from your config file.

### 5. Set up the shared OpenVPN config

```bash
sudo cp backend/common.conf.example /etc/openvpn/client/common.conf
```

Create a credentials file with your VPN provider username and password:

```bash
sudo nano /etc/openvpn/client/credentials.txt
# Line 1: your username
# Line 2: your password
sudo chmod 600 /etc/openvpn/client/credentials.txt
```

> **Providers with embedded credentials**: Some providers include credentials directly inside
> the `.ovpn` files rather than using a separate auth file. If yours does, comment out the
> `auth-user-pass` line in `common.conf` — otherwise OpenVPN will fail to start.

### 6. Install and enable the switch script

```bash
sudo cp backend/switch-openvpn.sh /usr/local/bin/
sudo chmod +x /usr/local/bin/switch-openvpn.sh
```

Allow the web server to run it as root:

```bash
echo "www-data ALL=(root) NOPASSWD: /usr/local/bin/switch-openvpn.sh" \
  | sudo tee /etc/sudoers.d/switch-openvpn
```

Enable the OpenVPN service:

```bash
sudo systemctl enable --now openvpn-client@active
```

### 7. Deploy the web UI

```bash
sudo cp -r backend/. /var/www/vpn/
sudo chown -R www-data:www-data /var/www/vpn/
```

### 8. Configure Apache

```bash
sudo cp backend/apache-vhost.conf.example /etc/apache2/sites-available/vpn.conf
```

Open the file and edit two lines to match your setup:

- `ServerName` — the hostname or IP you'll use to reach the Pi (e.g. `vpn.local` or `10.0.0.20`)
- `Require ip` — your LAN subnet (e.g. `192.168.1.0/24`)

```bash
sudo a2ensite vpn
sudo systemctl reload apache2
```

---

## Tray App Setup

### Prerequisites

- **Rust stable toolchain** — install via [rustup.rs](https://rustup.rs)
- **Windows**: MSVC build tools (Visual Studio Build Tools or Visual Studio with the C++ workload)
- **Linux**: `libappindicator3-dev` or `libayatana-appindicator3-dev` for the system tray

```bash
# Arch/Garuda
sudo pacman -S libayatana-appindicator

# Debian/Ubuntu
sudo apt install libayatana-appindicator3-dev
```

### Building

```bash
cargo build --release
```

The output binary is placed at:

- **Linux**: `target/release/DragonFoxVPN`
- **Windows**: `target\release\DragonFoxVPN.exe`

On Windows you can also use the included PowerShell script:

```powershell
.\build_windows.ps1
```

### Linux: passwordless sudo for network commands

The tray app uses `ip`, `resolvectl`, and `sysctl` to manage routing. Rather than running the
whole app as root, grant your user passwordless access to just those commands:

```bash
sudo nano /etc/sudoers.d/dragonfoxvpn
```

Add this line, replacing `yourusername` with your actual username:

```text
yourusername ALL=(root) NOPASSWD: /sbin/ip, /usr/bin/resolvectl, /sbin/sysctl, /usr/bin/systemd-resolve
```

Then run the app normally (no sudo):

```bash
./target/release/DragonFoxVPN
```

### First Run

On first launch the app shows a setup dialog. Here's what each field means:

| Field                | What to enter                                                                                     |
| -------------------- | ------------------------------------------------------------------------------------------------- |
| **VPN Switcher URL** | `http://` or `https://` followed by your Pi's IP or hostname, e.g. `http://10.0.0.20`            |
| **VPN Server IP**    | Your Pi's LAN IP address (the same IP you SSH into it with). Auto-filled when you enter the URL. |
| **Router IP**        | Your router's LAN IP — usually `192.168.1.1` or `10.0.0.1`; check via `ip route \| grep default` |

The **VPN Server IP** field is automatically populated via DNS lookup when you enter a valid Switcher URL. You can override it manually if needed.

Use the **Test Connection** button to verify all three values before saving — it checks that the switcher page is reachable and returns locations, and that both IPs respond to pings.

### Settings

Access **Settings...** from the tray menu at any time to update network configuration or adjust behaviour. Settings are organised into three sections:

- **Network**: VPN Switcher URL, VPN Server IP, Router IP, and the Test Connection button.
- **Behavior**: Auto-Connect on start, Auto-Reconnect if server returns, and Run on Startup (Windows only).
- **Language**: Select from 10 supported languages. Takes effect after restart.

## Running

### Windows

Run `DragonFoxVPN.exe` as **Administrator** (required to modify the routing table and network settings).

### Linux

```bash
./target/release/DragonFoxVPN
```

(No sudo needed if you followed the sudoers step above.)

## Configuration

The application stores its configuration in:

- **Windows**: `%APPDATA%\DragonFoxVPN\dragonfox_vpn.json`
- **Linux**: `~/.config/dragonfox_vpn.json`

Flag icons are cached locally in a `flags` subdirectory alongside the config file.

---

## Troubleshooting

### Web UI not accessible

- Confirm Apache is running: `sudo systemctl status apache2`
- Check the vhost `Require ip` line matches your client's subnet
- Try accessing by IP directly: `http://<pi-ip>/` to rule out DNS issues

### OpenVPN tunnel not coming up

- Check service status: `sudo systemctl status openvpn-client@active`
- View logs: `sudo journalctl -u openvpn-client@active -n 50`
- Verify your credentials file has the correct username on line 1 and password on line 2
- Confirm at least one `.ovpn` file exists in `EXPRESS_DIR` and run `switch-openvpn.sh --refresh`

### Location switching does nothing

- Confirm the sudoers entry for `www-data` is in place: `sudo cat /etc/sudoers.d/switch-openvpn`
- Test the script manually: `sudo /usr/local/bin/switch-openvpn.sh --refresh`
- Check Apache error log: `sudo tail /var/log/apache2/error.log`

### Tray shows "Server Unreachable"

- The Pi is not responding to pings from the client — check they are on the same LAN
- Confirm the VPN Gateway IP in Settings matches the Pi's actual LAN IP

### Kill switch triggers unexpectedly

- This usually means `traceroute` to `8.8.8.8` isn't seeing the VPN gateway as the first hop
- Confirm `vpn-route-up.sh` executed successfully by checking OpenVPN logs
- Verify IP forwarding is still enabled: `sysctl net.ipv4.ip_forward` (should return `1`)

### Routing not restored after disabling VPN

- The app removes the VPN route but your default route should return automatically
- If internet is still broken, run: `sudo ip route add default via <your-router-ip>`

---

## License

MIT License — see [LICENSE](LICENSE) for details.

Bundled dependency: [flag-icons](https://github.com/lipis/flag-icons) by Panayiotis Lipiridis (MIT).
