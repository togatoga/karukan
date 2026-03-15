#!/bin/bash
# Run fcitx5 integration tests using Docker
#
# Usage:
#   ./tests/integration/run_fcitx5_test.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

IMAGE_NAME="karukan-fcitx5-test"

echo "Building Docker image..."
docker build \
    -f "$SCRIPT_DIR/Dockerfile.fcitx5" \
    -t "$IMAGE_NAME" \
    "$REPO_ROOT"

echo ""
echo "Running integration tests..."
docker run --rm "$IMAGE_NAME"
