#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"

if [[ -n "${MONOCHANGE_BIN:-}" ]]; then
	mc_cmd=("$MONOCHANGE_BIN")
elif [[ -x "$repo_root/target/debug/mc" ]]; then
	mc_cmd=("$repo_root/target/debug/mc")
else
	mc_cmd=(cargo run -p monochange --bin mc --manifest-path "$repo_root/Cargo.toml" --)
fi

examples=(
	github-cargo-quickstart
	github-npm-quickstart
	gitlab-migration
	internal-only-workspace
	mixed-workspace
	public-packages-placeholder-publish
	release-pr-workflow
)

for example in "${examples[@]}"; do
	example_dir="$script_dir/$example"
	echo "== validating $example =="
	(
		cd "$example_dir"
		"${mc_cmd[@]}" validate
		"${mc_cmd[@]}" check
		"${mc_cmd[@]}" release --dry-run --diff >/dev/null
	)
done

echo "All repo-shaped examples passed validate/check/release --dry-run --diff."
