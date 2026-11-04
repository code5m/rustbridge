#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" != "--skip-build" ]]; then cargo build --release; fi
command -v dpkg-deb >/dev/null
VERSION="$(sed -n '/^version = /{s/^version = "//;s/"$//;p;q;}' Cargo.toml)"
ARCH="$(dpkg --print-architecture)"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
install -D -m 0755 target/release/rustbridge "$STAGE/usr/bin/rustbridge"
install -D -m 0644 README.md "$STAGE/usr/share/doc/rustbridge/README.md"
install -D -m 0644 LICENSE "$STAGE/usr/share/doc/rustbridge/LICENSE"
mkdir -p "$STAGE/DEBIAN" "$STAGE/usr/share/applications" dist
cat > "$STAGE/DEBIAN/control" <<EOF
Package: rustbridge
Version: $VERSION
Section: devel
Priority: optional
Architecture: $ARCH
Maintainer: RustBridge Contributors <noreply@github.com>
Depends: libx11-6, libxkbcommon0, libxkbcommon-x11-0, libgl1, libwayland-client0
Description: Native lightweight Rust learner for Java 8 developers
 A Rust/egui desktop source reader with Java conceptual comparison.
EOF
cat > "$STAGE/usr/share/applications/rustbridge.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=RustBridge
GenericName=Java-to-Rust Learning Editor
Comment=Explore and compare real Rust code with Java 8 concepts
Exec=rustbridge %f
Terminal=false
Categories=Development;IDE;Education;
MimeType=text/x-rust;
EOF
PACKAGE="dist/rustbridge_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$PACKAGE"
dpkg-deb --info "$PACKAGE"
echo "PACKAGE=$PACKAGE"
