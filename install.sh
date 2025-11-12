#!/bin/bash
set -e

# Automatic Secret Rotation (asr) installer
# Usage: curl -fsSL https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.sh | bash

echo "Installing Automatic Secret Rotation (asr)..."

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "Rust/Cargo not found. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

# Check Rust version
RUST_VERSION=$(rustc --version | awk '{print $2}')
echo "Using Rust version: $RUST_VERSION"

# Clone or update repository
INSTALL_DIR="${ASR_INSTALL_DIR:-$HOME/.asr}"
if [ -d "$INSTALL_DIR" ]; then
    echo "Updating existing installation at $INSTALL_DIR..."
    cd "$INSTALL_DIR"
    git pull
else
    echo "Cloning repository to $INSTALL_DIR..."
    git clone https://github.com/kelleyblackmore/Automatic-Secret-Rotation.git "$INSTALL_DIR"
    cd "$INSTALL_DIR"
fi

# Build and install
echo "Building asr..."
cargo build --release

# Install to user bin
BIN_DIR="${HOME}/.local/bin"
mkdir -p "$BIN_DIR"
cp target/release/asr "$BIN_DIR/"

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
    echo ""
    echo "⚠️  Add $BIN_DIR to your PATH by adding this to your ~/.bashrc or ~/.zshrc:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

# Verify installation
if command -v asr &> /dev/null; then
    echo ""
    echo "✅ asr installed successfully!"
    asr --version
    echo ""
    echo "Get started with:"
    echo "  asr --help"
    echo "  asr init        # Create a config file"
else
    echo ""
    echo "⚠️  Installation complete, but 'asr' is not in PATH."
    echo "Add $BIN_DIR to your PATH or run directly: $BIN_DIR/asr"
fi

echo ""
echo "For more information, visit:"
echo "  https://github.com/kelleyblackmore/Automatic-Secret-Rotation"
