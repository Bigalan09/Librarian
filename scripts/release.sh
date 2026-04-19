#!/usr/bin/env bash
# Usage: ./scripts/release.sh [patch|minor|major]
# Triggers the release workflow on GitHub which bumps version, builds, and publishes.
set -euo pipefail

BUMP="${1:-patch}"

if [[ "$BUMP" != "patch" && "$BUMP" != "minor" && "$BUMP" != "major" ]]; then
  echo "Usage: $0 [patch|minor|major]"
  exit 1
fi

CURRENT=$(grep -m1 '^version' Cargo.toml | sed 's/.*"\(.*\)"/\1/')
echo "Current version: $CURRENT"
echo "Triggering $BUMP release via GitHub Actions..."

gh workflow run release.yml -f bump="$BUMP"

echo "Release workflow started. Track progress at:"
echo "  https://github.com/$(gh repo view --json nameWithOwner -q .nameWithOwner)/actions"
