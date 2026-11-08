#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
command -v dpkg-deb >/dev/null || { echo "FAIL: dpkg-deb is required" >&2; exit 1; }

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCHIVE="$ROOT/installer/RAW-Importer_${VERSION}_linux_x86_64.zip"
[[ -f "$ARCHIVE" ]] || { echo "FAIL: Linux ZIP not found: $ARCHIVE" >&2; exit 1; }

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
unzip -q "$ARCHIVE" -d "$STAGE"
SOURCE="$STAGE/RAW-Importer_${VERSION}_linux_x86_64"
PACKAGE="$STAGE/package"

mkdir -p \
  "$PACKAGE/DEBIAN" \
  "$PACKAGE/usr/bin" \
  "$PACKAGE/usr/share/applications" \
  "$PACKAGE/usr/share/doc/raw-importer" \
  "$PACKAGE/usr/share/icons/hicolor"

install -m 755 "$SOURCE/raw-importer-app" "$PACKAGE/usr/bin/raw-importer-app"
sed 's#@EXEC@#raw-importer-app#' "$SOURCE/raw-importer.desktop" \
  > "$PACKAGE/usr/share/applications/raw-importer.desktop"
for size in 32 64 128 256 512; do
  mkdir -p "$PACKAGE/usr/share/icons/hicolor/${size}x${size}/apps"
  install -m 644 "$SOURCE/icons/${size}x${size}.png" \
    "$PACKAGE/usr/share/icons/hicolor/${size}x${size}/apps/raw-importer.png"
done

LICENSE_FILE="$ROOT/LICENSE"
[[ -f "$LICENSE_FILE" ]] || LICENSE_FILE="$ROOT/legacy/csharp/LICENSE"
install -m 644 "$LICENSE_FILE" "$PACKAGE/usr/share/doc/raw-importer/copyright"
cat > "$PACKAGE/DEBIAN/control" <<CONTROL
Package: raw-importer
Version: ${VERSION}
Section: graphics
Priority: optional
Architecture: amd64
Maintainer: The_Lom <The_Lom@users.noreply.github.com>
Depends: libc6 (>= 2.35), libgtk-3-0, libwebkit2gtk-4.1-0, libayatana-appindicator3-1, libxdo3
Description: Desktop application for importing camera RAW and image files
 Organises copied files into date-based folders without modifying source files.
CONTROL

mkdir -p "$ROOT/installer"
OUTPUT="$ROOT/installer/raw-importer_${VERSION}_amd64.deb"
if [[ -e "$OUTPUT" ]]; then
  mv "$OUTPUT" "$OUTPUT.$(date +%Y%m%d-%H%M%S).bak"
fi
dpkg-deb --build --root-owner-group "$PACKAGE" "$OUTPUT"
( cd "$ROOT/installer" && sha256sum "$(basename "$OUTPUT")" > "$(basename "$OUTPUT").sha256" )
dpkg-deb --info "$OUTPUT"
echo "PASS: Debian package written to $OUTPUT"