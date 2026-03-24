#!/bin/bash
# vpn-route-up.sh - DragonFoxVPN: OpenVPN tunnel-up routing hook
# Copyright (c) 2026 DragonFox Studios.
# https://github.com/senjinthedragon/DragonFoxVPN
# Licensed under the MIT License.
# See LICENSE for full license information.
#
# Called automatically by OpenVPN via the "up" directive in common.conf
# each time the tunnel comes up. Sets up policy routing so LAN clients
# are routed through the VPN tunnel while the Pi itself continues to use
# the direct internet connection.
#
# OpenVPN passes the tunnel device name as $1.
# Configuration is read from /etc/dragonfoxvpn/config.conf.

TUN_DEV="$1"

# --- Load configuration ---
CONFIG_FILE="/etc/dragonfoxvpn/config.conf"
[[ -f "$CONFIG_FILE" ]] && source "$CONFIG_FILE"

# --- Defaults (used if config file is missing) ---
LAN_IF="${LAN_IF:-eth0}"
LAN_NET="${LAN_NET:-192.168.1.0/24}"
PI_IP="${PI_IP:-192.168.1.2}"
ROUTE_TABLE_NAME="${ROUTE_TABLE_NAME:-vpn}"
ROUTE_TABLE_ID="${ROUTE_TABLE_ID:-100}"

# --- 1. Enable IP forwarding ---
sysctl -w net.ipv4.ip_forward=1 >/dev/null

# --- 2. Wait for tunnel device to be available ---
echo "Waiting for $TUN_DEV..."
for i in {1..10}; do
    ip link show "$TUN_DEV" &>/dev/null && { echo "$TUN_DEV is up"; break; }
    sleep 1
done

# --- 3. Ensure policy routing table exists ---
if ! grep -q "$ROUTE_TABLE_ID $ROUTE_TABLE_NAME" /etc/iproute2/rt_tables; then
    echo "$ROUTE_TABLE_ID $ROUTE_TABLE_NAME" >> /etc/iproute2/rt_tables
fi

# --- 4. Set up VPN routing table ---
ip route flush table $ROUTE_TABLE_NAME 2>/dev/null || true
ip route add default dev "$TUN_DEV" table $ROUTE_TABLE_NAME

# --- 5. Policy routing rules ---
# Pi's own traffic uses the main table (direct connection, not through VPN)
ip rule add from $PI_IP  lookup main             pref 1000 2>/dev/null || true
# All other LAN client traffic uses the VPN table
ip rule add from $LAN_NET lookup $ROUTE_TABLE_NAME pref 1001 2>/dev/null || true

# --- 6. NAT: masquerade LAN traffic going out through the tunnel ---
iptables -t nat -C POSTROUTING -s $LAN_NET -o "$TUN_DEV" -j MASQUERADE 2>/dev/null || \
iptables -t nat -A POSTROUTING -s $LAN_NET -o "$TUN_DEV" -j MASQUERADE

# --- 7. Forwarding rules ---
iptables -C FORWARD -i "$LAN_IF" -o "$TUN_DEV" -j ACCEPT 2>/dev/null || \
iptables -A FORWARD -i "$LAN_IF" -o "$TUN_DEV" -j ACCEPT

iptables -C FORWARD -i "$TUN_DEV" -o "$LAN_IF" -m state --state RELATED,ESTABLISHED -j ACCEPT 2>/dev/null || \
iptables -A FORWARD -i "$TUN_DEV" -o "$LAN_IF" -m state --state RELATED,ESTABLISHED -j ACCEPT

echo "VPN routing active: LAN clients ($LAN_NET) → $TUN_DEV. Pi ($PI_IP) uses direct connection."
