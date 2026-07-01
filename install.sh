#!/usr/bin/env bash
set -euo pipefail

REPO="stremtec/xei"
VERSION="${VERSION:-v0.6.0}"
BIN="xei"

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) TARGET="aarch64-apple-darwin" ;;
      *)     TARGET="x86_64-apple-darwin" ;;
    esac
    ;;
  Linux)
    case "$(uname -m)" in
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      *)       TARGET="x86_64-unknown-linux-gnu" ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $(uname -s)" >&2; exit 1
    ;;
esac

URL="https://github.com/${REPO}/releases/download/${VERSION}/${BIN}-${TARGET}.gz"
DEST="${HOME}/.local/bin"

echo "→ Downloading xei ${VERSION} for ${TARGET}..."
mkdir -p "$DEST"

curl -fsSL "$URL" | gunzip > "${DEST}/${BIN}"
chmod +x "${DEST}/${BIN}"

echo "✓ xei installed to ${DEST}/${BIN}"
echo "  Make sure ${DEST} is in your PATH."
command -v xei >/dev/null 2>&1 && xei --version || echo "  Run: export PATH=\"\${HOME}/.local/bin:\$PATH\""
