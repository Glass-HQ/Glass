#!/bin/bash

# Run Glass with CEF support
# This script builds and runs Glass from the bundled .app to ensure CEF works correctly

set -euo pipefail

BUILD_TYPE="${1:-debug}"
CEF_PATH="${CEF_PATH:-$HOME/.local/share/cef}"

# Get architecture
version_info=$(rustc --version --verbose)
host_line=$(echo "$version_info" | grep host)
target_triple=${host_line#*: }

# Determine paths based on build type
if [[ "$BUILD_TYPE" == "release" ]]; then
    TARGET_DIR="release"
    BUILD_FLAG="--release"
else
    TARGET_DIR="debug"
    BUILD_FLAG=""
    export CARGO_INCREMENTAL=true
fi

APP_PATH="target/${target_triple}/${TARGET_DIR}/bundle/Glass.app"

# Check if we need to rebuild/rebundle
NEEDS_BUNDLE=false

if [[ ! -d "$APP_PATH" ]]; then
    NEEDS_BUNDLE=true
elif [[ ! -d "$APP_PATH/Contents/Frameworks/Chromium Embedded Framework.framework" ]]; then
    NEEDS_BUNDLE=true
fi

if [[ "$NEEDS_BUNDLE" == "true" ]]; then
    echo "Building and bundling Glass with CEF..."
    ./script/bundle-mac-cef "$BUILD_TYPE"
fi

# Set remote server path for development
export ZED_COPY_REMOTE_SERVER="$PWD/target/${target_triple}/${TARGET_DIR}/remote_server.gz"

# Run the bundled app
echo "Running Glass from: $APP_PATH"
exec "$APP_PATH/Contents/MacOS/Glass" "$@"
