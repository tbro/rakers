#!/usr/bin/env bash
# Bump the crate version, commit, and tag.
#
# Usage: scripts/bump_version.sh <version>
#   version may include a leading 'v' (e.g. v0.2.0 or 0.2.0)

set -euo pipefail

if [ -z "${1:-}" ]; then
    echo "usage: $0 <version>"
    exit 1
fi

ver="${1#v}"
tag="v${ver}"
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

if [ "$ver" = "$current" ]; then
    echo "error: Cargo.toml is already at version $current"
    exit 1
fi

sed -i "s/^version = \"$current\"/version = \"$ver\"/" Cargo.toml
cargo update --workspace -q
git add Cargo.toml Cargo.lock
git commit -m "Bump version to $ver"
git tag "$tag"
echo "Bumped $current → $ver and tagged $tag"
echo "Push with: git push && git push origin $tag"
