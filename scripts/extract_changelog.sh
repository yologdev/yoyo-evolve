#!/usr/bin/env bash
# Extract changelog section for a specific version tag from CHANGELOG.md
# Usage: ./scripts/extract_changelog.sh v0.1.5
set -euo pipefail

TAG="${1:?Usage: extract_changelog.sh <tag>}"
VERSION="${TAG#v}"

CHANGELOG="$(dirname "$0")/../CHANGELOG.md"

if [ ! -f "$CHANGELOG" ]; then
  echo "Error: CHANGELOG.md not found" >&2
  exit 1
fi

# Extract everything between ## [VERSION] and the next ## [ heading
BODY=$(awk -v ver="$VERSION" '
  /^## \[/ {
    if (found) exit
    if (index($0, "[" ver "]")) { found=1; next }
  }
  found { print }
' "$CHANGELOG")

if [ -z "$BODY" ]; then
  echo "Error: Version $VERSION not found in CHANGELOG.md" >&2
  exit 1
fi

echo "$BODY"
