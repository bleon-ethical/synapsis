#!/bin/bash
# Synapsis Installer - macOS (Intel + Apple Silicon)
# Pure Rust - No Python dependencies
set -e

VERSION="${SYNAPSIS_VERSION:-0.3.0}"
REPO="methodwhite/synapsis"
INSTALL_DIR="${HOME}/.local/bin"
DATA_DIR="${HOME}/.local/share/synapsis"

echo "Synapsis v${VERSION} Installer (macOS)"
echo "======================================="
echo ""

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)  TARGET="x86_64-apple-darwin" ;;
    arm64)   TARGET="aarch64-apple-darwin" ;;
    *)       echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

echo "Platform: macOS (${ARCH}) -> ${TARGET}"
echo "Install dir: ${INSTALL_DIR}"
echo "Data dir: ${DATA_DIR}"
echo ""

# Create directories
mkdir -p "${INSTALL_DIR}"
mkdir -p "${DATA_DIR}"

# Try download pre-built binary
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/v${VERSION}/synapsis-${TARGET}.tar.gz"

if command -v curl &>/dev/null; then
    echo "Downloading synapsis v${VERSION} for ${TARGET}..."
    TMPDIR=$(mktemp -d)
    trap "rm -rf ${TMPDIR}" EXIT
    
    if curl -fsSL "${DOWNLOAD_URL}" -o "${TMPDIR}/synapsis.tar.gz"; then
        cd "${TMPDIR}"
        tar xzf synapsis.tar.gz
        cp synapsis/synapsis synapsis/synapsis-mcp "${INSTALL_DIR}/" 2>/dev/null || true
        chmod +x "${INSTALL_DIR}/synapsis" "${INSTALL_DIR}/synapsis-mcp" 2>/dev/null || true
        
        # macOS: remove quarantine attribute
        xattr -d com.apple.quarantine "${INSTALL_DIR}/synapsis" 2>/dev/null || true
        xattr -d com.apple.quarantine "${INSTALL_DIR}/synapsis-mcp" 2>/dev/null || true
        
        echo ""
        echo "Synapsis v${VERSION} installed successfully!"
        "${INSTALL_DIR}/synapsis" --version 2>/dev/null || true
        exit 0
    fi
fi

# Fallback: build from source
echo "No pre-built binary found. Building from source..."
echo "Requires: Xcode Command Line Tools + Rust"

# Check Rust
if ! command -v rustc &>/dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "${HOME}/.cargo/env"
fi

# Check Xcode
if ! xcode-select -p &>/dev/null; then
    echo "Installing Xcode Command Line Tools..."
    xcode-select --install
    echo "After Xcode CLI tools install completes, re-run this script."
    exit 1
fi

TMPDIR=$(mktemp -d)
trap "rm -rf ${TMPDIR}" EXIT

git clone --depth 1 "https://github.com/${REPO}.git" "${TMPDIR}/synapsis"
cd "${TMPDIR}/synapsis"
cargo build --release

cp target/release/synapsis target/release/synapsis-mcp "${INSTALL_DIR}/"
chmod +x "${INSTALL_DIR}/synapsis" "${INSTALL_DIR}/synapsis-mcp"

echo ""
echo "Synapsis built and installed from source!"
"${INSTALL_DIR}/synapsis" --version

# PATH check
if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
    echo ""
    echo "Add to your shell profile (~/.zshrc or ~/.bash_profile):"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
fi

echo ""
echo "Done!"
