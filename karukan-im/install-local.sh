#!/usr/bin/env bash
#
# karukan ローカルインストールスクリプト
# ~/.local にインストールし、fcitx5 の設定を行います。sudo 不要。
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ADDON_DIR="$REPO_ROOT/karukan-im/fcitx5-addon"
CONFIG_DIR="$REPO_ROOT/karukan-im/config"
PREFIX="$HOME/.local"

# --- Build ---
echo "==> Building karukan-im fcitx5 addon..."
cmake -B "$ADDON_DIR/build" -DCMAKE_INSTALL_PREFIX="$PREFIX" "$ADDON_DIR"
cmake --build "$ADDON_DIR/build" -j

# --- Install ---
echo "==> Installing to $PREFIX ..."
cmake --install "$ADDON_DIR/build"

# --- FCITX_ADDON_DIRS (environment.d) ---
ENV_DIR="$HOME/.config/environment.d"
ENV_FILE="$ENV_DIR/fcitx5-karukan.conf"
mkdir -p "$ENV_DIR"

# Detect the system fcitx5 addon directory so that standard addons remain
# accessible after setting FCITX_ADDON_DIRS.
SYSTEM_FCITX5_ADDON_DIR="$(pkg-config --variable=addondir fcitx5 2>/dev/null || true)"
if [ -z "$SYSTEM_FCITX5_ADDON_DIR" ]; then
    for candidate_dir in /usr/lib/x86_64-linux-gnu/fcitx5 /usr/lib/fcitx5 /usr/lib64/fcitx5; do
        if [ -d "$candidate_dir" ]; then
            SYSTEM_FCITX5_ADDON_DIR="$candidate_dir"
            break
        fi
    done
fi

if [ -n "$SYSTEM_FCITX5_ADDON_DIR" ]; then
    # \${HOME} is intentionally escaped so that the literal "${HOME}" is written
    # to the file; systemd expands it at session start.  $SYSTEM_FCITX5_ADDON_DIR
    # is expanded now (at install time) to record the detected system path.
    FCITX5_ADDON_DIRS_VALUE="\${HOME}/.local/lib/fcitx5:$SYSTEM_FCITX5_ADDON_DIR"
else
    FCITX5_ADDON_DIRS_VALUE="\${HOME}/.local/lib/fcitx5"
fi

cat > "$ENV_FILE" << EOF
FCITX_ADDON_DIRS=$FCITX5_ADDON_DIRS_VALUE
EOF
echo "==> Wrote $ENV_FILE"

# Update the current shell session without losing any pre-existing value.
export FCITX_ADDON_DIRS="$HOME/.local/lib/fcitx5${SYSTEM_FCITX5_ADDON_DIR:+:$SYSTEM_FCITX5_ADDON_DIR}${FCITX_ADDON_DIRS:+:$FCITX_ADDON_DIRS}"

# --- Config ---
KARUKAN_CONFIG_DIR="$HOME/.config/karukan-im"
if [ ! -f "$KARUKAN_CONFIG_DIR/config.toml" ]; then
    mkdir -p "$KARUKAN_CONFIG_DIR"
    cp "$CONFIG_DIR/default.toml" "$KARUKAN_CONFIG_DIR/config.toml"
    echo "==> Copied default config to $KARUKAN_CONFIG_DIR/config.toml"
else
    echo "==> $KARUKAN_CONFIG_DIR/config.toml already exists, skipping"
fi

# --- User dictionary ---
USER_DICT_DIR="$HOME/.local/share/karukan-im/user_dicts"
if [ ! -d "$USER_DICT_DIR" ] || [ -z "$(ls -A "$USER_DICT_DIR" 2>/dev/null)" ]; then
    mkdir -p "$USER_DICT_DIR"
    cp "$CONFIG_DIR/default_user_dict.txt" "$USER_DICT_DIR/default.txt"
    echo "==> Copied default user dictionary to $USER_DICT_DIR/default.txt"
else
    echo "==> $USER_DICT_DIR already has files, skipping"
fi

# --- Restart fcitx5 ---
echo "==> Restarting fcitx5..."
fcitx5 -r -d 2>/dev/null || true

echo ""
echo "Done! fcitx5-configtool で Karukan を追加してください。"
