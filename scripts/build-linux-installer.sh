#!/usr/bin/env bash
# Builds the Linux package of RAW Importer: a .zip with the app and install.sh.
#
#   scripts/build-linux-installer.sh                 # x86_64 (typical Linux PC)
#   scripts/build-linux-installer.sh --arch aarch64  # ARM64 (e.g. Raspberry Pi 5)
#   scripts/build-linux-installer.sh --arch all      # both
#
# On macOS the app is compiled inside a Docker container (Docker Desktop or
# OrbStack must be running). On Linux it is compiled natively when the host
# architecture matches and WebKitGTK 4.1 dev packages are installed.
# Output: installer/RAW-Importer_<version>_linux_<arch>.zip (+ .sha256)
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

ARCHES=(x86_64)
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch)
      case "${2:-}" in
        x86_64|amd64) ARCHES=(x86_64) ;;
        aarch64|arm64) ARCHES=(aarch64) ;;
        all) ARCHES=(x86_64 aarch64) ;;
        *) echo "Unknown arch: ${2:-}" >&2; exit 2 ;;
      esac
      shift 2 ;;
    -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
  esac
done

mkdir -p logs installer
TS="$(date +%Y%m%d-%H%M%S)"
LOG="logs/build-linux-installer-$TS.log"
ln -sf "$(basename "$LOG")" logs/build-linux-installer-latest.log
exec > >(tee "$LOG") 2>&1

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
echo "== RAW Importer Linux package build $VERSION ($TS), arches: ${ARCHES[*]}"

host_arch() { case "$(uname -m)" in x86_64|amd64) echo x86_64 ;; aarch64|arm64) echo aarch64 ;; *) uname -m ;; esac; }

build_binary() {   # $1 = arch; prints nothing, leaves binary at target-linux/<arch>/release/raw-importer-app
  local arch="$1" platform
  [[ $arch == x86_64 ]] && platform=linux/amd64 || platform=linux/arm64
  local tdir="target-linux/$arch"

  if [[ "$(uname -s)" == Linux && "$(host_arch)" == "$arch" ]]; then
    echo "== Native Linux build ($arch)"
    CARGO_TARGET_DIR="$ROOT/$tdir" npm run tauri build -- --no-bundle
    return
  fi

  command -v docker >/dev/null || { echo "FAIL: docker not found. Install Docker Desktop or OrbStack and start it." >&2; exit 1; }
  docker info >/dev/null 2>&1 || { echo "FAIL: Docker is installed but not running. Start Docker Desktop/OrbStack." >&2; exit 1; }

  local image="raw-importer-linux-build:$arch"
  echo "== Docker image $image ($platform)"
  docker build --platform "$platform" -t "$image" -f packaging/linux/Dockerfile packaging/linux

  echo "== Compiling in container ($platform) – the first run takes a while"
  docker run --rm --platform "$platform" \
    -v "$ROOT":/work \
    -v "raw-importer-node-modules-$arch":/work/node_modules \
    -v "raw-importer-cargo-registry-$arch":/opt/cargo/registry \
    -e CARGO_TARGET_DIR="/work/$tdir" \
    -w /work "$image" \
    bash -euo pipefail -c 'npm ci --no-audit --no-fund && npm run tauri build -- --no-bundle'
}

package() {   # $1 = arch
  local arch="$1"
  local bin="$ROOT/target-linux/$arch/release/raw-importer-app"
  [[ -f "$bin" ]] || { echo "FAIL: $bin not found" >&2; exit 1; }
  if command -v file >/dev/null; then echo "   binary: $(file -b "$bin" | cut -c1-90)"; fi

  local name="RAW-Importer_${VERSION}_linux_${arch}"
  local stage; stage="$(mktemp -d)"
  local dir="$stage/$name"
  mkdir -p "$dir/icons"
  cp "$bin" "$dir/raw-importer-app"; chmod 755 "$dir/raw-importer-app"
  cp packaging/linux/install.sh "$dir/"; chmod 755 "$dir/install.sh"
  cp packaging/linux/raw-importer.desktop "$dir/"
  cat > "$dir/README.txt" <<'README'
RAW Importer for Linux

Install for the current user with ./install.sh. See install.sh --help for other options.
Requires WebKitGTK 4.1 and GTK 3. On Debian or Ubuntu: install libwebkit2gtk-4.1-0 and libgtk-3-0.
README
  echo "$arch" > "$dir/ARCH"
  echo "$VERSION" > "$dir/VERSION"
  cp src-tauri/icons/32x32.png        "$dir/icons/32x32.png"
  cp src-tauri/icons/64x64.png        "$dir/icons/64x64.png"
  cp src-tauri/icons/128x128.png      "$dir/icons/128x128.png"
  cp src-tauri/icons/128x128@2x.png   "$dir/icons/256x256.png"
  cp src-tauri/icons/icon.png         "$dir/icons/512x512.png"

  local out="$ROOT/installer/$name.zip"
  if [[ -e "$out" ]]; then mv "$out" "$out.$TS.bak"; echo "   existing zip kept as $(basename "$out").$TS.bak"; fi
  ( cd "$stage" && zip -qrX "$out" "$name" )
  rm -rf "$stage"
  ( cd installer && shasum -a 256 "$name.zip" > "$name.zip.sha256" 2>/dev/null || sha256sum "$name.zip" > "$name.zip.sha256" )
  echo "   zip:    $out ($(du -h "$out" | cut -f1))"
  echo "   sha256: $(cut -d' ' -f1 "$out.sha256")"
}

for arch in "${ARCHES[@]}"; do
  build_binary "$arch"
  echo "== Packaging $arch"
  package "$arch"
done

echo
echo "PASS: Linux package(s) written to $ROOT/installer/"
[[ "$(uname -s)" == Darwin ]] && open "$ROOT/installer" || true
echo "Log: $LOG"
