#!/bin/sh
set -eu

REPO="Bigalan09/Librarian"
BINARY="librarian"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"

    case "$platform" in
        Linux)  os="unknown-linux-gnu" ;;
        Darwin) os="apple-darwin" ;;
        *)      echo "Error: unsupported platform $platform" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64)  arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *)             echo "Error: unsupported architecture $arch" >&2; exit 1 ;;
    esac

    target="${arch}-${os}"

    # Get latest release tag
    tag="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' | head -1 | cut -d '"' -f 4)"

    if [ -z "$tag" ]; then
        echo "Error: could not determine latest release" >&2
        exit 1
    fi

    url="https://github.com/${REPO}/releases/download/${tag}/${BINARY}-${target}.tar.gz"
    echo "Downloading ${BINARY} ${tag} for ${target}..."

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    curl -fsSL "$url" -o "${tmpdir}/${BINARY}.tar.gz"
    tar -xzf "${tmpdir}/${BINARY}.tar.gz" -C "$tmpdir"

    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmpdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "${tmpdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    fi

    chmod +x "${INSTALL_DIR}/${BINARY}"
    echo "Installed ${BINARY} ${tag} to ${INSTALL_DIR}/${BINARY}"
}

main
