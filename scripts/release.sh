#!/bin/bash

# FsPulse Release Script
# -----------------------
# This script updates the version in Cargo.toml, commits it,
# tags the commit, and pushes both the tag and the main branch.
# This triggers a GitHub CI release build and prepares for a crates.io publish.

set -euo pipefail

# Cleanup function for rollback on failure
cleanup() {
  if [[ -n "${TAG:-}" ]] && git rev-parse "$TAG" >/dev/null 2>&1 && [[ "${PUSHED:-}" != "true" ]]; then
    echo "🧹 Cleaning up: removing tag $TAG"
    git tag -d "$TAG" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# Require exactly one argument: the version number (e.g., 0.0.6)
if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version> (e.g. 0.0.6)"
  exit 1
fi

VERSION="$1"
TAG="v$VERSION"
PUSHED="false"

# Verify we're on the main branch
current_branch=$(git rev-parse --abbrev-ref HEAD)
if [[ "$current_branch" != "main" ]]; then
  echo "❌ ERROR: Must be on 'main' branch. Currently on '$current_branch'."
  exit 1
fi

# Verify working tree is clean
if [[ -n $(git status --porcelain) ]]; then
  echo "❌ ERROR: Working tree is not clean. Commit or stash changes first."
  git status --short
  exit 1
fi

# Verify we're synced with remote
echo "🔍 Checking sync with remote..."
git fetch origin main
local_commit=$(git rev-parse HEAD)
remote_commit=$(git rev-parse origin/main)
if [[ "$local_commit" != "$remote_commit" ]]; then
  echo "❌ ERROR: Local main is not synced with origin/main."
  echo "   Local:  $local_commit"
  echo "   Remote: $remote_commit"
  echo "Please push/pull changes first."
  exit 1
fi

# Verify tag doesn't already exist
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "❌ ERROR: Tag $TAG already exists."
  echo "If you want to re-release, delete the tag first:"
  echo "   git tag -d $TAG"
  echo "   git push origin :refs/tags/$TAG"
  exit 1
fi

# Verify changelog contains this version
if ! grep -Eq "^## \\[v$VERSION\\]" CHANGELOG.md; then
  echo "❌ ERROR: CHANGELOG.md does not contain entry for version v$VERSION"
  echo "Please add a section like '## [v$VERSION] - YYYY-MM-DD' before releasing."
  exit 1
fi

# Confirm with the user before proceeding
echo
echo "✅ All pre-flight checks passed"
echo
echo "🔍 Changelog preview:"
awk "/^## \[v$VERSION\]/ {found=1; print; next} /^## \[v/ && found {exit} found" CHANGELOG.md
echo
read -p "This will tag and release version $VERSION. Continue? [y/N] " confirm
if [[ ! "$confirm" =~ ^[Yy]$ ]]; then
  echo "Aborted."
  exit 0
fi

# Update version in Cargo.toml (cross-platform sed)
echo "📦 Updating Cargo.toml to version $VERSION..."
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml && rm Cargo.toml.bak

# Ensure Cargo.lock is updated
echo "🔧 Running cargo check to update Cargo.lock..."
if ! cargo check; then
  echo "❌ ERROR: cargo check failed. Fix errors before releasing."
  git restore Cargo.toml
  exit 1
fi

# Stage and commit version and lockfile
git add Cargo.toml Cargo.lock
git commit -m "Release version $VERSION"

# Create the tag
echo "🏷️  Creating tag $TAG..."
git tag "$TAG"

# Push both main and tag atomically (safer)
echo "🚀 Pushing to GitHub..."
if git push --atomic origin main "$TAG"; then
  PUSHED="true"
  echo "✅ Release $VERSION pushed successfully!"
else
  echo "❌ ERROR: Push failed. Removing local tag..."
  git reset --hard HEAD~1
  git tag -d "$TAG"
  exit 1
fi

echo
echo "✅ GitHub Actions should now build and publish the release."
echo "📦 Monitor the build at: https://github.com/gtunes-dev/fspulse/actions"
echo
echo "When the build completes and you've verified the release, publish to crates.io:"
echo "   cargo publish"
