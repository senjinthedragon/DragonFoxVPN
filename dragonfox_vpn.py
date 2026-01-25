#!/usr/bin/env python3
"""
DragonFox VPN Tray Application
==============================

A professional-grade system tray utility for managing VPN connections.
Features a modern dark UI, location switching, auto-connect capabilities,
and robust background network state management. Supports Linux and Windows.

Copyright (c) 2026 DragonFox Studios
"""

__version__ = "1.0.1.30"

import datetime
import json
import logging
import os
import platform
import re
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import List, Tuple, Optional, Dict, Any

import requests
import urllib3
from bs4 import BeautifulSoup
from PyQt5.QtCore import QTimer, Qt, QThread, pyqtSignal, QSize, QPoint, QObject, QUrl
from PyQt5.QtGui import QIcon, QPixmap, QPainter, QColor, QFont, QBrush, QPen, QRadialGradient, QImage
from PyQt5.QtWidgets import (QApplication, QSystemTrayIcon, QMenu, QAction, QDialog,
                             QVBoxLayout, QHBoxLayout, QLabel, QPushButton, QWidget,
                             QMessageBox, QListWidget, QListWidgetItem, QLineEdit,
                             QAbstractItemView, QFrame, QProgressBar, QStyle)
import pycountry


# --- Logging Configuration ---
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    handlers=[
        logging.StreamHandler(sys.stdout)
    ]
)
logger = logging.getLogger("DragonFoxVPN")

# --- Constants & Configuration ---
class Config:
    """Global configuration constants."""
    ISP_GATEWAY: str = "10.0.0.1"
    VPN_GATEWAY: str = "10.0.0.20"
    DNS_SERVER: str = "10.0.0.20"
    CHECK_INTERVAL: int = 3000  # milliseconds
    VPN_SWITCHER_URL: str = "https://vpn.hatchling.org"
    
    @staticmethod
    def get_config_path() -> Path:
        """Get the platform-specific configuration file path."""
        if platform.system() == "Windows":
            base_dir = Path(os.getenv("APPDATA", os.path.expanduser("~"))) / "DragonFoxVPN"
        else:
            base_dir = Path.home() / ".config"
        
        base_dir.mkdir(parents=True, exist_ok=True)
        return base_dir / "dragonfox_vpn.json"

CONFIG_FILE = Config.get_config_path()

# Suppress SSL warnings for self-signed certs
urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)

# --- Modern Dark Theme ---
STYLESHEET = """
    QWidget {
        background-color: #1e1e1e;
        color: #e0e0e0;
        font-family: 'Segoe UI', 'Roboto', sans-serif;
        font-size: 14px;
    }
    QDialog {
        background-color: #1e1e1e;
        border: 1px solid #333;
    }
    QLineEdit {
        background-color: #2d2d2d;
        border: 1px solid #3d3d3d;
        border-radius: 4px;
        padding: 8px;
        color: #ffffff;
        selection-background-color: #007acc;
    }
    QLineEdit:focus {
        border: 1px solid #007acc;
    }
    QListWidget {
        background-color: #252526;
        border: 1px solid #333;
        border-radius: 4px;
        outline: none;
    }
    QListWidget::item {
        padding: 8px;
        border-bottom: 1px solid #2d2d2d;
    }
    QListWidget::item:selected {
        background-color: #37373d;
        color: #ffffff;
        border-left: 3px solid #007acc;
    }
    QListWidget::item:hover {
        background-color: #2a2d2e;
    }
    QListWidget::item#header {
        background-color: #333333;
        color: #aaaaaa;
        font-weight: bold;
        border-bottom: 2px solid #444;
        padding-top: 10px;
        padding-bottom: 5px;
    }
    QPushButton {

        background-color: #0e639c;
        color: white;
        border: none;
        padding: 8px 16px;
        border-radius: 4px;
        font-weight: bold;
    }
    QPushButton:hover {
        background-color: #1177bb;
    }
    QPushButton:pressed {
        background-color: #094770;
    }
    QPushButton:disabled {
        background-color: #333;
        color: #666;
    }
    QPushButton#cancel_btn {
        background-color: #3c3c3c;
    }
    QPushButton#cancel_btn:hover {
        background-color: #4c4c4c;
    }
    QLabel#header {
        font-size: 18px;
        font-weight: bold;
        color: #ffffff;
        margin-bottom: 10px;
    }
    QLabel#status_label {
        font-weight: bold;
        padding: 5px;
        border-radius: 3px;
    }
    QProgressBar {
        border: 1px solid #333;
        border-radius: 2px;
        text-align: center;
        background-color: #2d2d2d;
    }
    QProgressBar::chunk {
        background-color: #007acc;
    }
"""

# --- Global State ---
class AppState:
    """Manages the current state of the application."""
    vpn_state: str = "Disabled"
    vpn_location: str = "Unknown"
    connection_start_time: Optional[datetime.datetime] = None
    adapter_name: str = "auto"
    manual_disable: bool = True
    
    # Icons (initialized later)
    icon_connected: Optional[QIcon] = None
    icon_disabled: Optional[QIcon] = None
    icon_dropped: Optional[QIcon] = None
    icon_info: Optional[QIcon] = None

