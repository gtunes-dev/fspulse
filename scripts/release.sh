#!/bin/bash

# FsPulse Release Script
# -----------------------
# This script updates the version in Cargo.toml, commits it,
# tags the commit, and pushes both the tag and the main branch.
# This triggers a GitHub CI release build and prepares for a crates.io publish.

set -euo pipefail

# Require exactly one argument: the version number (e.g., 0.0.6)
if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version> (e.g. 0.0.6)"
  exit 1
fi

VERSION="$1"
TAG="v$VERSION"

# Confirm with the user before proceeding
read -p "This will tag and release version $VERSION. Continue? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

# Verify changelog contains this version
if ! grep -q "## \[v$VERSION\]" CHANGELOG.md; then
  echo "❌ ERROR: CHANGELOG.md does not contain entry for version v$VERSION"
  echo "Please add a section like '## [v$VERSION] - YYYY-MM-DD' before releasing."
  exit 1
fi

echo
echo "✅ Found changelog entry for v$VERSION"
echo
echo "🔍 Changelog preview:"
awk "/## \[v$VERSION\]/,/^## \[v/" CHANGELOG.md | head -n -1 || true
echo

# Update version in Cargo.toml
echo "📦 Updating Cargo.toml to version $VERSION..."
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Ensure Cargo.lock is updated
echo "🔧 Running cargo check to update Cargo.lock if needed..."
cargo check > /dev/null

# Stage and commit version and lockfile
git add Cargo.toml Cargo.lock
git commit -m "Release version $VERSION"

# Create and push the tag
echo "🏷️ Tagging $TAG..."
git tag "$TAG"
git push origin main
git push origin "$TAG"

echo "✅ Release $VERSION pushed. GitHub Actions should now build and publish the release."
echo "📦 When ready, run:"
echo "   cargo publish --token <your-token>"