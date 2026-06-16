#!/bin/bash
# Synapsis Installer - Linux (x86_64 + ARM64)
# Pure Rust - No Python dependencies
set -e

VERSION="${SYNAPSIS_VERSION:-0.8.2}"
REPO="methodwhite/synapsis"
INSTALL_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.local/share/synapsis"

echo "Synapsis v${VERSION} Installer (Linux)"
echo "======================================"
echo ""

ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) TARGET="aarch64-unknown-linux-gnu" ;;
    *)       echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

echo "Platform: Linux (${ARCH}) -> ${TARGET}"
echo "Install dir: ${INSTALL_DIR}"
echo "Data dir: ${DATA_DIR}"
echo ""

mkdir -p "${INSTALL_DIR}"
mkdir -p "${DATA_DIR}"

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/v${VERSION}/synapsis-${TARGET}.tar.gz"

if command -v curl &>/dev/null; then
    echo "Downloading synapsis v${VERSION} for ${TARGET}..."
    TMPDIR=$(mktemp -d)
    trap "rm -rf ${TMPDIR}" EXIT

    if curl -fsSL "${DOWNLOAD_URL}" -o "${TMPDIR}/synapsis.tar.gz"; then
        cd "${TMPDIR}"
        tar xzf synapsis.tar.gz
        cp synapsis/synapsis synapsis/synapsis-mcp synapsis/synapsis-server "${INSTALL_DIR}/" 2>/dev/null || true
        chmod +x "${INSTALL_DIR}/synapsis" "${INSTALL_DIR}/synapsis-mcp" "${INSTALL_DIR}/synapsis-server" 2>/dev/null || true

        echo ""
        echo "Synapsis v${VERSION} installed successfully!"
        "${INSTALL_DIR}/synapsis" --version 2>/dev/null || true
        exit 0
    fi
fi

echo "No pre-built binary found. Building from source..."
echo "Requires: Rust (https://rustup.rs)"

if ! command -v rustc &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "${HOME}/.cargo/env"
fi

TMPDIR=$(mktemp -d)
trap "rm -rf ${TMPDIR}" EXIT

git clone --depth 1 "https://github.com/${REPO}.git" "${TMPDIR}/synapsis"
cd "${TMPDIR}/synapsis"
cargo build --release

cp target/release/synapsis target/release/synapsis-mcp target/release/synapsis-server "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/synapsis" "${INSTALL_DIR}/synapsis-mcp" "${INSTALL_DIR}/synapsis-server"

echo ""
echo "Synapsis built and installed from source!"
"${INSTALL_DIR}/synapsis" --version

if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "Add to your shell profile (~/.bashrc or ~/.zshrc):"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
fi

echo ""
echo "Done!"