# --- System Operations ---
class SystemHandler:
    """Handles OS-specific operations like routing and DNS."""
    
    @staticmethod
    def run_command(cmd: str, check: bool = True) -> Tuple[str, str, int]:
        """Runs a system command and returns stdout, stderr, and return code."""
        try:
            result = subprocess.run(cmd, shell=True, capture_output=True, text=True, check=check)
            return result.stdout.strip(), result.stderr.strip(), result.returncode
        except subprocess.CalledProcessError as e:
            return e.stdout, e.stderr, e.returncode
        except Exception as e:
            logger.error(f"Command execution error: {e}")
            return "", str(e), -1

    @staticmethod
    def get_active_adapter() -> str:
        """Detects the active network adapter name."""
        if platform.system() == "Windows":
            stdout, _, code = SystemHandler.run_command("netsh interface ipv4 show interfaces")
            if code == 0:
                # Simple heuristic for Windows: look for 'connected' interface
                lines = stdout.splitlines()
                for line in lines:
                    if "connected" in line.lower() and "loopback" not in line.lower():
                        parts = line.split()
                        # Usually last column is the name
                        return parts[-1]
            return "Ethernet" # Fallback
        else:
            stdout, _, code = SystemHandler.run_command("ip route show default")
            if code == 0 and stdout:
                match = re.search(r'dev (\S+)', stdout)
                if match:
                    return match.group(1)
            return "eno1" # Fallback

    @staticmethod
    def flush_dns() -> None:
        """Flushes the system DNS cache."""
        if platform.system() == "Windows":
            SystemHandler.run_command("ipconfig /flushdns", check=False)
        else:
            SystemHandler.run_command("sudo systemd-resolve --flush-caches", check=False)
            SystemHandler.run_command("sudo resolvectl flush-caches", check=False)

    @staticmethod
    def enable_routing(adapter: str, vpn_gw: str, vpn_dns: str) -> bool:
        """Configures system routing to use the VPN."""
        if platform.system() == "Windows":
            # Requires Admin privilegs
            SystemHandler.run_command(f"route delete 0.0.0.0 mask 0.0.0.0", check=False)
            _, _, code = SystemHandler.run_command(f"route add 0.0.0.0 mask 0.0.0.0 {vpn_gw} metric 1")
            # DNS on Windows usually requires netsh
            SystemHandler.run_command(f'netsh interface ipv4 set dns name="{adapter}" static {vpn_dns}', check=False)
            return code == 0
        else:
            SystemHandler.run_command(f"sudo sysctl -w net.ipv6.conf.{adapter}.disable_ipv6=1", check=False)
            SystemHandler.run_command(f"sudo resolvectl dns {adapter} {vpn_dns}", check=False)
            SystemHandler.run_command(f"sudo ip route del default dev {adapter}", check=False)
            _, _, code = SystemHandler.run_command(f"sudo ip route add default via {vpn_gw} dev {adapter} metric 50")
            return code == 0

    @staticmethod
    def disable_routing(adapter: str, vpn_gw: str) -> None:
        """Restores default system routing."""
        if platform.system() == "Windows":
            SystemHandler.run_command(f"route delete 0.0.0.0 mask 0.0.0.0 {vpn_gw}", check=False)
            # Re-add local gateway if needed, but usually Windows handles this if we just delete
            SystemHandler.run_command(f'netsh interface ipv4 set dns name="{adapter}" source=dhcp', check=False)
        else:
            SystemHandler.run_command(f"sudo ip route del default via {vpn_gw} dev {adapter}", check=False)
            SystemHandler.run_command(f"sudo sysctl -w net.ipv6.conf.{adapter}.disable_ipv6=0", check=False)
            SystemHandler.run_command(f"sudo resolvectl revert {adapter}", check=False)

    @staticmethod
    def check_connection(vpn_gw: str, isp_gw: str) -> bool:
        """Checks if the first hop is the VPN gateway."""
        if platform.system() == "Windows":
            stdout, _, code = SystemHandler.run_command("tracert -d -h 1 8.8.8.8", check=False)
        else:
            stdout, _, code = SystemHandler.run_command("traceroute -n -m 1 -w 1 8.8.8.8", check=False)
        
        if code == 0 and stdout:
            # Look for an IP address in the first hop
            ips = re.findall(r'\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}', stdout)
            if ips:
                first_hop = ips[0]
                if first_hop == isp_gw:
                    return False
                return True # If it's not the ISP GW, we're likely on VPN
        return False

    @staticmethod
    def is_route_active(vpn_gw: str, adapter: str) -> bool:
        """Checks if the VPN route exists in the routing table."""
        if platform.system() == "Windows":
            stdout, _, code = SystemHandler.run_command("route print", check=False)
            return code == 0 and vpn_gw in stdout
        else:
            stdout, _, code = SystemHandler.run_command(f"ip route show default via {vpn_gw} dev {adapter}", check=False)
            return code == 0 and bool(stdout.strip())

