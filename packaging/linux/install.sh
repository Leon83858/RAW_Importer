#!/usr/bin/env bash
# RAW Importer – Linux installer
#
#   ./install.sh              install for the current user (~/.local), no root needed
#   ./install.sh --system     install for all users (/opt + /usr/local), uses sudo
#   ./install.sh --uninstall  remove (add --system for a system-wide install)
set -euo pipefail

APP_ID="raw-importer"
APP_NAME="RAW Importer"
BIN_NAME="raw-importer-app"
HERE="$(cd "$(dirname "$0")" && pwd)"

MODE=user
ACTION=install
for arg in "$@"; do
  case "$arg" in
    --system) MODE=system ;;
    --user) MODE=user ;;
    --uninstall) ACTION=uninstall ;;
    -h|--help) sed -n '2,7p' "$0"; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

if [[ $MODE == system ]]; then
  LIB_DIR="/opt/$APP_ID"
  BIN_DIR="/usr/local/bin"
  SHARE_DIR="/usr/local/share"
  if [[ $EUID -ne 0 ]]; then SUDO="sudo"; else SUDO=""; fi
else
  LIB_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/$APP_ID"
  BIN_DIR="$HOME/.local/bin"
  SHARE_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
  SUDO=""
fi
DESKTOP_FILE="$SHARE_DIR/applications/$APP_ID.desktop"
ICON_SIZES=(32 64 128 256 512)

refresh_caches() {
  command -v update-desktop-database >/dev/null 2>&1 && $SUDO update-desktop-database -q "$SHARE_DIR/applications" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 && $SUDO gtk-update-icon-cache -q -t "$SHARE_DIR/icons/hicolor" 2>/dev/null || true
}

if [[ $ACTION == uninstall ]]; then
  echo "Removing $APP_NAME ($MODE install) ..."
  $SUDO rm -f "$BIN_DIR/$APP_ID" "$DESKTOP_FILE"
  for s in "${ICON_SIZES[@]}"; do $SUDO rm -f "$SHARE_DIR/icons/hicolor/${s}x${s}/apps/$APP_ID.png"; done
  $SUDO rm -rf "$LIB_DIR"
  refresh_caches
  echo "Done. Settings and import history were kept in ~/.local/share/com.projectbuilder.rawimportercs"
  exit 0
fi

# --- checks ---------------------------------------------------------------
[[ "$(uname -s)" == Linux ]] || { echo "This installer is for Linux only." >&2; exit 1; }
[[ -f "$HERE/$BIN_NAME" ]] || { echo "Missing $BIN_NAME next to install.sh." >&2; exit 1; }

PKG_ARCH="$(cat "$HERE/ARCH" 2>/dev/null || echo unknown)"
case "$(uname -m)" in
  x86_64|amd64) HOST_ARCH=x86_64 ;;
  aarch64|arm64) HOST_ARCH=aarch64 ;;
  *) HOST_ARCH="$(uname -m)" ;;
esac
if [[ "$PKG_ARCH" != unknown && "$PKG_ARCH" != "$HOST_ARCH" ]]; then
  echo "This package is built for $PKG_ARCH, but this computer is $HOST_ARCH." >&2
  exit 1
fi

if command -v ldd >/dev/null 2>&1; then
  MISSING="$(ldd "$HERE/$BIN_NAME" 2>/dev/null | awk '/not found/ {print $1}' || true)"
  if [[ -n "$MISSING" ]]; then
    echo "Missing system libraries:"; echo "$MISSING" | sed 's/^/  - /'
    echo
    echo "Install the runtime dependencies first, e.g.:"
    echo "  Debian/Ubuntu/Mint:  sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0"
    echo "  Fedora:              sudo dnf install webkit2gtk4.1 gtk3"
    echo "  Arch/Manjaro:        sudo pacman -S webkit2gtk-4.1 gtk3"
    echo "  openSUSE:            sudo zypper install libwebkit2gtk-4_1-0 gtk3"
    echo "Then run ./install.sh again."
    exit 1
  fi
fi

# --- install --------------------------------------------------------------
echo "Installing $APP_NAME $(cat "$HERE/VERSION" 2>/dev/null) ($MODE install) ..."
$SUDO mkdir -p "$LIB_DIR" "$BIN_DIR" "$SHARE_DIR/applications"
$SUDO install -m 755 "$HERE/$BIN_NAME" "$LIB_DIR/$BIN_NAME"
$SUDO ln -sfn "$LIB_DIR/$BIN_NAME" "$BIN_DIR/$APP_ID"

for s in "${ICON_SIZES[@]}"; do
  src="$HERE/icons/${s}x${s}.png"
  [[ -f "$src" ]] || continue
  $SUDO mkdir -p "$SHARE_DIR/icons/hicolor/${s}x${s}/apps"
  $SUDO install -m 644 "$src" "$SHARE_DIR/icons/hicolor/${s}x${s}/apps/$APP_ID.png"
done

TMP_DESKTOP="$(mktemp)"
sed "s#@EXEC@#$LIB_DIR/$BIN_NAME#" "$HERE/$APP_ID.desktop" > "$TMP_DESKTOP"
$SUDO install -m 644 "$TMP_DESKTOP" "$DESKTOP_FILE"
rm -f "$TMP_DESKTOP"
refresh_caches

echo
echo "$APP_NAME installed."
echo "  Start it from your application menu or with: $APP_ID"
if [[ $MODE == user && ":$PATH:" != *":$BIN_DIR:"* ]]; then
  echo "  Note: $BIN_DIR is not in your PATH; the menu entry works regardless."
fi
echo "  Uninstall: ./install.sh --uninstall$([[ $MODE == system ]] && echo ' --system')"
