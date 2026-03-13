#!/bin/bash
set -e

# Open Flow Installation Script
# Usage: curl -sSL https://raw.githubusercontent.com/jqlong17/open-flow/master/install.sh | sh

REPO="jqlong17/open-flow"
INSTALL_DIR="$HOME/.local/bin"
VERSION="${VERSION:-latest}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}Installing Open Flow...${NC}"

# Check macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo -e "${RED}Error: Open Flow currently only supports macOS${NC}"
    exit 1
fi

# Check architecture
ARCH=$(uname -m)
if [[ "$ARCH" == "arm64" ]]; then
    TARGET="aarch64-apple-darwin"
elif [[ "$ARCH" == "x86_64" ]]; then
    TARGET="x86_64-apple-darwin"
else
    echo -e "${RED}Error: Unsupported architecture: $ARCH${NC}"
    exit 1
fi

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download URL
if [[ "$VERSION" == "latest" ]]; then
    URL="https://github.com/${REPO}/releases/latest/download/open-flow-${TARGET}.tar.gz"
else
    URL="https://github.com/${REPO}/releases/download/${VERSION}/open-flow-${TARGET}.tar.gz"
fi

echo "Downloading from ${URL}..."

# Download and extract
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

if command -v curl &> /dev/null; then
    curl -sSL "$URL" -o "$TMP_DIR/open-flow.tar.gz"
elif command -v wget &> /dev/null; then
    wget -q "$URL" -O "$TMP_DIR/open-flow.tar.gz"
else
    echo -e "${RED}Error: curl or wget is required${NC}"
    exit 1
fi

tar -xzf "$TMP_DIR/open-flow.tar.gz" -C "$TMP_DIR"

# Install binary
cp "$TMP_DIR/open-flow" "$INSTALL_DIR/"
chmod +x "$INSTALL_DIR/open-flow"

# Add to PATH if needed
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}Adding $INSTALL_DIR to PATH...${NC}"
    
    if [[ -f "$HOME/.zshrc" ]]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.zshrc"
        echo -e "${GREEN}Added to ~/.zshrc. Run 'source ~/.zshrc' to update PATH.${NC}"
    elif [[ -f "$HOME/.bashrc" ]]; then
        echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc"
        echo -e "${GREEN}Added to ~/.bashrc. Run 'source ~/.bashrc' to update PATH.${NC}"
    fi
fi

echo -e "${GREEN}✓ Open Flow installed successfully!${NC}"
echo ""
echo "Starting Open Flow (first run auto-downloads ~230MB model)..."
echo ""
export PATH="$INSTALL_DIR:$PATH"
exec "$INSTALL_DIR/open-flow" start
