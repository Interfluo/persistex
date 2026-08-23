#!/usr/bin/env bash
# Build a .deb and an AppImage. Run on Linux (or in CI).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
mkdir -p "$DIST"

echo "==> building"
cargo build --release -p persistex
BIN="$ROOT/target/release/persistex"

DESKTOP='[Desktop Entry]
Type=Application
Name=persistex
Comment=Multisine excitation designer
Exec=persistex
Icon=persistex
Categories=Science;Engineering;
Terminal=false'

# ---------------------------------------------------------------- .deb
echo "==> building deb"
PKG="$(mktemp -d)/persistex_${VERSION}_${ARCH}"
mkdir -p "$PKG/DEBIAN" "$PKG/usr/bin" "$PKG/usr/share/applications" \
         "$PKG/usr/share/icons/hicolor/256x256/apps"
install -m755 "$BIN" "$PKG/usr/bin/persistex"
cp "$ROOT/packaging/assets/icon_256.png" \
   "$PKG/usr/share/icons/hicolor/256x256/apps/persistex.png"
echo "$DESKTOP" > "$PKG/usr/share/applications/persistex.desktop"
cat > "$PKG/DEBIAN/control" <<CONTROL
Package: persistex
Version: $VERSION
Section: science
Priority: optional
Architecture: $ARCH
Maintainer: persistex
Depends: libc6, libgl1, libx11-6
Description: Multisine excitation designer
 Orthogonal phase-optimised multisine design for system identification.
CONTROL
dpkg-deb --build --root-owner-group "$PKG" "$DIST/persistex-${VERSION}-linux-${ARCH}.deb"

# ------------------------------------------------------------ AppImage
if command -v appimagetool >/dev/null 2>&1; then
  echo "==> building AppImage"
  APPDIR="$(mktemp -d)/persistex.AppDir"
  mkdir -p "$APPDIR/usr/bin"
  install -m755 "$BIN" "$APPDIR/usr/bin/persistex"
  cp "$ROOT/packaging/assets/icon_256.png" "$APPDIR/persistex.png"
  echo "$DESKTOP" > "$APPDIR/persistex.desktop"
  printf '#!/bin/sh\nexec "$(dirname "$0")/usr/bin/persistex" "$@"\n' > "$APPDIR/AppRun"
  chmod +x "$APPDIR/AppRun"
  ARCH=x86_64 appimagetool "$APPDIR" "$DIST/persistex-${VERSION}-linux-x86_64.AppImage"
else
  echo "==> skipping AppImage (appimagetool not on PATH)"
fi

ls -lh "$DIST"
