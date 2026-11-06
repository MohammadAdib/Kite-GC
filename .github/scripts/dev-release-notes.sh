#!/usr/bin/env bash
# Release notes for the rolling `development-build` pre-release (dev-build.yml): what this build has
# over the previous development build, read from the `development-build` tag before it is replaced.
#
#   dev-release-notes.sh > notes.md
#
# First-parent history, so an upstream sync reads as its one merge commit instead of every upstream
# commit it brought in. Needs the full commit graph and tags (checkout with fetch-depth: 0). Run
# locally (after `git fetch --tags`) to preview the next build's notes.
set -euo pipefail

repo_url="${GITHUB_SERVER_URL:-https://github.com}/${GITHUB_REPOSITORY:-MohammadAdib/Kite-GC}"
max=100

head=$(git rev-parse HEAD)
version=$(node -p "require('./package.json').version")
prev=$(git rev-parse -q --verify 'refs/tags/development-build^{commit}' || true)

echo "Kite Ground Control **$version**, built from \`development\` at [\`${head:0:8}\`]($repo_url/commit/$head) ($(git log -1 --format=%cs HEAD))."
echo
echo "An automatic pre-release of the latest \`development\` commit, replaced by every push. Not a tested release."
echo

if [ -n "$prev" ] && [ "$prev" = "$head" ]; then
  echo "## Changes"
  echo
  echo "Rebuild of the previous development build's commit; nothing new."
  range=""
elif [ -n "$prev" ] && git merge-base --is-ancestor "$prev" "$head"; then
  echo "## Changes since the previous development build"
  echo
  echo "[Full diff]($repo_url/compare/${prev:0:12}...${head:0:12})"
  echo
  range="$prev..$head"
else
  # First build, or history rewritten since the last one: show the recent history instead.
  echo "## Recent changes"
  echo
  range="$head"
  max=30
fi

if [ -n "$range" ]; then
  total=$(git rev-list --count --first-parent "$range")
  git log --first-parent -n "$max" --format="- %s ([\`%h\`]($repo_url/commit/%H))" "$range"
  if [ "$range" != "$head" ] && [ "$total" -gt "$max" ]; then
    echo "- …and $((total - max)) more"
  fi
fi

echo
echo "## Downloads"
echo
echo "- **Windows**: \`*_installer.exe\` (installer) or \`*_portable.zip\`."
echo "- **macOS** (universal, Apple Silicon + Intel): \`*.dmg\`. Unsigned: on first launch right-click → Open, or run \`xattr -dr com.apple.quarantine \"/Applications/Kite Ground Control.app\"\`."
echo "- **Linux** x64 and ARM64: \`.deb\`, \`.rpm\` or \`.AppImage\`."
