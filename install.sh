#!/bin/sh
set -e

REPO="humancto/forge-lang"
INSTALL_DIR="${FORGE_INSTALL_DIR:-$HOME/.forge/bin}"

main() {
    echo "Installing Forge..."
    echo ""

    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Linux)  OS_TARGET="unknown-linux-gnu" ;;
        Darwin) OS_TARGET="apple-darwin" ;;
        *)
            echo "Error: unsupported OS: $OS"
            echo "Forge supports Linux and macOS. For other platforms, build from source:"
            echo "  cargo install forge-lang"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)   ARCH_TARGET="x86_64" ;;
        aarch64|arm64)  ARCH_TARGET="aarch64" ;;
        *)
            echo "Error: unsupported architecture: $ARCH"
            exit 1
            ;;
    esac

    TARGET="${ARCH_TARGET}-${OS_TARGET}"

    if [ -n "$1" ]; then
        VERSION="$1"
    else
        VERSION=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' \
            | head -1 \
            | sed 's/.*"tag_name": *"//;s/".*//')
        if [ -z "$VERSION" ]; then
            echo "Error: could not determine latest version."
            echo "Install a specific version: curl -sSf ... | sh -s -- v0.3.0"
            echo "Or install via cargo: cargo install forge-lang"
            exit 1
        fi
    fi

    ARCHIVE="forge-${VERSION}-${TARGET}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"

    echo "  Platform: ${OS} ${ARCH}"
    echo "  Version:  ${VERSION}"
    echo "  Target:   ${TARGET}"
    echo ""

    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT

    echo "Downloading ${URL}..."
    if ! curl -sSfL "$URL" -o "${TMPDIR}/${ARCHIVE}"; then
        echo ""
        echo "Error: download failed."
        echo "Check available releases: https://github.com/${REPO}/releases"
        echo ""
        echo "Alternative install methods:"
        echo "  cargo install forge-lang"
        echo "  brew install humancto/tap/forge"
        exit 1
    fi

    echo "Extracting..."
    tar xzf "${TMPDIR}/${ARCHIVE}" -C "${TMPDIR}"

    mkdir -p "$INSTALL_DIR"
    mv "${TMPDIR}/forge-${VERSION}-${TARGET}/forge" "${INSTALL_DIR}/forge"
    chmod +x "${INSTALL_DIR}/forge"

    echo ""
    echo "Forge ${VERSION} installed to ${INSTALL_DIR}/forge"
    echo ""

    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo "Add Forge to your PATH by adding this to your shell profile:"
            echo ""
            SHELL_NAME=$(basename "$SHELL")
            case "$SHELL_NAME" in
                zsh)  PROFILE="~/.zshrc" ;;
                bash) PROFILE="~/.bashrc" ;;
                fish) PROFILE="~/.config/fish/config.fish" ;;
                *)    PROFILE="~/.profile" ;;
            esac
            if [ "$SHELL_NAME" = "fish" ]; then
                echo "  echo 'set -gx PATH ${INSTALL_DIR} \$PATH' >> ${PROFILE}"
            else
                echo "  echo 'export PATH=\"${INSTALL_DIR}:\$PATH\"' >> ${PROFILE}"
            fi
            echo ""
            echo "Then restart your shell or run: source ${PROFILE}"
            echo ""
            ;;
    esac

    echo "Verify installation:"
    echo "  forge --version"
    echo ""
    echo "Get started:"
    echo "  forge              # start REPL"
    echo "  forge run hello.fg # run a file"
    echo "  forge learn        # interactive tutorial"
}

main "$@"
