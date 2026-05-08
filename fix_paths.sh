#!/bin/bash
set -e
cd "$(dirname "$0")/crates/monochange/src"

for f in cli.rs cli_help.rs cli_runtime.rs __tests.rs git_support.rs prepared_release_cache.rs monochange.toml.template; do
  if [ -f "$f" ]; then
    sed -i 's|\.monochange/readiness\.json|\.monochange/local/readiness.json|g' "$f"
    sed -i 's|\.monochange/bootstrap-result\.json|\.monochange/local/bootstrap-result.json|g' "$f"
    sed -i 's|\.monochange/release-manifest\.json|\.monochange/local/release-manifest.json|g' "$f"
    sed -i 's|\.monochange/release\.json|\.monochange/local/release.json|g' "$f"
    sed -i 's|\.monochange/prepared-release\.json|\.monochange/local/prepared-release.json|g' "$f"
    sed -i 's|\.monochange/previous-result\.json|\.monochange/local/previous-result.json|g' "$f"
    sed -i 's|\.monochange/publish-result\.json|\.monochange/local/publish-result.json|g' "$f"
    sed -i 's|\.monochange/cache\.json|\.monochange/local/cache.json|g' "$f"
    sed -i 's|\.monochange/unit-prepared-release\.json|\.monochange/local/unit-prepared-release.json|g' "$f"
    sed -i 's|\.monochange/custom\.json|\.monochange/local/custom.json|g' "$f"
    sed -i 's|\.monochange/write-error|\.monochange/local/write-error|g' "$f"
  fi
done

# Fix .gitignore content in git_support.rs and __tests.rs
for f in git_support.rs __tests.rs; do
  if [ -f "$f" ]; then
    sed -i 's|\.monochange/\\n|\.monochange/local/\\n|g' "$f"
  fi
done

echo "Done"
