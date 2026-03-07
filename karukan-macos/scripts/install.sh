#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_DIR/build"
APP_NAME="Karukan.app"
INSTALL_DIR="$HOME/Library/Input Methods"

if [ ! -d "$BUILD_DIR/$APP_NAME" ]; then
    echo "ERROR: $BUILD_DIR/$APP_NAME not found. Run scripts/build.sh first."
    exit 1
fi

echo "==> Installing to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

# Kill existing instance if running
killall KarukanInputMethod 2>/dev/null || true

# Copy the app bundle
rm -rf "$INSTALL_DIR/$APP_NAME"
cp -R "$BUILD_DIR/$APP_NAME" "$INSTALL_DIR/"

echo "==> Installed: $INSTALL_DIR/$APP_NAME"
echo ""
echo "To activate:"
echo "  1. Open System Settings > Keyboard > Input Sources"
echo "  2. Click '+' and find 'Karukan' under Japanese"
echo "  3. Add it and switch to it"
echo ""
echo "Note: You may need to log out and back in for the input method to appear."
