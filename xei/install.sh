#!/usr/bin/env bash
set -euo pipefail

REPO="stremtec/xei"
# Empty = latest release (resolved by GitHub's /releases/latest/download).
# Pin a specific build with:  VERSION=v3.0.8 curl … | bash
VERSION="${VERSION:-}"

case "$(uname -s)" in
  Darwin)
    case "$(uname -m)" in
      arm64) TARGET="aarch64-apple-darwin" ;;
      *)     TARGET="x86_64-apple-darwin" ;;
    esac
    HAS_DESKTOP=1
    ;;
  Linux)
    case "$(uname -m)" in
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      *)       TARGET="x86_64-unknown-linux-gnu" ;;
    esac
    HAS_DESKTOP=0
    ;;
  *)
    echo "Unsupported OS: $(uname -s)" >&2; exit 1
    ;;
esac

DEST="${HOME}/.local/bin"
mkdir -p "$DEST"

install_bin() {
  local name="$1"
  local url
  if [ -z "$VERSION" ]; then
    url="https://github.com/${REPO}/releases/latest/download/${name}-${TARGET}.gz"
    echo "→ Downloading latest ${name} for ${TARGET}..."
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${name}-${TARGET}.gz"
    echo "→ Downloading ${name} ${VERSION} for ${TARGET}..."
  fi
  if curl -fsSL "$url" | gunzip > "${DEST}/${name}"; then
    chmod +x "${DEST}/${name}"
    echo "  ✓ ${name} installed to ${DEST}/${name}"
  else
    echo "  ⚠ ${name} download failed, skipping"
  fi
}

install_bin "xei"

if [ "$HAS_DESKTOP" = "1" ]; then
  install_bin "suisei" || true
fi

echo ""
echo "Installed. Make sure ${DEST} is in your PATH."
command -v xei >/dev/null 2>&1 && xei --version || echo "  Run: export PATH=\"\${HOME}/.local/bin:\$PATH\""
