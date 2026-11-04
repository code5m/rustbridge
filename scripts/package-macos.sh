#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$#" -eq 0 || "$1" != "--skip-build" ]]; then cargo build --release; fi
VERSION="$(sed -n '/^version = /{s/^version = "//;s/"$//;p;q;}' Cargo.toml)"
mkdir -p dist/RustBridge.app/Contents/MacOS dist/RustBridge.app/Contents/Resources
install -m 755 target/release/rustbridge dist/RustBridge.app/Contents/MacOS/rustbridge
install -m 644 LICENSE dist/RustBridge.app/Contents/Resources/LICENSE
install -m 644 README.md dist/RustBridge.app/Contents/Resources/README.md
cat > dist/RustBridge.app/Contents/Info.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>rustbridge</string>
<key>CFBundleIdentifier</key><string>dev.rustbridge.ide</string>
<key>CFBundleName</key><string>RustBridge</string>
<key>CFBundleDisplayName</key><string>RustBridge</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
ARCH="$(uname -m)"
rm -f "dist/rustbridge_$VERSION-macos-$ARCH.zip"
ditto -c -k --sequesterRsrc --keepParent dist/RustBridge.app \
  "dist/rustbridge_$VERSION-macos-$ARCH.zip"
echo "Unsigned/unnotarized macOS app bundle zipped."
