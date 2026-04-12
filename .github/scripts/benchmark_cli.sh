#!/usr/bin/env bash
set -euo pipefail

WARMUP_RUNS=1
BENCHMARK_RUNS=6

SCENARIO_IDS=(baseline history_x10)
SCENARIO_NAMES=("Baseline fixture" "Large history fixture")
SCENARIO_PACKAGES=(20 200)
SCENARIO_CHANGESETS=(50 500)
SCENARIO_COMMITS=(50 500)
COMMAND_LABELS=(
	"mc validate"
	"mc discover --format json"
	"mc release --dry-run"
	"mc release"
)
COMMAND_ARGS=(
	"validate"
	"discover --format json"
	"release --dry-run"
	"release"
)

render_comment() {
	local output_path="$1"
	shift

	{
		echo "## Binary Benchmark: main vs PR"
		echo
		echo "Measured with \`hyperfine --warmup ${WARMUP_RUNS} --runs ${BENCHMARK_RUNS}\`."
		echo
		echo "Commands:"
		local label
		for label in "${COMMAND_LABELS[@]}"; do
			echo "- \`${label}\`"
		done

		local index=0
		while [ "$#" -gt 0 ]; do
			local scenario_name="$1"
			local scenario_description="$2"
			local table_path="$3"
			shift 3

			echo
			echo "### ${scenario_name}"
			echo
			echo "Fixture: ${scenario_description}"
			echo
			cat "$table_path"
		done
	} >"$output_path"
}

git_commit() {
	local root="$1"
	shift
	git -C "$root" \
		-c user.name=bench \
		-c user.email=bench@test \
		commit -m "$1" >/dev/null
}

generate_fixture() {
	local root="$1"
	local packages="$2"
	local changesets="$3"
	local commits="$4"

	mkdir -p "$root/crates" "$root/.changeset"

	{
		echo '[workspace]'
		echo 'members = ['
		local i
		for i in $(seq 0 $((packages - 1))); do
			echo "  \"crates/pkg-${i}\","
		done
		echo ']'
		echo 'resolver = "2"'
	} >"$root/Cargo.toml"

	{
		echo '[defaults]'
		echo 'package_type = "cargo"'
		echo
		local i
		for i in $(seq 0 $((packages - 1))); do
			echo "[package.pkg-${i}]"
			echo "path = \"crates/pkg-${i}\""
			echo
			mkdir -p "$root/crates/pkg-${i}"
			{
				echo '[package]'
				echo "name = \"pkg-${i}\""
				echo 'version = "1.0.0"'
				echo 'edition = "2021"'
			} >"$root/crates/pkg-${i}/Cargo.toml"
		done
		echo '[ecosystems.cargo]'
		echo 'enabled = true'
	} >"$root/monochange.toml"

	git -C "$root" init -b main >/dev/null
	git -C "$root" add .
	git_commit "$root" initial

	local commit_index
	for commit_index in $(seq 0 $((commits - 1))); do
		local package_index=$((commit_index % packages))
		printf '// commit %d\n' "$commit_index" >"$root/crates/pkg-${package_index}/src.rs"

		if [ "$commit_index" -lt "$changesets" ]; then
			cat >"$root/.changeset/change-$(printf '%04d' "$commit_index").md" <<EOF
---
pkg-${package_index}: patch
---

Fix issue #${commit_index}.
EOF
		fi

		git -C "$root" add .
		git_commit "$root" "change ${commit_index}"
	done

	if [ "$changesets" -gt "$commits" ]; then
		local changeset_index
		for changeset_index in $(seq "$commits" $((changesets - 1))); do
			local package_index=$((changeset_index % packages))
			cat >"$root/.changeset/change-$(printf '%04d' "$changeset_index").md" <<EOF
---
pkg-${package_index}: patch
---

Fix issue #${changeset_index}.
EOF
			git -C "$root" add .
			git_commit "$root" "changeset ${changeset_index}"
		done
	fi
}

run_scenario() {
	local main_bin="$1"
	local pr_bin="$2"
	local scenario_name="$3"
	local packages="$4"
	local changesets="$5"
	local commits="$6"
	local table_path="$7"

	local fixture_dir
	fixture_dir="$(mktemp -d -t monochange-bench.XXXXXX)"
	trap "rm -rf '$fixture_dir'" RETURN

	generate_fixture "$fixture_dir" "$packages" "$changesets" "$commits"

	local hyperfine_args=()
	local idx
	for idx in "${!COMMAND_LABELS[@]}"; do
		hyperfine_args+=(--command-name "main · ${COMMAND_LABELS[$idx]}" "${main_bin} ${COMMAND_ARGS[$idx]}")
		hyperfine_args+=(--command-name "pr · ${COMMAND_LABELS[$idx]}" "${pr_bin} ${COMMAND_ARGS[$idx]}")
	done

	(
		cd "$fixture_dir"
		hyperfine \
			--prepare "git reset --hard HEAD >/dev/null && git clean -fd >/dev/null" \
			--style basic \
			--warmup "$WARMUP_RUNS" \
			--runs "$BENCHMARK_RUNS" \
			--time-unit millisecond \
			--export-markdown "$table_path" \
			"${hyperfine_args[@]}"
	)
}

run_mode() {
	local main_bin=""
	local pr_bin=""
	local output_path=""

	while [ "$#" -gt 0 ]; do
		case "$1" in
		--main-bin)
			main_bin="$2"
			shift 2
			;;
		--pr-bin)
			pr_bin="$2"
			shift 2
			;;
		--output)
			output_path="$2"
			shift 2
			;;
		*)
			echo "unknown argument: $1" >&2
			exit 1
			;;
		esac
	done

	local scenario_render_args=()
	local idx
	for idx in "${!SCENARIO_IDS[@]}"; do
		local table_path
		table_path="$(mktemp -t monochange-bench-table.XXXXXX.md)"
		run_scenario \
			"$main_bin" \
			"$pr_bin" \
			"${SCENARIO_NAMES[$idx]}" \
			"${SCENARIO_PACKAGES[$idx]}" \
			"${SCENARIO_CHANGESETS[$idx]}" \
			"${SCENARIO_COMMITS[$idx]}" \
			"$table_path"
		scenario_render_args+=(
			"${SCENARIO_NAMES[$idx]}"
			"${SCENARIO_PACKAGES[$idx]} packages, ${SCENARIO_CHANGESETS[$idx]} changesets, ${SCENARIO_COMMITS[$idx]} commits"
			"$table_path"
		)
	done

	render_comment "$output_path" "${scenario_render_args[@]}"
}

render_fixture_mode() {
	local fixture_dir=""
	local output_path=""

	while [ "$#" -gt 0 ]; do
		case "$1" in
		--fixture-dir)
			fixture_dir="$2"
			shift 2
			;;
		--output)
			output_path="$2"
			shift 2
			;;
		*)
			echo "unknown argument: $1" >&2
			exit 1
			;;
		esac
	done

	render_comment \
		"$output_path" \
		"Baseline fixture" \
		"20 packages, 50 changesets, 50 commits" \
		"$fixture_dir/baseline.md" \
		"Large history fixture" \
		"200 packages, 500 changesets, 500 commits" \
		"$fixture_dir/history_x10.md"
}

main() {
	local mode="${1:-}"
	shift || true

	case "$mode" in
	run) run_mode "$@" ;;
	render-fixture) render_fixture_mode "$@" ;;
	*)
		echo "usage: $0 <run|render-fixture> [args...]" >&2
		exit 1
		;;
	esac
}

main "$@"
