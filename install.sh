#!/usr/bin/env sh
set -e

REPO="engnhn/devclean"
BINARY_NAME="devclean"

printf "\033[1;36m==> Installing devclean CLI...\033[0m\n"

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

if [ "$OS" != "linux" ]; then
    printf "\033[1;31mError: devclean currently supports Linux operating systems.\033[0m\n"
    exit 1
fi

case "$ARCH" in
    x86_64|amd64)
        TARGET="x86_64-unknown-linux-gnu"
        ;;
    aarch64|arm64)
        TARGET="aarch64-unknown-linux-gnu"
        ;;
    *)
        printf "\033[1;31mError: Unsupported architecture '%s'. Supported architectures: x86_64, aarch64\033[0m\n" "$ARCH"
        exit 1
        ;;
esac

DEST_DIR="/usr/local/bin"
if [ ! -w "$DEST_DIR" ]; then
    DEST_DIR="$HOME/.local/bin"
    mkdir -p "$DEST_DIR"
fi

DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

printf "Downloading %s for %s (%s)...\n" "$BINARY_NAME" "$OS" "$ARCH"
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/devclean.tar.gz"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_DIR/devclean.tar.gz" "$DOWNLOAD_URL"
else
    printf "\033[1;31mError: Neither curl nor wget was found on your system.\033[0m\n"
    exit 1
fi

tar -xzf "$TMP_DIR/devclean.tar.gz" -C "$TMP_DIR"
chmod +x "$TMP_DIR/$BINARY_NAME"

if [ -w "$DEST_DIR" ]; then
    mv "$TMP_DIR/$BINARY_NAME" "$DEST_DIR/$BINARY_NAME"
else
    sudo mv "$TMP_DIR/$BINARY_NAME" "$DEST_DIR/$BINARY_NAME"
fi

printf "\033[1;32m==> devclean installed successfully to %s/%s\033[0m\n" "$DEST_DIR" "$BINARY_NAME"
printf "\nTo get started, run:\n"
printf "  \033[1;33mdevclean scan ~/projects\033[0m      (Scan projects directory)\n"
printf "  \033[1;33mdevclean --help\033[0m               (View CLI options)\n\n"
