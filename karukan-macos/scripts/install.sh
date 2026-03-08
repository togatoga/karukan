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
echo "重要: macOS IME は ~/Library/Input Methods/ に配置する必要があります。"
echo "      /Applications/ からは認識されません。"
echo ""
echo "有効化手順:"
echo "  1. ログアウトしてログインし直す（または再起動）"
echo "  2. System Settings > Keyboard > Input Sources > Edit > +"
echo "  3. Japanese の下にある 'Karukan' を追加"
echo ""
echo "ログアウトせずに試す場合:"
echo "  killall KarukanInputMethod 2>/dev/null"
echo "  open '$INSTALL_DIR/$APP_NAME'"
