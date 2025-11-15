#!/bin/bash
set -e

# Automatic Secret Rotation installer
# Usage: curl -fsSL https://raw.githubusercontent.com/kelleyblackmore/Automatic-Secret-Rotation/main/install.sh | bash
#
# This script downloads a pre-built binary from the latest GitHub release when
# available. If no suitable binary is found or if the environment variable
# ASR_BUILD_FROM_SOURCE is set, it will build the project from source.
#
# Binary name defaults:
#   - macOS: secret-rotator (to avoid conflict with system 'asr' tool)
#   - Other platforms: asr
#
# Override options:
#   ASR_BINARY_NAME=custom-name ./install.sh    # Custom binary name
#   ASR_BUILD_FROM_SOURCE=1 ./install.sh        # Force build from source (skip download)
#   ASR_VERSION=1.2.3 ./install.sh              # Install a specific version (defaults to latest)

# Print a friendly heading
echo "Installing Automatic Secret Rotation..."

# Determine binary name
# On macOS, default to 'secret-rotator' to avoid conflict with system 'asr' tool.
# Users can override with ASR_BINARY_NAME environment variable.
if [[ "$OSTYPE" == "darwin"* ]]; then
    DEFAULT_BINARY_NAME="secret-rotator"
    if command -v /usr/sbin/asr &> /dev/null; then
        echo ""
        echo "WARNING: macOS detected: System tool 'asr' (Apple Software Restore) found at /usr/sbin/asr"
        echo "   Defaulting to binary name 'secret-rotator' to avoid conflict"
        echo "   (Override with: ASR_BINARY_NAME=asr ./install.sh)"
        echo ""
    fi
else
    DEFAULT_BINARY_NAME="asr"
fi

# Allow override via environment variable
BINARY_NAME="${ASR_BINARY_NAME:-$DEFAULT_BINARY_NAME}"

echo "Binary will be installed as: $BINARY_NAME"

# Detect platform and architecture
# Output is of the form OS-ARCH, e.g. darwin-arm64, linux-x86_64.
detect_platform() {
    local os arch
    case "$OSTYPE" in
        darwin*) os="darwin" ;;
        linux*)  os="linux"  ;;
        *)       os="unknown" ;;
    esac

    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        arm64|aarch64) arch="arm64" ;;
        *) arch="unknown" ;;
    esac
    echo "${os}-${arch}"
}

PLATFORM=$(detect_platform)
BIN_DIR="${HOME}/.local/bin"
mkdir -p "$BIN_DIR"

# Try to download a pre-built binary from GitHub releases. Falls back to
# building from source if the download fails.
download_binary() {
    local platform=$1
    local repo="kelleyblackmore/Automatic-Secret-Rotation"

    echo "Checking for pre-built binary for $platform..."
    
    # Determine asset name (e.g., asr-darwin-arm64, asr-linux-x86_64)
    local asset_name="asr-${platform}"
    local download_url

    # If ASR_VERSION is set, use a specific version tag (prepend 'v' if missing).
    if [ -n "$ASR_VERSION" ]; then
        if [[ "$ASR_VERSION" == v* ]]; then
            download_url="https://github.com/$repo/releases/download/${ASR_VERSION}/${asset_name}"
        else
            download_url="https://github.com/$repo/releases/download/v${ASR_VERSION}/${asset_name}"
        fi
    else
        # Otherwise use the latest release download URL. See
        # https://josh-ops.com/posts/github-download-latest-release/ for details.
        download_url="https://github.com/$repo/releases/latest/download/${asset_name}"
    fi

    echo "Downloading $asset_name from $download_url..."
    if curl -fsSL -o "$BIN_DIR/$BINARY_NAME" -L "$download_url"; then
        chmod +x "$BIN_DIR/$BINARY_NAME"
        echo "Downloaded pre-built binary successfully!"
        return 0
    else
        echo "Pre-built binary not available for $platform, will build from source"
        return 1
    fi
}

# Try to download the binary, fall back to building from source. Skip download
# entirely if ASR_BUILD_FROM_SOURCE is set.
if [ "${ASR_BUILD_FROM_SOURCE:-0}" = "1" ]; then
    echo "ASR_BUILD_FROM_SOURCE=1 set, skipping binary download and building from source..."
    download_success=false
elif ! download_binary "$PLATFORM"; then
    download_success=false
else
    download_success=true
fi

if [ "$download_success" != "true" ]; then
    echo ""
    echo "Building from source..."

    # Check if Rust is installed
    if ! command -v cargo &> /dev/null; then
        echo "Rust/Cargo not found. Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi

    # Show the Rust version for transparency
    RUST_VERSION=$(rustc --version | awk '{print $2}')
    echo "Using Rust version: $RUST_VERSION"

    # Clone or update the repository
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
    echo "Building from source..."
    cargo build --release

    # Install to user bin with the chosen name
    cp target/release/asr "$BIN_DIR/$BINARY_NAME"
    chmod +x "$BIN_DIR/$BINARY_NAME"
    echo "Built and installed from source!"
fi

# Ensure ~/.local/bin is in PATH
SHELL_CONFIG=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_CONFIG="$HOME/.zshrc"
elif [ -f "$HOME/.bash_profile" ]; then
    SHELL_CONFIG="$HOME/.bash_profile"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_CONFIG="$HOME/.bashrc"
fi

if [ -n "$SHELL_CONFIG" ]; then
    # Check if PATH modification already exists
    if ! grep -q "export PATH=\"\$HOME/.local/bin:\$PATH\"" "$SHELL_CONFIG" 2>/dev/null; then
        echo "" >> "$SHELL_CONFIG"
        echo "# Automatic Secret Rotation" >> "$SHELL_CONFIG"
        echo "export PATH=\"\$HOME/.local/bin:\$PATH\"" >> "$SHELL_CONFIG"
        echo "Added $BIN_DIR to PATH in $SHELL_CONFIG"
        echo "   Run 'source $SHELL_CONFIG' or restart your terminal to use '$BINARY_NAME'"
    else
        echo "$BIN_DIR already in PATH configuration"
    fi
else
    echo ""
    echo "WARNING: Please add $BIN_DIR to your PATH by adding this to your shell config:"
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
fi

# Verify installation
if [ -f "$BIN_DIR/$BINARY_NAME" ]; then
    echo ""
    echo "$BINARY_NAME installed successfully!"
    "$BIN_DIR/$BINARY_NAME" --version || true

    echo ""
    echo "Get started with:"
    echo "  $BINARY_NAME --help"
    echo "  $BINARY_NAME init        # Create a config file"
    echo ""

    if [[ "$OSTYPE" == "darwin"* ]] && [ "$BINARY_NAME" = "secret-rotator" ]; then
        echo "Tip: On macOS, the binary is installed as 'secret-rotator' to avoid"
        echo "   conflict with the system 'asr' tool. If you prefer 'asr', reinstall with:"
        echo "   ASR_BINARY_NAME=asr ./install.sh"
    fi
else
    echo ""
    echo "ERROR: Installation failed - binary not found at $BIN_DIR/$BINARY_NAME"
fi

echo ""
echo "For more information, visit:"
echo "  https://github.com/kelleyblackmore/Automatic-Secret-Rotation"
