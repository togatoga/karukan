#!/usr/bin/env bash
# The karukan version, kept in one place: the root Cargo.toml's
# [workspace.package] version.
#
#   scripts/version.sh current                  print it
#   scripts/version.sh bump patch|minor|major   raise it, then print the new one
#
# A bump also refreshes Cargo.lock for the workspace crates and adds the
# release to the AppStream metadata that software centers show.
set -euo pipefail
cd "$(dirname "$0")/.."

metainfo=karukan-im/fcitx5/fcitx5-addon/org.fcitx.Fcitx5.Addon.Karukan.metainfo.xml.in

current() {
    sed -n 's/^version = "\([0-9]*\.[0-9]*\.[0-9]*\)"$/\1/p' Cargo.toml | head -n 1
}

case "${1:-}" in
current)
    current
    ;;
bump)
    now=$(current)
    IFS=. read -r major minor patch <<<"$now"
    case "${2:-}" in
    major) next="$((major + 1)).0.0" ;;
    minor) next="$major.$((minor + 1)).0" ;;
    patch) next="$major.$minor.$((patch + 1))" ;;
    *) echo "usage: $0 bump patch|minor|major" >&2; exit 2 ;;
    esac
    sed -i "0,/^version = \"$now\"\$/s//version = \"$next\"/" Cargo.toml
    cargo update --workspace --quiet
    sed -i "s|^  <releases>\$|  <releases>\n    <release version=\"$next\" date=\"$(date -u +%F)\"/>|" "$metainfo"
    echo "$next"
    ;;
*)
    echo "usage: $0 current | bump patch|minor|major" >&2
    exit 2
    ;;
esac
