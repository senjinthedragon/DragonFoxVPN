#!/bin/bash
# switch-openvpn.sh
# Switches the active OpenVPN client config by updating a symlink and restarting the service.
#
# Usage:
#   switch-openvpn.sh <config-basename>   -- switch to a specific config
#   switch-openvpn.sh --refresh           -- rebuild locations.txt from disk
#
# Must be run as root (via sudo from the web UI).
#
# Setup:
#   1. Place your .ovpn files in EXPRESS_DIR.
#   2. Ensure OPENVPN_SERVICE matches your systemd unit name.
#   3. Set CONF_OVERLAY to your shared credentials/options file, or leave empty ("").
#   4. Copy to /usr/local/bin/ and chmod +x.
#   5. Allow the web server to run it as root without a password:
#        echo "www-data ALL=(root) NOPASSWD: /usr/local/bin/switch-openvpn.sh" \
#          | sudo tee /etc/sudoers.d/switch-openvpn

set -euo pipefail

# --- Load configuration ---
CONFIG_FILE="/etc/dragonfoxvpn/config.conf"
[[ -f "$CONFIG_FILE" ]] && source "$CONFIG_FILE"

# --- Defaults (used if config file is missing) ---
EXPRESS_DIR="${EXPRESS_DIR:-/etc/openvpn/client/configs}"
CLIENT_LINK="${CLIENT_LINK:-/etc/openvpn/client/active.conf}"
OPENVPN_SERVICE="${OPENVPN_SERVICE:-openvpn-client@active}"
CONF_OVERLAY="${CONF_OVERLAY:-config /etc/openvpn/client/common.conf}"

LOCATIONS_FILE="/etc/openvpn/locations.txt"

TARGET="${1:-}"

if [[ "$TARGET" == "--refresh" ]]; then
    LOCATIONS=()
    for f in "$EXPRESS_DIR"/*.ovpn; do
        [[ -f "$f" ]] && LOCATIONS+=("$(basename "$f" .ovpn)")
    done
    printf "%s\n" "${LOCATIONS[@]}" > "$LOCATIONS_FILE"
    echo "Locations refreshed: ${#LOCATIONS[@]} configs found."
    exit 0
fi

if [[ -z "$TARGET" ]]; then
    echo "Usage: $0 <location> | --refresh" >&2
    exit 1
fi

CONF_FILE="$EXPRESS_DIR/$TARGET.ovpn"

if [[ ! -f "$CONF_FILE" ]]; then
    echo "Config file not found: $CONF_FILE" >&2
    exit 1
fi

# Patch the .ovpn file if not already patched:
#   - Inject the overlay config directive
#   - Comment out redirect-gateway and dhcp-option DNS (the tray app manages
#     routing and DNS directly at the OS level)
if [[ -n "$CONF_OVERLAY" ]] && ! grep -Fxq "$CONF_OVERLAY" "$CONF_FILE"; then
    TMPFILE=$(mktemp)

    # Helper: copy file to TMPFILE with redirect-gateway/DNS lines commented out
    patch_lines() {
        while IFS= read -r line; do
            if [[ "$line" =~ ^redirect-gateway ]] || [[ "$line" =~ ^dhcp-option[[:space:]]+DNS ]]; then
                [[ "$line" =~ ^# ]] && echo "$line" || echo "# $line"
            else
                echo "$line"
            fi
        done < "$CONF_FILE"
    }

    if grep -q "^<cert>" "$CONF_FILE"; then
        # Inline certs: insert overlay just before the <cert> block
        OVERLAY_ADDED=false
        while IFS= read -r line; do
            if [[ "$line" =~ ^\<cert\> ]] && [[ "$OVERLAY_ADDED" == false ]]; then
                echo "$CONF_OVERLAY"
                OVERLAY_ADDED=true
            fi
            if [[ "$line" =~ ^redirect-gateway ]] || [[ "$line" =~ ^dhcp-option[[:space:]]+DNS ]]; then
                [[ "$line" =~ ^# ]] && echo "$line" || echo "# $line"
            else
                echo "$line"
            fi
        done < "$CONF_FILE" > "$TMPFILE"
    else
        # External certs: prepend overlay at the top of the file
        { echo "$CONF_OVERLAY"; patch_lines; } > "$TMPFILE"
    fi

    mv "$TMPFILE" "$CONF_FILE"
fi

# Update symlink and restart service
ln -sf "$CONF_FILE" "$CLIENT_LINK"
systemctl restart "$OPENVPN_SERVICE"

# Regenerate locations list
LOCATIONS=()
for f in "$EXPRESS_DIR"/*.ovpn; do
    [[ -f "$f" ]] && LOCATIONS+=("$(basename "$f" .ovpn)")
done
printf "%s\n" "${LOCATIONS[@]}" > "$LOCATIONS_FILE"

echo "Switched to $TARGET successfully."