# --- Icon Factory ---
def create_status_icon(color_name: str) -> QIcon:
    """Creates a high-quality antialiased status icon programmatically."""
    colors = {
        "green": (QColor("#4CAF50"), QColor("#2E7D32")),
        "yellow": (QColor("#FFC107"), QColor("#FFA000")),
        "red": (QColor("#F44336"), QColor("#D32F2F")),
        "blue": (QColor("#2196F3"), QColor("#1976D2"))
    }
    
    main_color, border_color = colors.get(color_name, colors["yellow"])
    
    pixmap = QPixmap(64, 64)
    pixmap.fill(Qt.transparent)
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing)
    
    gradient = QRadialGradient(32, 32, 30)
    gradient.setColorAt(0, main_color.lighter(120))
    gradient.setColorAt(1, main_color)
    
    painter.setBrush(QBrush(gradient))
    painter.setPen(QPen(border_color, 2))
    painter.drawEllipse(4, 4, 56, 56)
    
    shine_gradient = QRadialGradient(32, 20, 20)
    shine_gradient.setColorAt(0, QColor(255, 255, 255, 100))
    shine_gradient.setColorAt(1, Qt.transparent)
    painter.setBrush(QBrush(shine_gradient))
    painter.setPen(Qt.NoPen)
    painter.drawEllipse(10, 8, 44, 25)
    
    painter.end()
    return QIcon(pixmap)

# --- Icon & Flag Manager ---
class IconManager:
    """Manages fetching and caching of flag icons."""
    CACHE_DIR = Config.get_config_path().parent / "flags"
    
    @staticmethod
    def get_flag_icon(country_name: str) -> Tuple[Optional[QIcon], str]:
        """Returns QIcon for a country, fetching it if necessary.
           Returns (QIcon, iso_code) or (None, '')."""
        
        # Manual overrides for non-standard names
        overrides = {
            "usa": "us", "uk": "gb", "south korea": "kr", "russia": "ru",
            "czech republic": "cz", "north macedonia": "mk", "moldova": "md",
            "laos": "la", "vietnam": "vn", "tanzania": "tz", "bolivia": "bo",
            "venezuela": "ve", "iran": "ir", "syria": "sy", "brunei": "bn",
            "cape verde": "cv", "congo": "cg", "democratic republic of the congo": "cd",
            "swaziland": "sz", "timor-leste": "tl", "vatican city": "va",
            "palestine": "ps", "taiwan": "tw", "hong kong": "hk", "macau": "mo",
            "india via singapore": "in", "india via uk": "in", "india via uk": "in" 
        }
        
        norm_name = country_name.lower().strip()
        iso_code = overrides.get(norm_name)
        
        if not iso_code:
            try:
                # Try explicit lookup first
                c = pycountry.countries.get(name=country_name)
                if not c:
                    # Try fuzzy search
                    matches = pycountry.countries.search_fuzzy(country_name)
                    if matches:
                        c = matches[0]
                
                if c:
                    iso_code = c.alpha_2.lower()
            except LookupError:
                pass
        
        if not iso_code:
            return None, ""
            
        IconManager.CACHE_DIR.mkdir(parents=True, exist_ok=True)
        flag_path = IconManager.CACHE_DIR / f"{iso_code}.png"
        
        if flag_path.exists():
            return QIcon(str(flag_path)), iso_code
            
        return None, iso_code

    @staticmethod
    def fetch_flag(iso_code: str):
        """Fetches flag from CDN and saves to cache. Intended to run in a thread."""
        if not iso_code: return
        try:
            IconManager.CACHE_DIR.mkdir(parents=True, exist_ok=True)
            flag_path = IconManager.CACHE_DIR / f"{iso_code}.png"
            if not flag_path.exists():
                url = f"https://flagcdn.com/48x36/{iso_code}.png"
                resp = requests.get(url, timeout=5)
                if resp.status_code == 200:
                    with open(flag_path, 'wb') as f:
                        f.write(resp.content)
        except Exception as e:
            logger.error(f"Failed to fetch flag for {iso_code}: {e}")

class FlagLoaderThread(QThread):
    """Worker to fetch flags in background to avoid UI freeze."""
    icon_ready = pyqtSignal(str) # Emits iso_code when ready
    
    def __init__(self, iso_codes):
        super().__init__()
        self.iso_codes = set(iso_codes)
        
    def run(self):
        for code in self.iso_codes:
            IconManager.fetch_flag(code)
            self.icon_ready.emit(code)


