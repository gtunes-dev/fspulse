#!/bin/bash

# FsPulse Release Script
# -----------------------
# This script updates the version in Cargo.toml, commits it,
# tags the commit, and pushes both the tag and the main branch.
# This triggers a GitHub CI release build and prepares for a crates.io publish.

set -euo pipefail

# Require exactly one argument: the version number (e.g., 0.0.3)
if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version> (e.g. 0.0.3)"
  exit 1
fi

VERSION="$1"
TAG="v$VERSION"

# Confirm with the user before proceeding
read -p "This will tag and push version $VERSION. Continue? [y/N] " confirm
if [[ "$confirm" != "y" && "$confirm" != "Y" ]]; then
  echo "Aborted."
  exit 1
fi

# Update version in Cargo.toml
echo "Updating Cargo.toml to version $VERSION..."
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Stage and commit the version bump
git add Cargo.toml
git commit -m "Bump version to $VERSION"

# Create the version tag
echo "Tagging $TAG..."
git tag "$TAG"

# Push commit and tag to origin
git push origin main
git push origin "$TAG"

# Print next steps for crates.io
echo "✅ Tag pushed. GitHub CI should now build and upload release binaries."
echo "💡 When you're ready to publish to crates.io, run:"
echo "   cargo publish --token <your-token>"