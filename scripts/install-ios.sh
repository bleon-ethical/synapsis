#!/bin/bash
# Synapsis Cross-Compiler - iPhoneOS (iOS) / iPadOS
# Builds for aarch64-apple-ios. Requires macOS + Xcode.
# Pure Rust - No Python dependencies
set -e

echo "Synapsis iPhoneOS Cross-Compiler"
echo "================================"
echo ""

# This must run on macOS with Xcode installed
if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "iPhoneOS builds require macOS with Xcode."
    exit 1
fi

TARGET="aarch64-apple-ios"
VERSION="${SYNAPSIS_VERSION:-0.3.0}"

# Install Rust iOS target
rustup target add "${TARGET}" 2>/dev/null || {
    echo "Installing Rust iOS target: ${TARGET}..."
    rustup target add "${TARGET}"
}

echo "Building synapsis-core for ${TARGET}..."
cd "$(dirname "$0")/../synapsis-core" 2>/dev/null || cd "$(dirname "$0")/synapsis-core" 2>/dev/null || {
    git clone --depth 1 https://github.com/methodwhite/synapsis-core.git /tmp/synapsis-core
    cd /tmp/synapsis-core
}
cargo build --release --target "${TARGET}" 2>&1 | tail -3

echo "Building synapsis for ${TARGET}..."
cd "$(dirname "$0")/.." 2>/dev/null || cd /tmp && git clone --depth 1 https://github.com/methodwhite/synapsis.git /tmp/synapsis-build && cd /tmp/synapsis-build
cargo build --release --target "${TARGET}" --bin synapsis --bin synapsis-mcp 2>&1 | tail -3

echo ""
echo "iOS binaries built at:"
echo "  target/${TARGET}/release/synapsis"
echo "  target/${TARGET}/release/synapsis-mcp"
echo ""
echo "For iOS deployment, use these with an a-Shell or iSH terminal,"
echo "or embed via FFI in a native iOS app."
echo ""
echo "Alternatively, run synapsis on a server and connect iOS via MCP-TCP."