# --- Configuration Manager ---
class ConfigManager:
    """Manages persistent configuration settings."""
    def __init__(self):
        self.config = {
            "favorites": [],
            "auto_connect": False,
            "last_location": None
        }
        self.load()

    def load(self):
        """Loads configuration from the filesystem."""
        if CONFIG_FILE.exists():
            try:
                with open(CONFIG_FILE, 'r') as f:
                    self.config.update(json.load(f))
            except Exception as e:
                logger.error(f"Failed to load config: {e}")

    def save(self):
        """Saves configuration to the filesystem."""
        try:
            CONFIG_FILE.parent.mkdir(parents=True, exist_ok=True)
            with open(CONFIG_FILE, 'w') as f:
                json.dump(self.config, f, indent=4)
        except Exception as e:
            logger.error(f"Failed to save config: {e}")

    def get(self, key: str) -> Any:
        return self.config.get(key)

    def set(self, key: str, value: Any):
        self.config[key] = value
        self.save()

    def is_favorite(self, location_label: str) -> bool:
        return location_label in self.config["favorites"]

    def toggle_favorite(self, location_label: str):
        favs = self.config["favorites"]
        if location_label in favs:
            favs.remove(location_label)
        else:
            favs.append(location_label)
        self.save()

# Global config instance
config_manager = ConfigManager()

# --- VPN API ---
class VPNApi:
    """Handles interaction with the VPN switcher web backend."""
    
    CONTINENT_EMOJIS = {
        "Europe": "🌍", "Africa": "🌍", "Asia": "🌏", "Oceania": "🌏",
        "North America": "🌎", "South America": "🌎", "Other": "🌐"
    }
    
    COUNTRY_EMOJIS = {
        "albania": "🇦🇱", "algeria": "🇩🇿", "andorra": "🇦🇩", "argentina": "🇦🇷", "armenia": "🇦🇲",
        "australia": "🇦🇺", "austria": "🇦🇹", "azerbaijan": "🇦🇿", "bahamas": "🇧🇸", "bangladesh": "🇧🇩",
        "belarus": "🇧🇾", "belgium": "🇧🇪", "bermuda": "🇧🇲", "bhutan": "🇧🇹", "bolivia": "🇧🇴",
        "bosnia and herzegovina": "🇧🇦", "brazil": "🇧🇷", "brunei": "🇧🇳", "bulgaria": "🇧🇬", 
        "cambodia": "🇰🇭", "canada": "🇨🇦", "cayman islands": "🇰🇾", "chile": "🇨🇱", "colombia": "🇨🇴",
        "costa rica": "🇨🇷", "croatia": "🇭🇷", "cuba": "🇨🇺", "cyprus": "🇨🇾", "czech republic": "🇨🇿",
        "denmark": "🇩🇰", "dominican republic": "🇩🇴", "ecuador": "🇪🇨", "egypt": "🇪🇬", "estonia": "🇪🇪",
        "finland": "🇫🇮", "france": "🇫🇷", "georgia": "🇬🇪", "germany": "🇩🇪", "ghana": "🇬🇭",
        "greece": "🇬🇷", "guam": "🇬🇺", "guatemala": "🇬🇹", "honduras": "🇭🇳", "hong kong": "🇭🇰",
        "hungary": "🇭🇺", "iceland": "🇮🇸", "india": "🇮🇳", "indonesia": "🇮🇩", "ireland": "🇮🇪",
        "isle of man": "🇮🇲", "israel": "🇮🇱", "italy": "🇮🇹", "jamaica": "🇯🇲", "japan": "🇯🇵",
        "jersey": "🇯🇪", "kazakhstan": "🇰🇿", "kenya": "🇰🇪", "laos": "🇱🇦", "latvia": "🇱🇻",
        "lebanon": "🇱🇧", "liechtenstein": "🇱🇮", "lithuania": "🇱🇹", "luxembourg": "🇱🇺", "macau": "🇲🇴",
        "malaysia": "🇲🇾", "malta": "🇲🇹", "mexico": "🇲🇽", "moldova": "🇲🇩", "monaco": "🇲🇨",
        "mongolia": "🇲🇳", "montenegro": "🇲🇪", "morocco": "🇲🇦", "myanmar": "🇲🇲", "nepal": "🇳🇵",
        "netherlands": "🇳🇱", "new zealand": "🇳🇿", "north macedonia": "🇲🇰", "norway": "🇳🇴",
        "pakistan": "🇵🇰", "panama": "🇵🇦", "peru": "🇵🇪", "philippines": "🇵🇭", "poland": "🇵🇱",
        "portugal": "🇵🇹", "puerto rico": "🇵🇷", "romania": "🇷🇴", "serbia": "🇷🇸", "singapore": "🇸🇬",
        "slovakia": "🇸🇰", "slovenia": "🇸🇮", "south africa": "🇿🇦", "south korea": "🇰🇷",
        "spain": "🇪🇸", "sri lanka": "🇱🇰", "sweden": "🇸🇪", "switzerland": "🇨🇭", "taiwan": "🇹🇼",
        "thailand": "🇹🇭", "trinidad and tobago": "🇹🇹", "turkey": "🇹🇷", "uk": "🇬🇧",
        "ukraine": "🇺🇦", "uruguay": "🇺🇾", "usa": "🇺🇸", "uzbekistan": "🇺🇿", "venezuela": "🇻🇪",
        "vietnam": "🇻🇳"
    }

    @staticmethod
    def fetch_locations() -> Tuple[List[Dict[str, str]], Optional[str]]:
        """Fetches available VPN locations from the backend."""
        try:
            response = requests.get(Config.VPN_SWITCHER_URL, timeout=5, verify=False)
            response.raise_for_status()
            soup = BeautifulSoup(response.text, 'html.parser')
            
            locations = []
            current_location = None
            current_continent = None
            
            for element in soup.select('.dropdown-content > *'):
                if 'optgroup-label' in element.get('class', []):
                    continent_text = element.get_text().strip()
                    for emoji in VPNApi.CONTINENT_EMOJIS.values():
                        continent_text = continent_text.replace(emoji, '').strip()
                    current_continent = continent_text
                elif 'dropdown-item' in element.get('class', []):
                    value = element.get('data-value', '')
                    label = element.get_text().strip()
                    is_active = 'active' in element.get('class', [])
                    
                    if current_continent and value:
                        country_name = label.split(' - ')[0].lower().strip()
                        for emoji in VPNApi.COUNTRY_EMOJIS.values():
                            label = label.replace(emoji, '').strip()
                        
                        locations.append({
                            'continent': current_continent,
                            'value': value,
                            'label': label,
                            'country': country_name
                        })
                        
                        if is_active:
                            current_location = label
            
            return locations, current_location
        except Exception as e:
            logger.error(f"Failed to fetch VPN locations: {e}")
            return [], None

    @staticmethod
    def switch_location(location_value: str) -> Tuple[bool, str]:
        """Switches VPN location via a POST request."""
        try:
            response = requests.post(
                Config.VPN_SWITCHER_URL,
                data={'location': location_value},
                timeout=10,
                verify=False
            )
            response.raise_for_status()
            return True, "Location switched successfully"
        except Exception as e:
            logger.error(f"Failed to switch location: {e}")
            return False, f"Failed to switch location: {e}"

