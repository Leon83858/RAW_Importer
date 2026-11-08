#!/usr/bin/env bash
# Builds the macOS installer (.dmg) for RAW Importer 3.0 as a release build.
#
#   scripts/build-mac-installer.sh              # Apple Silicon (arm64), default
#   scripts/build-mac-installer.sh --universal  # arm64 + x86_64 (Intel) in one app
#
# The result is NOT code-signed and NOT notarized (no Apple Developer ID yet,
# see DECISIONS.md O-6). Gatekeeper will warn on first launch.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script must run on macOS (needs hdiutil / Xcode tools)." >&2
  exit 1
fi

UNIVERSAL=0
for arg in "$@"; do
  case "$arg" in
    --universal) UNIVERSAL=1 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

mkdir -p logs
TS="$(date +%Y%m%d-%H%M%S)"
LOG="logs/build-mac-installer-$TS.log"
ln -sf "$(basename "$LOG")" logs/build-mac-installer-latest.log
exec > >(tee "$LOG") 2>&1

echo "== RAW Importer macOS installer build ($TS)"
echo "   macOS $(sw_vers -productVersion) $(uname -m), $(rustc --version), node $(node --version)"

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
ARGS=(--bundles app)
if [[ $UNIVERSAL -eq 1 ]]; then
  echo "== Ensuring Rust targets for universal build"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin
  ARGS+=(--target universal-apple-darwin)
  BUNDLE_DIR="$TARGET_DIR/universal-apple-darwin/release/bundle"
  ARCH=universal
else
  BUNDLE_DIR="$TARGET_DIR/release/bundle"
  ARCH="$(uname -m)"
fi

if [[ ! -d node_modules ]]; then
  echo "== npm ci"
  npm ci
fi

# Only the .app comes from Tauri. The .dmg is built below with plain hdiutil:
# Tauri's dmg step styles the window via Finder AppleScript, which fails
# intermittently on this macOS (error -10006) and leaves the image mounted.
echo "== tauri build ${ARGS[*]}"
npm run tauri build -- "${ARGS[@]}"

APP="$BUNDLE_DIR/macos/RAW Importer.app"
[[ -d "$APP" ]] || { echo "FAIL: $APP not found" >&2; exit 1; }

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
NAME="RAW Importer_${VERSION}_${ARCH}.dmg"
OUT_DIR="$ROOT/installer"
OUT="$OUT_DIR/$NAME"
mkdir -p "$OUT_DIR"
if [[ -e "$OUT" ]]; then
  BAK="$OUT.$TS.bak"
  echo "== Existing $NAME kept as $(basename "$BAK")"
  mv "$OUT" "$BAK"
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/RAW Importer.app"
ln -s /Applications "$STAGE/Applications"

echo "== hdiutil create $NAME"
hdiutil create -volname "RAW Importer" -srcfolder "$STAGE" -fs HFS+ \
  -format UDZO -imagekey zlib-level=9 -ov "$OUT"

echo "== Verifying $NAME"
hdiutil verify "$OUT"
( cd "$OUT_DIR" && shasum -a 256 "$NAME" > "$NAME.sha256" )
echo "   size:   $(du -h "$OUT" | cut -f1)"
echo "   sha256: $(cut -d' ' -f1 "$OUT.sha256")"

if [[ -d "$APP" ]]; then
  echo "== Architectures: $(lipo -archs "$APP/Contents/MacOS/"* 2>/dev/null | head -1)"
  echo "== Signature status:"
  codesign -dv "$APP" 2>&1 | sed 's/^/   /' || true
fi

echo
echo "PASS: installer written to $OUT"
open -R "$OUT" || true
echo "NOTE: unsigned / not notarized. On first launch: right-click the app -> Open,"
echo "      or: xattr -dr com.apple.quarantine \"/Applications/RAW Importer.app\""
echo "Log: $LOG"
