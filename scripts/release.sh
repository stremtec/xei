#!/usr/bin/env bash
# One-command version bump for xei.
#
# The single source of truth is [workspace.package] version in the root
# Cargo.toml. This script syncs the two places that Cargo/npm require a literal
# version — the xei-core path-dep pin (for `cargo publish`) and npm's
# package.json — then refreshes Cargo.lock. The install scripts
# (install.js/sh/ps1) resolve the version dynamically and are NEVER bumped.
#
#   scripts/release.sh 3.0.9          # bump manifests only
#   scripts/release.sh 3.0.9 --tag    # + commit + tag v3.0.9
#
# After --tag:  git push origin master && git push origin v3.0.9
# (the v* tag triggers .github/workflows/release.yml → 5 target binaries).
set -euo pipefail

V="${1:-}"
[ -z "$V" ] && { echo "usage: scripts/release.sh <version> [--tag]" >&2; exit 1; }
V="${V#v}"
printf '%s' "$V" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || { echo "error: version must be x.y.z (got '$V')" >&2; exit 1; }

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

# GNU sed vs BSD (macOS) sed in-place syntax.
sedi() { if sed --version >/dev/null 2>&1; then sed -i "$@"; else sed -i '' "$@"; fi; }

# 1. Workspace version — the single source.
sedi -E "s/^version = \"[0-9]+\.[0-9]+\.[0-9]+\"/version = \"$V\"/" Cargo.toml
# 2. xei-core path-dep pin (cargo publish requires a literal version).
sedi -E "s#(xei-core = \{ path = \"\.\./xei-core\", version = )\"[0-9]+\.[0-9]+\.[0-9]+\"#\1\"$V\"#" xei/Cargo.toml
# 3. npm manifest.
( cd xei && npm version "$V" --no-git-tag-version --allow-same-version >/dev/null )
# 4. Refresh Cargo.lock with the new versions.
cargo build -p xei-core -p xei-editor >/dev/null 2>&1 || true

echo "Bumped → $V"
grep -nE '^version = ' Cargo.toml
grep -n 'xei-core = { path' xei/Cargo.toml
grep '"version"' xei/package.json

if [ "${2:-}" = "--tag" ]; then
  git add -A
  git commit -m "v$V"
  git tag "v$V"
  echo "Committed + tagged v$V — push: git push origin master && git push origin v$V"
fi