# --- Concurrency Threads ---
class LocationFetcherThread(QThread):
    """Worker thread for non-blocking location fetching."""
    finished = pyqtSignal(list, str)
    def run(self):
        locs, curr = VPNApi.fetch_locations()
        self.finished.emit(locs, curr)

class NetworkMonitorThread(QThread):
    """Worker thread for network status monitoring."""
    status_checked = pyqtSignal(bool, bool)
    def run(self):
        vpn_active = SystemHandler.check_connection(Config.VPN_GATEWAY, Config.ISP_GATEWAY)
        route_exists = SystemHandler.is_route_active(Config.VPN_GATEWAY, AppState.adapter_name)
        self.status_checked.emit(vpn_active, route_exists)

class LocationSwitchThread(QThread):
    """Worker thread for location switching operations."""
    finished = pyqtSignal(bool, str, str)
    def __init__(self, value, label):
        super().__init__()
        self.value = value
        self.label = label
    def run(self):
        success, message = VPNApi.switch_location(self.value)
        if success:
            time.sleep(2)
        self.finished.emit(success, message, self.label if success else "")

# --- UI Components ---
class ModernLocationDialog(QDialog):
    """Searchable dialog for VPN location selection."""
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Change VPN Location")
        self.resize(550, 700)
        self.selected_value = None
        self.selected_label = None
        self.locations = []
        
        layout = QVBoxLayout(self)
        layout.setSpacing(15)
        layout.setContentsMargins(20, 20, 20, 20)
        
        header = QLabel("Select VPN Location")
        header.setObjectName("header")
        header.setAlignment(Qt.AlignCenter)
        layout.addWidget(header)
        
        self.search_input = QLineEdit()
        self.search_input.setPlaceholderText("🔍 Search countries or cities...")
        self.search_input.textChanged.connect(self.filter_locations)
        layout.addWidget(self.search_input)
        
        self.progress = QProgressBar()
        self.progress.setRange(0, 0)
        self.progress.setTextVisible(False)
        layout.addWidget(self.progress)
        
        self.list_widget = QListWidget()
        self.list_widget.setSelectionMode(QAbstractItemView.SingleSelection)
        self.list_widget.itemClicked.connect(self.on_item_clicked)
        self.list_widget.setContextMenuPolicy(Qt.CustomContextMenu)
        self.list_widget.customContextMenuRequested.connect(self.show_context_menu)
        layout.addWidget(self.list_widget)
        
        button_layout = QHBoxLayout()
        self.cancel_btn = QPushButton("Cancel")
        self.cancel_btn.setObjectName("cancel_btn")
        self.cancel_btn.clicked.connect(self.reject)
        button_layout.addWidget(self.cancel_btn)
        
        self.switch_btn = QPushButton("Switch Location")
        self.switch_btn.clicked.connect(self.on_switch_clicked)
        self.switch_btn.setEnabled(False)
        button_layout.addWidget(self.switch_btn)
        layout.addLayout(button_layout)
        
        self.fetcher = LocationFetcherThread()
        self.fetcher.finished.connect(self.on_locations_loaded)
        self.fetcher.start()

    def on_locations_loaded(self, locations, current):
        self.progress.hide()
        self.locations = locations
        if current:
            AppState.vpn_location = current
        self.populate_list()
        if current:
            items = self.list_widget.findItems(current, Qt.MatchContains)
            for item in items:
                if item.data(Qt.UserRole + 1) == current:
                    item.setSelected(True)
                    self.list_widget.scrollToItem(item)
                    self.on_item_clicked(item)
                    break

    def update_icons(self, iso_code):
        """Called when a flag icon is downloaded/ready."""
        # Find all items with this iso_code
        for i in range(self.list_widget.count()):
            item = self.list_widget.item(i)
            if item.data(Qt.UserRole + 2) == iso_code:
                icon_path = IconManager.CACHE_DIR / f"{iso_code}.png"
                if icon_path.exists():
                    item.setIcon(QIcon(str(icon_path)))

    def populate_list(self, filter_text=""):
        self.list_widget.clear()
        filter_text = filter_text.lower()
        
        # Sort by: Favorite -> Continent -> Label
        sorted_locs = sorted(self.locations, key=lambda x: (
            not config_manager.is_favorite(x['label']), 
            x.get('continent', 'Other'), 
            x['label']
        ))
        
        current_fav_status = None
        current_continent = None
        
        needed_flags = set()
        
        for loc in sorted_locs:
            if filter_text and filter_text not in loc['label'].lower():
                continue
                
            # Headers Logic
            is_fav = config_manager.is_favorite(loc['label'])
            continent = loc.get('continent', 'Other')
            
            # Add Favorite Header
            if is_fav != current_fav_status:
                current_fav_status = is_fav
                text = "Favorites" if is_fav else "All Locations"
                # If we switched to non-favorites, we reset continent tracking to force header
                if not is_fav:
                    current_continent = None
                    # Only show "All Locations" if we had favorites
                    if any(config_manager.is_favorite(l['label']) for l in self.locations):
                        self.add_header("All Locations")
                elif is_fav:
                    self.add_header("Favorites")
            
            # Add Continent Header (only for non-favorites usually, or grouped within favorites)
            if not is_fav and continent != current_continent:
                current_continent = continent
                self.add_header(continent)
                
            # Try to get Icon
            icon, iso_code = IconManager.get_flag_icon(loc['country'])
            if iso_code and not icon:
                needed_flags.add(iso_code)
                
            display_text = loc['label']
            if is_fav:
                 display_text = "⭐ " + display_text
            
            # If no icon available yet, and we are on Windows, we might want to avoid the ugly unicode
            # But the original code was: emoji + label.
            # If icon is present, use it.
            
            item = QListWidgetItem(display_text)
            if icon:
                item.setIcon(icon)
            
            item.setData(Qt.UserRole, loc['value'])
            item.setData(Qt.UserRole + 1, loc['label'])
            item.setData(Qt.UserRole + 2, iso_code) # Store ISO code for async update
            
            self.list_widget.addItem(item)
            
        if needed_flags:
            self.flag_loader = FlagLoaderThread(list(needed_flags))
            self.flag_loader.icon_ready.connect(self.update_icons)
            self.flag_loader.start()

    def add_header(self, text):
        item = QListWidgetItem(text)
        item.setFlags(Qt.NoItemFlags) # Non-selectable
        item.setData(Qt.UserRole, "header")
        # Use custom styling via QSS ID selector simulation or iterate in paint?
        # QListWidget doesn't support IDs per item easily. 
        # But we added QListWidget::item#header in CSS? No, that selector won't work on ITEM.
        # We have to set property or use setBackground.
        item.setForeground(QBrush(QColor("#aaaaaa")))
        item.setBackground(QColor("#333333"))
        font = item.font()
        font.setBold(True)
        item.setFont(font)
        self.list_widget.addItem(item)
    def filter_locations(self, text):
        self.populate_list(text)

    def on_item_clicked(self, item):
        self.selected_value = item.data(Qt.UserRole)
        self.selected_label = item.data(Qt.UserRole + 1)
        self.switch_btn.setEnabled(True)
        self.switch_btn.setText(f"Switch to {self.selected_label}")

    def show_context_menu(self, pos):
        item = self.list_widget.itemAt(pos)
        if not item: return
        label = item.data(Qt.UserRole + 1)
        menu = QMenu()
        is_fav = config_manager.is_favorite(label)
        action = menu.addAction("Remove from Favorites" if is_fav else "Add to Favorites")
        action.triggered.connect(lambda: self.toggle_favorite(label))
        menu.exec_(self.list_widget.mapToGlobal(pos))

    def toggle_favorite(self, label):
        config_manager.toggle_favorite(label)
        self.populate_list(self.search_input.text())
    
    def on_switch_clicked(self):
        self.switch_btn.setEnabled(False)
        self.cancel_btn.setEnabled(False)
        self.progress.show()
        self.switch_thread = LocationSwitchThread(self.selected_value, self.selected_label)
        self.switch_thread.finished.connect(self.on_switch_finished)
        self.switch_thread.start()
    
    def on_switch_finished(self, success, message, new_location):
        if success:
            AppState.vpn_location = new_location
            config_manager.set("last_location", new_location)
            self.accept()
        else:
            QMessageBox.warning(self, "Error", message)
            self.switch_btn.setEnabled(True)
            self.cancel_btn.setEnabled(True)
            self.progress.hide()

