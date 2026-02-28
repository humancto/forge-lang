#!/bin/sh
set -e

REPO="forge-lang/forge"
INSTALL_DIR="${FORGE_INSTALL_DIR:-/usr/local/bin}"

main() {
    echo ""
    echo "  ⚒️  Forge Installer"
    echo "  ==================="
    echo ""

    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Darwin)  OS_NAME="macos" ;;
        Linux)   OS_NAME="linux" ;;
        *)       echo "  Error: unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64|amd64)    ARCH_NAME="x86_64" ;;
        aarch64|arm64)   ARCH_NAME="aarch64" ;;
        *)               echo "  Error: unsupported architecture: $ARCH"; exit 1 ;;
    esac

    ARTIFACT="forge-${OS_NAME}-${ARCH_NAME}"

    VERSION=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')

    if [ -z "$VERSION" ]; then
        echo "  Error: could not determine latest version"
        echo "  Try installing from source: cargo install --git https://github.com/${REPO}.git"
        exit 1
    fi

    URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARTIFACT}.tar.gz"

    echo "  Detected: ${OS} ${ARCH}"
    echo "  Version:  ${VERSION}"
    echo "  Binary:   ${ARTIFACT}"
    echo ""

    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT

    echo "  Downloading..."
    curl -fsSL "$URL" -o "$TMPDIR/forge.tar.gz"

    echo "  Extracting..."
    tar xzf "$TMPDIR/forge.tar.gz" -C "$TMPDIR"

    echo "  Installing to ${INSTALL_DIR}/forge..."
    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMPDIR/forge" "$INSTALL_DIR/forge"
    else
        sudo mv "$TMPDIR/forge" "$INSTALL_DIR/forge"
    fi
    chmod +x "$INSTALL_DIR/forge"

    echo ""
    echo "  ✅ Forge ${VERSION} installed successfully!"
    echo ""
    echo "  Get started:"
    echo "    forge version"
    echo "    forge learn"
    echo "    forge"
    echo ""
}

main
