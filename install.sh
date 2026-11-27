#!/bin/sh
# Installs Banshee for the current user: binary, icon, menu entry and autostart (tray).
set -eu
cd "$(dirname "$0")"
BIN="$HOME/.local/bin/banshee"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.config/autostart" \
         "$HOME/.local/share/icons/hicolor/128x128/apps"
install -m 755 target/release/banshee "$BIN"
# leftovers from before the rename (the app used to be called ghostdeck)
pkill -x ghostdeck 2>/dev/null || true
rm -f "$HOME/.local/bin/ghostdeck" "$HOME/.local/share/applications/ghostdeck.desktop" \
      "$HOME/.config/autostart/ghostdeck.desktop" "$HOME/.local/share/icons/hicolor/128x128/apps/ghostdeck.png"
install -m 644 icons/icon.png "$HOME/.local/share/icons/hicolor/128x128/apps/banshee.png"

entry() {
cat <<EOF
[Desktop Entry]
Type=Application
Name=Banshee
Comment=Perfiles, ventiladores y batería de la MSI Prestige 16
Exec=$BIN $1
Icon=$HOME/.local/share/icons/hicolor/128x128/apps/banshee.png
Categories=Settings;HardwareSettings;
Terminal=false
StartupWMClass=Banshee
EOF
}
entry "" > "$HOME/.local/share/applications/banshee.desktop"
{ entry "--tray"; echo "X-GNOME-Autostart-enabled=true"; } > "$HOME/.config/autostart/banshee.desktop"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Instalado en $BIN"
