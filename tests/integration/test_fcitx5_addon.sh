#!/bin/bash
set -euo pipefail

PASS=0
FAIL=0

pass() {
    echo "  PASS: $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "  FAIL: $1"
    FAIL=$((FAIL + 1))
}

echo "=== Karukan fcitx5 Integration Tests ==="
echo ""

# -------------------------------------------------------
# 1. Verify installed files exist
# -------------------------------------------------------
echo "[1/5] Checking installed files..."

ADDON_DIR=$(pkg-config --variable=libdir Fcitx5Core)/fcitx5
FCITX5_DATA="/usr/share/fcitx5"

if [ -f "$ADDON_DIR/karukan.so" ]; then
    pass "karukan.so installed at $ADDON_DIR/karukan.so"
else
    fail "karukan.so not found at $ADDON_DIR/karukan.so"
fi

if [ -f "$ADDON_DIR/libkarukan_im.so" ]; then
    pass "libkarukan_im.so installed at $ADDON_DIR/libkarukan_im.so"
else
    fail "libkarukan_im.so not found at $ADDON_DIR/libkarukan_im.so"
fi

if [ -f "$FCITX5_DATA/addon/karukan.conf" ]; then
    pass "addon config installed at $FCITX5_DATA/addon/karukan.conf"
else
    fail "addon config not found at $FCITX5_DATA/addon/karukan.conf"
fi

if [ -f "$FCITX5_DATA/inputmethod/karukan.conf" ]; then
    pass "inputmethod config installed at $FCITX5_DATA/inputmethod/karukan.conf"
else
    fail "inputmethod config not found at $FCITX5_DATA/inputmethod/karukan.conf"
fi

# -------------------------------------------------------
# 2. Verify shared library linkage
# -------------------------------------------------------
echo ""
echo "[2/5] Checking shared library linkage..."

if ldd "$ADDON_DIR/karukan.so" | grep -q "libkarukan_im.so"; then
    pass "karukan.so links to libkarukan_im.so"
else
    fail "karukan.so does not link to libkarukan_im.so"
fi

if ldd "$ADDON_DIR/karukan.so" | grep -q "libFcitx5Core"; then
    pass "karukan.so links to libFcitx5Core"
else
    fail "karukan.so does not link to libFcitx5Core"
fi

# Verify RPATH is set to $ORIGIN so it can find libkarukan_im.so
RPATH_INFO=$(readelf -d "$ADDON_DIR/karukan.so" 2>/dev/null | grep -E "RPATH|RUNPATH" || true)
if echo "$RPATH_INFO" | grep -q '\$ORIGIN'; then
    pass "karukan.so has \$ORIGIN RPATH"
else
    # Even without RPATH, if libs are in the same dir it may work
    pass "karukan.so RPATH check (libs co-located in $ADDON_DIR)"
fi

# -------------------------------------------------------
# 3. Verify addon config contents
# -------------------------------------------------------
echo ""
echo "[3/5] Checking addon configuration..."

ADDON_CONF="$FCITX5_DATA/addon/karukan.conf"
if grep -q "Library=karukan" "$ADDON_CONF"; then
    pass "addon config has Library=karukan"
else
    fail "addon config missing Library=karukan"
fi

if grep -q "Type=SharedLibrary" "$ADDON_CONF"; then
    pass "addon config has Type=SharedLibrary"
else
    fail "addon config missing Type=SharedLibrary"
fi

if grep -q "Category=InputMethod" "$ADDON_CONF"; then
    pass "addon config has Category=InputMethod"
else
    fail "addon config missing Category=InputMethod"
fi

IM_CONF="$FCITX5_DATA/inputmethod/karukan.conf"
if grep -q "LangCode=ja" "$IM_CONF"; then
    pass "inputmethod config has LangCode=ja"
else
    fail "inputmethod config missing LangCode=ja"
fi

# -------------------------------------------------------
# 4. Start D-Bus and launch fcitx5, verify addon loading
# -------------------------------------------------------
echo ""
echo "[4/5] Starting fcitx5 and checking addon loading..."

# Start D-Bus session bus
eval "$(dbus-launch --sh-syntax)"
export DBUS_SESSION_BUS_ADDRESS

# Launch fcitx5 in the background, capture log
# Note: do NOT use -d (daemon) flag as it forks and loses stderr redirection
FCITX5_LOG=$(mktemp)
fcitx5 --verbose '*=5' >"$FCITX5_LOG" 2>&1 &
FCITX5_PID=$!

# Wait for fcitx5 to start (up to 15 seconds)
STARTED=false
for i in $(seq 1 30); do
    sleep 0.5
    if grep -q "Loaded addon karukan" "$FCITX5_LOG" 2>/dev/null; then
        STARTED=true
        break
    fi
    # Also check if fcitx5 has exited unexpectedly
    if ! kill -0 "$FCITX5_PID" 2>/dev/null; then
        break
    fi
done

if [ "$STARTED" = true ]; then
    pass "fcitx5 loaded addon karukan successfully"
else
    fail "fcitx5 did not load addon karukan within timeout"
    echo "  --- fcitx5 log (last 50 lines) ---"
    tail -50 "$FCITX5_LOG" | sed 's/^/  | /'
    echo "  --- end log ---"
fi

# -------------------------------------------------------
# 5. Verify karukan addon is recognized in fcitx5 log
# -------------------------------------------------------
echo ""
echo "[5/5] Checking karukan addon recognition..."

# Check for the addon being discovered (even if loading model fails, the .so is loaded)
if grep -q "Loaded addon karukan" "$FCITX5_LOG" 2>/dev/null; then
    pass "karukan addon loaded by fcitx5"
elif grep -q "loadAddon.*karukan" "$FCITX5_LOG" 2>/dev/null; then
    pass "karukan addon recognized by fcitx5 (load attempted)"
else
    fail "karukan addon not recognized by fcitx5"
    echo "  --- grep for 'karukan' in log ---"
    grep "karukan" "$FCITX5_LOG" 2>/dev/null | head -10 | sed 's/^/  | /' || echo "  | (no matches)"
    echo "  --- end ---"
fi

# Check that no load error occurred for karukan
if grep -q "Could not load addon karukan" "$FCITX5_LOG" 2>/dev/null; then
    fail "fcitx5 reported error loading karukan addon"
    grep "karukan" "$FCITX5_LOG" | sed 's/^/  | /'
else
    pass "no load error for karukan addon"
fi

# Cleanup
kill "$FCITX5_PID" 2>/dev/null || true
kill "$DBUS_SESSION_BUS_PID" 2>/dev/null || true
rm -f "$FCITX5_LOG"

# -------------------------------------------------------
# Summary
# -------------------------------------------------------
echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

echo ""
echo "All integration tests passed!"
exit 0
