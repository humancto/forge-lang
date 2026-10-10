#!/bin/sh
# Forge installer.
#
#   curl -fsSL https://raw.githubusercontent.com/humancto/forge-lang/main/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- v0.10.0      # specific version
#
# Environment:
#   FORGE_INSTALL_DIR          install location (default: ~/.forge/bin)
#   FORGE_INSTALL_SKIP_VERIFY  set to 1 to skip SHA-256 verification (not recommended)
#   FORGE_RELEASE_BASE_URL     mirror serving <tag>/<asset> (default: GitHub releases)
#
# Asset naming is shared with .github/workflows/release.yml:
#   forge-<tag>-<target>.tar.gz containing forge-<tag>-<target>/{forge,libforge_lang.a,...}
#   SHA256SUMS.txt listing every archive.
set -e

REPO="humancto/forge-lang"
INSTALL_DIR="${FORGE_INSTALL_DIR:-$HOME/.forge/bin}"

# Verify the downloaded archive against the release's SHA256SUMS.txt.
verify_checksum() {
    if [ "${FORGE_INSTALL_SKIP_VERIFY:-0}" = "1" ]; then
        echo "Warning: skipping checksum verification (FORGE_INSTALL_SKIP_VERIFY=1)"
        return 0
    fi

    if ! curl -sSfL "${BASE_URL}/SHA256SUMS.txt" -o "${TMPDIR}/SHA256SUMS.txt"; then
        echo "Error: could not download ${BASE_URL}/SHA256SUMS.txt to verify the archive."
        echo "Re-run with FORGE_INSTALL_SKIP_VERIFY=1 to install without verification."
        exit 1
    fi

    EXPECTED=$(awk -v f="$ARCHIVE" '$2 == f || $2 == "*"f { print $1 }' "${TMPDIR}/SHA256SUMS.txt" | head -1)
    if [ -z "$EXPECTED" ]; then
        echo "Error: ${ARCHIVE} is not listed in SHA256SUMS.txt"
        exit 1
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL=$(sha256sum "${TMPDIR}/${ARCHIVE}" | awk '{ print $1 }')
    elif command -v shasum >/dev/null 2>&1; then
        ACTUAL=$(shasum -a 256 "${TMPDIR}/${ARCHIVE}" | awk '{ print $1 }')
    else
        echo "Error: need sha256sum or shasum to verify the download."
        echo "Re-run with FORGE_INSTALL_SKIP_VERIFY=1 to install without verification."
        exit 1
    fi

    if [ "$EXPECTED" != "$ACTUAL" ]; then
        echo "Error: checksum mismatch for ${ARCHIVE}"
        echo "  expected: ${EXPECTED}"
        echo "  actual:   ${ACTUAL}"
        exit 1
    fi
    echo "Checksum verified (sha256 ${ACTUAL})"
}

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
            echo "Forge's installer supports Linux and macOS. On Windows, download"
            echo "forge-<version>-x86_64-pc-windows-msvc.zip from"
            echo "https://github.com/${REPO}/releases, or build from source:"
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

    if [ -n "${1:-}" ]; then
        VERSION="$1"
    else
        VERSION=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' \
            | head -1 \
            | sed 's/.*"tag_name": *"//;s/".*//')
        if [ -z "$VERSION" ]; then
            echo "Error: could not determine latest version."
            echo "Install a specific version: curl -sSf ... | sh -s -- v0.10.0"
            echo "Or install via cargo: cargo install forge-lang"
            exit 1
        fi
    fi
    # Release tags carry a leading "v"; accept "0.10.0" as well.
    case "$VERSION" in
        v*) ;;
        *) VERSION="v${VERSION}" ;;
    esac

    PKG="forge-${VERSION}-${TARGET}"
    ARCHIVE="${PKG}.tar.gz"
    BASE_URL="${FORGE_RELEASE_BASE_URL:-https://github.com/${REPO}/releases/download}/${VERSION}"
    URL="${BASE_URL}/${ARCHIVE}"

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

    verify_checksum

    echo "Extracting..."
    tar xzf "${TMPDIR}/${ARCHIVE}" -C "${TMPDIR}"

    mkdir -p "$INSTALL_DIR"
    mv "${TMPDIR}/${PKG}/forge" "${INSTALL_DIR}/forge"
    chmod +x "${INSTALL_DIR}/forge"
    # Static runtime: `forge build --native` / `--aot` look for it next to the
    # forge executable and then produce standalone binaries.
    if [ -f "${TMPDIR}/${PKG}/libforge_lang.a" ]; then
        mv "${TMPDIR}/${PKG}/libforge_lang.a" "${INSTALL_DIR}/libforge_lang.a"
    fi

    echo ""
    echo "Forge ${VERSION} installed to ${INSTALL_DIR}/forge"
    echo ""

    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) ;;
        *)
            echo "Add Forge to your PATH by adding this to your shell profile:"
            echo ""
            SHELL_NAME=$(basename "${SHELL:-sh}")
            # The profile path is only printed for the user, so keep "~" literal.
            # shellcheck disable=SC2088
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