class StatusDashboard(QDialog):
    """Dashboard for real-time connection stats."""
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("DragonFox Status")
        self.resize(400, 300)
        
        layout = QVBoxLayout(self)
        layout.setSpacing(20)
        layout.setContentsMargins(30, 30, 30, 30)
        
        title = QLabel("DragonFox VPN")
        title.setAlignment(Qt.AlignCenter)
        title.setStyleSheet("font-size: 24px; font-weight: bold; color: #007acc;")
        layout.addWidget(title)
        
        self.status_frame = QFrame()
        self.status_frame.setStyleSheet("background-color: #252526; border-radius: 8px; border: 1px solid #333;")
        frame_layout = QVBoxLayout(self.status_frame)
        self.status_val = QLabel(AppState.vpn_state)
        self.status_val.setAlignment(Qt.AlignCenter)
        self.status_val.setStyleSheet(f"font-size: 18px; font-weight: bold; color: {self.get_color()};")
        frame_layout.addWidget(self.status_val)
        layout.addWidget(self.status_frame)
        
        self.loc_label = QLabel(f"🌍 Location: {AppState.vpn_location}")
        self.ip_label = QLabel(f"🔒 Gateway: {Config.VPN_GATEWAY}")
        self.time_label = QLabel("⏱️ Duration: --:--:--")
        
        for lbl in [self.loc_label, self.ip_label, self.time_label]:
            lbl.setStyleSheet("font-size: 14px; padding: 5px;")
            layout.addWidget(lbl)
            
        close_btn = QPushButton("Close")
        close_btn.clicked.connect(self.accept)
        layout.addWidget(close_btn)
        
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.update_stats)
        self.timer.start(1000)
        self.update_stats()

    def get_color(self):
        if AppState.vpn_state == "Connected": return "#4CAF50"
        if AppState.vpn_state == "Dropped": return "#F44336"
        return "#FFC107"

    def update_stats(self):
        self.status_val.setText(AppState.vpn_state.upper())
        self.status_val.setStyleSheet(f"font-size: 18px; font-weight: bold; color: {self.get_color()};")
        self.loc_label.setText(f"🌍 Location: {AppState.vpn_location}")
        
        if AppState.vpn_state == "Connected" and AppState.connection_start_time:
            duration = datetime.datetime.now() - AppState.connection_start_time
            self.time_label.setText(f"⏱️ Duration: {str(duration).split('.')[0]}")
        else:
            self.time_label.setText("⏱️ Duration: --:--:--")

