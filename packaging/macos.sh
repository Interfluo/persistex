#!/usr/bin/env bash
# Build persistex.app and a drag-to-Applications .dmg.
#
# Produces a universal binary when both Apple targets are installed, otherwise a
# native-only build. Unsigned by default -- see SIGNING below.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/persistex.app"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"

rm -rf "$APP" "$DIST/persistex-$VERSION-macos.dmg"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

echo "==> building"
targets=()
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  if rustup target list --installed | grep -qx "$t"; then
    cargo build --release -p persistex --target "$t"
    targets+=("$ROOT/target/$t/release/persistex")
  else
    echo "    skipping $t (not installed: rustup target add $t)"
  fi
done

if [ "${#targets[@]}" -eq 0 ]; then
  cargo build --release -p persistex
  cp "$ROOT/target/release/persistex" "$APP/Contents/MacOS/persistex"
elif [ "${#targets[@]}" -eq 1 ]; then
  cp "${targets[0]}" "$APP/Contents/MacOS/persistex"
else
  lipo -create -output "$APP/Contents/MacOS/persistex" "${targets[@]}"
fi
chmod +x "$APP/Contents/MacOS/persistex"
cp "$ROOT/packaging/assets/persistex.icns" "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>persistex</string>
  <key>CFBundleDisplayName</key><string>persistex</string>
  <key>CFBundleIdentifier</key><string>io.persistex.designer</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>persistex</string>
  <key>CFBundleIconFile</key><string>persistex.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>10.15</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST

# SIGNING: with a Developer ID in the keychain, set IDENTITY to sign and notarise.
# Without it the app is ad-hoc signed, and recipients must right-click > Open once.
if [ -n "${IDENTITY:-}" ]; then
  echo "==> signing as $IDENTITY"
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
  if [ -n "${NOTARY_PROFILE:-}" ]; then
    ditto -c -k --keepParent "$APP" "$DIST/notarize.zip"
    xcrun notarytool submit "$DIST/notarize.zip" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    rm -f "$DIST/notarize.zip"
  fi
else
  codesign --force --deep --sign - "$APP" 2>/dev/null || true
  echo "==> ad-hoc signed (set IDENTITY to sign properly)"
fi

echo "==> building dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "persistex" -srcfolder "$STAGE" -ov -quiet \
  -format UDZO "$DIST/persistex-$VERSION-macos.dmg"
rm -rf "$STAGE"

echo "==> $DIST/persistex-$VERSION-macos.dmg"
lipo -archs "$APP/Contents/MacOS/persistex" 2>/dev/null | sed 's/^/    archs: /' || true
