#!/usr/bin/env bash
# Builds the Windows installer (NSIS setup .exe, x64) of RAW Importer on macOS
# by cross-compiling with cargo-xwin (Tauri's documented, experimental path).
#
#   scripts/build-windows-installer.sh           # build
#   scripts/build-windows-installer.sh --setup   # install the cross tools first (one time)
#
# Output: installer/RAW Importer_<version>_x64-setup.exe (+ .sha256)
# The installer is NOT code-signed: Windows SmartScreen will warn on first start.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
TARGET=x86_64-pc-windows-msvc

if [[ "${1:-}" == "--setup" ]]; then
  command -v brew >/dev/null || { echo "Homebrew is required: https://brew.sh" >&2; exit 1; }
  brew install nsis llvm
  rustup target add "$TARGET"
  cargo install --locked cargo-xwin
  echo "Setup done. Now run: scripts/build-windows-installer.sh"
  exit 0
fi

mkdir -p logs installer
TS="$(date +%Y%m%d-%H%M%S)"
LOG="logs/build-windows-installer-$TS.log"
ln -sf "$(basename "$LOG")" logs/build-windows-installer-latest.log
exec > >(tee "$LOG") 2>&1

# Homebrew's LLVM is keg-only; cargo-xwin needs clang-cl, llvm-lib and llvm-rc from it.
for llvm in /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin; do
  [[ -d "$llvm" ]] && export PATH="$llvm:$PATH"
done
missing=()
for tool in makensis llvm-rc clang-cl cargo-xwin; do command -v "$tool" >/dev/null || missing+=("$tool"); done
rustup target list --installed 2>/dev/null | grep -qx "$TARGET" || missing+=("rust target $TARGET")
if [[ ${#missing[@]} -gt 0 ]]; then
  echo "FAIL: missing: ${missing[*]}" >&2
  echo "Run once: scripts/build-windows-installer.sh --setup" >&2
  exit 1
fi

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
echo "== RAW Importer Windows installer build $VERSION ($TS)"
echo "== tauri build --runner cargo-xwin --target $TARGET --bundles nsis"
echo "   (first run downloads the Windows SDK/CRT headers for xwin, ~700 MB)"
npm run tauri build -- --runner cargo-xwin --target "$TARGET" --bundles nsis

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
shopt -s nullglob
EXES=("$TARGET_DIR/$TARGET/release/bundle/nsis/"*-setup.exe)
[[ ${#EXES[@]} -gt 0 ]] || { echo "FAIL: no setup .exe produced" >&2; exit 1; }
for exe in "${EXES[@]}"; do
  name="$(basename "$exe")"
  out="installer/$name"
  if [[ -e "$out" ]]; then mv "$out" "$out.$TS.bak"; echo "   existing $name kept as $name.$TS.bak"; fi
  cp -p "$exe" "$out"
  ( cd installer && shasum -a 256 "$name" > "$name.sha256" )
  echo "   setup:  $ROOT/$out ($(du -h "$out" | cut -f1))"
  echo "   sha256: $(cut -d' ' -f1 "$out.sha256")"
done

echo
echo "PASS: Windows installer written to $ROOT/installer/"
echo "NOTE: unsigned. On first start Windows SmartScreen shows 'Windows protected your PC'"
echo "      -> 'More info' -> 'Run anyway'. Installs per user, no admin rights needed."
open "$ROOT/installer" 2>/dev/null || true
echo "Log: $LOG"
