#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
command -v docker >/dev/null || { echo "FAIL: Docker is required to build the Arch package" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "FAIL: Docker is not running" >&2; exit 1; }

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCHIVE="$ROOT/installer/RAW-Importer_${VERSION}_linux_x86_64.zip"
[[ -f "$ARCHIVE" ]] || { echo "FAIL: Linux ZIP not found: $ARCHIVE" >&2; exit 1; }

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
unzip -q "$ARCHIVE" -d "$STAGE"
SOURCE="$STAGE/RAW-Importer_${VERSION}_linux_x86_64"
BUILD="$STAGE/build"
mkdir -p "$BUILD"
install -m 755 "$SOURCE/raw-importer-app" "$BUILD/raw-importer-app"
sed 's#@EXEC@#raw-importer-app#' "$SOURCE/raw-importer.desktop" > "$BUILD/raw-importer.desktop"
for size in 32 64 128 256 512; do
  install -m 644 "$SOURCE/icons/${size}x${size}.png" "$BUILD/${size}x${size}.png"
done
LICENSE_FILE="$ROOT/LICENSE"
[[ -f "$LICENSE_FILE" ]] || LICENSE_FILE="$ROOT/legacy/csharp/LICENSE"
install -m 644 "$LICENSE_FILE" "$BUILD/LICENSE"
HOST_UID="$(id -u)"
HOST_GID="$(id -g)"

cat > "$BUILD/PKGBUILD" <<PKGBUILD
pkgname=raw-importer
pkgver=${VERSION}
pkgrel=1
pkgdesc='Desktop application for importing camera RAW and image files'
arch=('x86_64')
url='https://github.com/Leon83858/RAW_Importer'
license=('GPL-3.0-only')
depends=('glibc' 'gtk3' 'webkit2gtk-4.1' 'libayatana-appindicator')
source=('raw-importer-app' 'raw-importer.desktop' '32x32.png' '64x64.png' '128x128.png' '256x256.png' '512x512.png' 'LICENSE')
sha256sums=('SKIP' 'SKIP' 'SKIP' 'SKIP' 'SKIP' 'SKIP' 'SKIP' 'SKIP')

package() {
  install -Dm755 "\$srcdir/raw-importer-app" "\$pkgdir/usr/bin/raw-importer-app"
  install -Dm644 "\$srcdir/raw-importer.desktop" "\$pkgdir/usr/share/applications/raw-importer.desktop"
  install -Dm644 "\$srcdir/LICENSE" "\$pkgdir/usr/share/licenses/raw-importer/LICENSE"
  for size in 32 64 128 256 512; do
    install -Dm644 "\$srcdir/\${size}x\${size}.png" \\
      "\$pkgdir/usr/share/icons/hicolor/\${size}x\${size}/apps/raw-importer.png"
  done
}
PKGBUILD

docker run --rm --platform linux/amd64 \
  -e HOST_UID="$HOST_UID" -e HOST_GID="$HOST_GID" -v "$BUILD:/work" \
  archlinux:base-devel \
  bash -lc 'set -euo pipefail; pacman -Syu --noconfirm --needed base-devel zstd; groupadd --gid "$HOST_GID" builder || true; useradd --create-home --uid "$HOST_UID" --gid "$HOST_GID" builder; su builder -s /bin/bash -c "cd /work && makepkg --nodeps --noconfirm --cleanbuild --force"'

mkdir -p "$ROOT/installer"
OUTPUT="$ROOT/installer/raw-importer-${VERSION}-1-x86_64.pkg.tar.zst"
cp -p "$BUILD/raw-importer-${VERSION}-1-x86_64.pkg.tar.zst" "$OUTPUT"
( cd "$ROOT/installer" && sha256sum "$(basename "$OUTPUT")" > "$(basename "$OUTPUT").sha256" )
echo "PASS: Arch package written to $OUTPUT"