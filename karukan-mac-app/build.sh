#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
APP_NAME="Karukan"
APP_BUNDLE="$SCRIPT_DIR/$APP_NAME.app"
RUST_TARGET_DIR="$ROOT_DIR/target/release"

echo "=== Building Karukan IME for macOS ==="

# Step 1: Build Rust static library
echo "[1/4] Building Rust static library..."
(cd "$ROOT_DIR" && cargo build --release -p karukan-im)
echo "  -> libkarukan_im.a ready"

# Step 2: Create .app bundle structure
echo "[2/4] Creating app bundle..."
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"
cp "$SCRIPT_DIR/Info.plist" "$APP_BUNDLE/Contents/"

# Step 3: Compile Swift sources
echo "[3/4] Compiling Swift app..."
SWIFT_FILES=(
    "$SCRIPT_DIR/main.swift"
    "$SCRIPT_DIR/KarukanInputController.swift"
)

swiftc \
    -import-objc-header "$SCRIPT_DIR/bridging-header.h" \
    -framework Cocoa \
    -framework InputMethodKit \
    -L "$RUST_TARGET_DIR" \
    -lkarukan_im \
    -I "$ROOT_DIR/karukan-im/include" \
    -o "$APP_BUNDLE/Contents/MacOS/$APP_NAME" \
    "${SWIFT_FILES[@]}"
echo "  -> Swift binary compiled"

# Step 4: Ad-hoc code sign
echo "[4/4] Code signing (ad-hoc)..."
codesign --force --sign - "$APP_BUNDLE"
echo "  -> Signed"

echo ""
echo "=== Build complete ==="
echo "App bundle: $APP_BUNDLE"
echo ""

# Install if requested
INSTALL_DIR="$HOME/Library/Input Methods"
if [[ "${1:-}" == "--install" ]]; then
    echo "Installing to $INSTALL_DIR ..."
    killall Karukan 2>/dev/null || true
    sleep 1
    rm -rf "$INSTALL_DIR/$APP_NAME.app"
    cp -R "$APP_BUNDLE" "$INSTALL_DIR/"
    echo "  -> Installed"
    echo ""
    echo "Log out and log back in, then add Karukan in System Settings > Keyboard > Input Sources > Japanese"
else
    echo "To install:"
    echo "  $0 --install"
    echo ""
    echo "Or manually:"
    echo "  killall Karukan; rm -rf \"$INSTALL_DIR/$APP_NAME.app\" && cp -R \"$APP_BUNDLE\" \"$INSTALL_DIR/\""
fi
