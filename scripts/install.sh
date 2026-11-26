#!/usr/bin/env sh
set -eu

REPO="arsyadal/tala"
INSTALL_DIR="${TALA_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${TALA_VERSION:-latest}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Linux) platform="linux" ;;
  Darwin) platform="macos" ;;
  *)
    echo "tala installer: unsupported OS '$os'. Download a release archive from https://github.com/$REPO/releases" >&2
    exit 1
    ;;
esac

case "$arch" in
  x86_64|amd64) target_arch="x86_64" ;;
  aarch64|arm64) target_arch="aarch64" ;;
  *)
    echo "tala installer: unsupported architecture '$arch'." >&2
    exit 1
    ;;
esac

asset="tala-${platform}-${target_arch}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  url="https://github.com/${REPO}/releases/latest/download/${asset}"
else
  url="https://github.com/${REPO}/releases/download/${VERSION}/${asset}"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading ${asset} from ${url}..."
if ! curl -fsSL "$url" -o "$tmp/$asset"; then
  echo "tala installer: failed to download $asset. Verify that a GitHub Release has been published." >&2
  exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$INSTALL_DIR"
install -m 755 "$tmp/tala" "$INSTALL_DIR/tala"

echo "Installed tala to ${INSTALL_DIR}/tala"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Add ${INSTALL_DIR} to your PATH to run 'tala' from any terminal." ;;
esac