# --- Main Application Logic ---
class VPNTrayApp(QApplication):
    """Main System Tray Application Controller."""
    def __init__(self, argv):
        super().__init__(argv)
        self.setQuitOnLastWindowClosed(False)
        self.setStyleSheet(STYLESHEET)
        
        # Anchor hidden widget for dialog parents
        self.anchor = QWidget()
        self.anchor.setAttribute(Qt.WA_DontShowOnScreen)
        self.anchor.hide()
        
        # Initialize icons
        AppState.icon_connected = create_status_icon("green")
        AppState.icon_disabled = create_status_icon("yellow")
        AppState.icon_dropped = create_status_icon("red")
        AppState.icon_info = create_status_icon("blue")
        
        # Tray setup
        self.tray_icon = QSystemTrayIcon(AppState.icon_disabled, self)
        self.setup_menu()
        self.tray_icon.show()
        self.tray_icon.activated.connect(self.on_tray_activated)
        
        self.init_app_state()
        
        # Background tasks
        self.timer = QTimer()
        self.timer.timeout.connect(self.check_vpn_status)
        self.timer.start(Config.CHECK_INTERVAL)
        
        self.monitor_thread = NetworkMonitorThread()
        self.monitor_thread.status_checked.connect(self.on_network_status_checked)

    def setup_menu(self):
        self.menu = QMenu()
        self.menu.setStyleSheet(STYLESHEET)
        
        self.action_dashboard = QAction("📊 Status Dashboard", self)
        self.action_dashboard.triggered.connect(self.on_dashboard)
        
        self.action_enable = QAction("Enable VPN", self)
        self.action_enable.triggered.connect(self.on_enable)
        
        self.action_disable = QAction("Disable VPN", self)
        self.action_disable.triggered.connect(self.on_disable)
        
        self.action_location = QAction("Change Location...", self)
        self.action_location.triggered.connect(self.on_change_location)
        
        self.action_autoconnect = QAction("Auto-Connect on Start", self)
        self.action_autoconnect.setCheckable(True)
        self.action_autoconnect.setChecked(config_manager.get("auto_connect"))
        self.action_autoconnect.triggered.connect(self.toggle_autoconnect)
        
        self.action_exit = QAction("Exit", self)
        self.action_exit.triggered.connect(self.on_exit)
        
        items = [self.action_dashboard, None, self.action_enable, self.action_disable, 
                 None, self.action_location, self.action_autoconnect, None, self.action_exit]
        for item in items:
            if item is None: self.menu.addSeparator()
            else: self.menu.addAction(item)
        
        self.tray_icon.setContextMenu(self.menu)
        self.update_ui_state()

    def init_app_state(self):
        AppState.adapter_name = SystemHandler.get_active_adapter()
        logger.info(f"Initialized with adapter: {AppState.adapter_name}")
        
        last_loc = config_manager.get("last_location")
        if last_loc:
            AppState.vpn_location = last_loc
            
        if config_manager.get("auto_connect"):
            logger.info("Auto-connect triggered.")
            QTimer.singleShot(1000, self.on_enable)
        else:
            self.fetch_thread = LocationFetcherThread()
            self.fetch_thread.finished.connect(self.on_initial_fetch_done)
            self.fetch_thread.start()

    def on_initial_fetch_done(self, locations, current):
        if current:
            AppState.vpn_location = current
            config_manager.set("last_location", current)
        self.update_ui_state()

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.DoubleClick:
            self.on_dashboard()
    
    def update_ui_state(self):
        """Refreshes iconography and menu states based on AppState."""
        tooltip = f"DragonFoxVPN: {AppState.vpn_state}\nLocation: {AppState.vpn_location}"
        self.tray_icon.setToolTip(tooltip)
        
        is_connected = (AppState.vpn_state == "Connected")
        self.action_enable.setEnabled(not is_connected)
        self.action_disable.setEnabled(is_connected)
        
        if AppState.vpn_state == "Connected": icon = AppState.icon_connected
        elif AppState.vpn_state == "Dropped": icon = AppState.icon_dropped
        elif AppState.vpn_state == "Enabled": icon = AppState.icon_info # Transition state
        else: icon = AppState.icon_disabled
        
        self.tray_icon.setIcon(icon)

    def toggle_autoconnect(self):
        config_manager.set("auto_connect", self.action_autoconnect.isChecked())
    
    def on_enable(self):
        logger.info("Enabling VPN routing...")
        AppState.manual_disable = False
        AppState.vpn_state = "Enabling..."
        self.update_ui_state()
        
        success = SystemHandler.enable_routing(AppState.adapter_name, Config.VPN_GATEWAY, Config.DNS_SERVER)
        SystemHandler.flush_dns()
        
        if success:
            AppState.vpn_state = "Connected"
            AppState.connection_start_time = datetime.datetime.now()
            self.tray_icon.showMessage("DragonFoxVPN", "Connected successfully.", QSystemTrayIcon.Information, 2000)
        else:
            logger.error("Failed to enable routing.")
            self.on_disable()
        self.update_ui_state()
    
    def on_disable(self):
        logger.info("Disabling VPN routing...")
        AppState.manual_disable = True
        SystemHandler.disable_routing(AppState.adapter_name, Config.VPN_GATEWAY)
        SystemHandler.flush_dns()
        AppState.vpn_state = "Disabled"
        AppState.connection_start_time = None
        self.update_ui_state()
    
    def on_dashboard(self):
        StatusDashboard(self.anchor).exec_()
    
    def on_change_location(self):
        was_connected = (AppState.vpn_state == "Connected")
        if ModernLocationDialog(self.anchor).exec_() == QDialog.Accepted and was_connected:
            logger.info("Location changed, reconnecting...")
            self.on_disable()
            QTimer.singleShot(1500, self.on_enable)
        self.update_ui_state()

    def on_exit(self):
        logger.info("Exiting application...")
        self.on_disable()
        self.quit()
    
    def check_vpn_status(self):
        if not self.monitor_thread.isRunning():
            self.monitor_thread.start()
            
    def on_network_status_checked(self, vpn_active: bool, route_exists: bool):
        """Core logic for the Kill Switch and auto-recovery."""
        if vpn_active and not route_exists and not AppState.manual_disable:
            logger.info("VPN route missing but connection active, recovering...")
            self.on_enable()
        elif not vpn_active and route_exists:
            logger.warning("VPN connection dropped! Triggering kill switch.")
            SystemHandler.run_command(f"sudo ip route del default via {Config.VPN_GATEWAY} dev {AppState.adapter_name}", check=False)
            AppState.vpn_state = "Dropped"
            AppState.connection_start_time = None
            self.update_ui_state()
            self.tray_icon.showMessage("DragonFoxVPN", "CONNECTION DROPPED. Kill switch active.", QSystemTrayIcon.Warning, 5000)

# --- Entry Point ---
def main():
    # Instance check
    if platform.system() != "Windows":
        try:
            p = subprocess.run(["pgrep", "-f", "dragonfox_vpn.py"], capture_output=True, text=True)
            pids = [pid for pid in p.stdout.splitlines() if pid != str(os.getpid())]
            if pids:
                logger.warning("Another instance is running. Exiting.")
                return
        except: pass

    app = VPNTrayApp(sys.argv)
    
    def signal_handler(sig, frame):
        logger.info("Termination signal received.")
        app.on_exit()
        sys.exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    sys.exit(app.exec_())

if __name__ == "__main__":
    main()
