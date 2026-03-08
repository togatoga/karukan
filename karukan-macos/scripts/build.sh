#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ROOT_DIR="$(dirname "$PROJECT_DIR")"
SWIFT_DIR="$PROJECT_DIR/swift/KarukanInputMethod"
INCLUDE_DIR="$PROJECT_DIR/include"
BUILD_DIR="$PROJECT_DIR/build"
APP_NAME="Karukan.app"
APP_DIR="$BUILD_DIR/$APP_NAME"

# Detect architecture
ARCH="${1:-$(uname -m)}"
if [ "$ARCH" = "arm64" ]; then
    TARGET="aarch64-apple-darwin"
elif [ "$ARCH" = "x86_64" ]; then
    TARGET="x86_64-apple-darwin"
else
    echo "Unsupported architecture: $ARCH"
    exit 1
fi

echo "==> Building karukan-macos static library (target: $TARGET)..."
cargo build -p karukan-macos --release --target "$TARGET"

STATIC_LIB="$ROOT_DIR/target/$TARGET/release/libkarukan_macos.a"
if [ ! -f "$STATIC_LIB" ]; then
    echo "ERROR: Static library not found at $STATIC_LIB"
    exit 1
fi

echo "==> Compiling Swift sources..."
mkdir -p "$BUILD_DIR"

SWIFT_SOURCES=(
    "$SWIFT_DIR/main.swift"
    "$SWIFT_DIR/KarukanInputController.swift"
    "$SWIFT_DIR/KarukanCandidateWindow.swift"
    "$SWIFT_DIR/SettingsView.swift"
)

swiftc \
    -target "${ARCH}-apple-macosx14.0" \
    -O \
    -import-objc-header "$INCLUDE_DIR/karukan_macos.h" \
    -L "$(dirname "$STATIC_LIB")" \
    -lkarukan_macos \
    -lc++ \
    -framework Cocoa \
    -framework InputMethodKit \
    -framework SwiftUI \
    -framework Accelerate \
    -framework Metal \
    -framework MetalKit \
    -o "$BUILD_DIR/KarukanInputMethod" \
    "${SWIFT_SOURCES[@]}"

echo "==> Assembling .app bundle..."
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

cp "$BUILD_DIR/KarukanInputMethod" "$APP_DIR/Contents/MacOS/"
cp "$SWIFT_DIR/Info.plist" "$APP_DIR/Contents/"
echo -n "APPL????" > "$APP_DIR/Contents/PkgInfo"

# Ad-hoc code sign
echo "==> Code signing (ad-hoc)..."
codesign --force --sign - --entitlements "$SWIFT_DIR/Entitlements.plist" "$APP_DIR"

echo "==> Build complete: $APP_DIR"
echo "    Run scripts/install.sh to install."
