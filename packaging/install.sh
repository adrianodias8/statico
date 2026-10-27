#!/bin/sh
# Install the latest Statico release: curl -fsSL <url>/install.sh | sh
# Set STATICO_REPO to the GitHub repository that publishes releases (owner/name).
set -eu
REPO="${STATICO_REPO:-adrianodias8/statico}"
DEST="${STATICO_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)          TARGET=x86_64-unknown-linux-musl ;;
  Linux-aarch64|Linux-arm64) TARGET=aarch64-unknown-linux-musl ;;
  Darwin-x86_64)         TARGET=x86_64-apple-darwin ;;
  Darwin-arm64)          TARGET=aarch64-apple-darwin ;;
  *) echo "Unsupported platform: $(uname -s) $(uname -m). Try: cargo install statico-cli" >&2; exit 1 ;;
esac

URL="https://github.com/$REPO/releases/latest/download/statico-$TARGET.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "Downloading $URL"
curl -fsSL "$URL" | tar xz -C "$TMP"
mkdir -p "$DEST"
mv "$TMP/statico" "$DEST/statico"
chmod +x "$DEST/statico"
echo "Installed statico to $DEST/statico"
case ":$PATH:" in *":$DEST:"*) ;; *) echo "Add $DEST to your PATH." ;; esac
"$DEST/statico" --version
