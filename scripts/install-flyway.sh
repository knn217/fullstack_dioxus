#!/usr/bin/env bash
set -Eeuo pipefail

VERSION="13.6.0"
INSTALL_DIR="/opt/flyway/$VERSION"
BIN_LINK="/usr/local/bin/flyway"

# Skip if already installed
if command -v flyway >/dev/null 2>&1; then
    echo "Flyway is already installed: $(flyway -v)"
    exit 0
fi

# Download and extract if necessary
if [[ ! -x "$INSTALL_DIR/flyway" ]]; then
    echo "Installing Flyway $VERSION..."

    TMP_DIR="$(mktemp -d)"
    trap 'rm -rf "$TMP_DIR"' EXIT

    wget -qO- \
        "https://download.red-gate.com/maven/release/com/redgate/flyway/flyway-commandline/$VERSION/flyway-commandline-$VERSION-linux-x64.tar.gz" \
        | tar -xz -C "$TMP_DIR"

    sudo mkdir -p "$(dirname "$INSTALL_DIR")"
    sudo mv "$TMP_DIR/flyway-$VERSION" "$INSTALL_DIR"
fi

# Register executable globally
sudo ln -s "$INSTALL_DIR/flyway" "$BIN_LINK"

echo "Flyway installed: $(flyway -v)"