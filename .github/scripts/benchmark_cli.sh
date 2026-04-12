#!/usr/bin/env bash
set -euo pipefail

WARMUP_RUNS=1
BENCHMARK_RUNS=6
PHASE_COMMAND_LABELS=(
	"mc release --dry-run"
	"mc release"
)
PHASE_COMMAND_ARGS=(
	"release --dry-run"
	"release"
)
PHASE_BUDGETS_FILE="$(cd "$(dirname "$0")" && pwd)/benchmark_phase_budgets.json"

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
			local phase_table_path="$4"
			shift 4

			echo
			echo "### ${scenario_name}"
			echo
			echo "Fixture: ${scenario_description}"
			echo
			cat "$table_path"
			if [ -f "$phase_table_path" ] && [ -s "$phase_table_path" ]; then
				echo
				cat "$phase_table_path"
			fi
		done
	} >"$output_path"
}

summarize_progress_events() {
	local events_path="$1"
	local output_path="$2"
	python3 - "$events_path" >"$output_path" <<'PY'
import json
import sys

events_path = sys.argv[1]
phase_totals = {}
step_total_ms = 0

with open(events_path, encoding="utf-8") as handle:
    for raw_line in handle:
        line = raw_line.strip()
        if not line:
            continue
        event = json.loads(line)
        if event.get("event") != "step_finished" or event.get("stepKind") != "PrepareRelease":
            continue
        step_total_ms += int(event.get("durationMs", 0) or 0)
        for phase in event.get("phaseTimings", []):
            label = phase.get("label")
            if not label:
                continue
            phase_totals[label] = phase_totals.get(label, 0) + int(
                phase.get("durationMs", 0) or 0
            )

summary = {
    "stepTotalMs": step_total_ms,
    "phases": [
        {"label": label, "durationMs": duration_ms}
        for label, duration_ms in sorted(
            phase_totals.items(), key=lambda item: (-item[1], item[0])
        )
    ],
}
json.dump(summary, sys.stdout, indent=2, sort_keys=True)
PY
}

run_phase_capture() {
	local bin="$1"
	local fixture_dir="$2"
	local command_args="$3"
	local events_path="$4"
	local -a argv=()
	read -r -a argv <<<"$command_args"
	(
		cd "$fixture_dir"
		git reset --hard HEAD >/dev/null
		git clean -fd >/dev/null
		"$bin" --progress-format json "${argv[@]}" >/dev/null 2>"$events_path"
	)
}

render_phase_markdown() {
	local scenario_id="$1"
	local output_path="$2"
	local violations_path="$3"
	local dry_main_summary="$4"
	local dry_pr_summary="$5"
	local release_main_summary="$6"
	local release_pr_summary="$7"

	python3 - \
		"$scenario_id" \
		"$PHASE_BUDGETS_FILE" \
		"$dry_main_summary" \
		"$dry_pr_summary" \
		"$release_main_summary" \
		"$release_pr_summary" \
		"$violations_path" >"$output_path" <<'PY'
import json
import sys

(
    scenario_id,
    budgets_path,
    dry_main_summary,
    dry_pr_summary,
    release_main_summary,
    release_pr_summary,
    violations_path,
) = sys.argv[1:]

with open(budgets_path, encoding="utf-8") as handle:
    scenario_budgets = json.load(handle).get(scenario_id, {})

command_summaries = {
    "mc release --dry-run": {
        "main": json.load(open(dry_main_summary, encoding="utf-8")),
        "pr": json.load(open(dry_pr_summary, encoding="utf-8")),
    },
    "mc release": {
        "main": json.load(open(release_main_summary, encoding="utf-8")),
        "pr": json.load(open(release_pr_summary, encoding="utf-8")),
    },
}

def phase_map(summary):
    return {phase["label"]: int(phase["durationMs"]) for phase in summary.get("phases", [])}

def status_label(main_ms, pr_ms, budget_ms):
    if budget_ms is not None and pr_ms > budget_ms:
        return "over budget"
    if pr_ms > main_ms:
        return "regressed"
    if pr_ms < main_ms:
        return "improved"
    return "flat"

def delta(pr_ms, main_ms):
    value = pr_ms - main_ms
    return f"{value:+d}"

sections = ["#### Phase timings", ""]
violations = 0

for command_label in ("mc release --dry-run", "mc release"):
    summaries = command_summaries[command_label]
    budget = scenario_budgets.get(command_label, {})
    phase_budget = budget.get("phases", {})
    main_summary = summaries["main"]
    pr_summary = summaries["pr"]
    main_phases = phase_map(main_summary)
    pr_phases = phase_map(pr_summary)
    rows = [
        (
            "prepare release total",
            budget.get("stepTotalMs"),
            int(main_summary.get("stepTotalMs", 0) or 0),
            int(pr_summary.get("stepTotalMs", 0) or 0),
        )
    ]
    labels = sorted(
        set(main_phases) | set(pr_phases),
        key=lambda label: (-max(main_phases.get(label, 0), pr_phases.get(label, 0)), label),
    )
    for label in labels:
        rows.append((label, phase_budget.get(label), main_phases.get(label, 0), pr_phases.get(label, 0)))

    sections.append(f"##### `{command_label}`")
    sections.append("")
    sections.append("| Phase | Budget [ms] | main [ms] | pr [ms] | Δ pr-main [ms] | Status |")
    sections.append("|:---|---:|---:|---:|---:|:---|")
    for label, budget_ms, main_ms, pr_ms in rows:
        status = status_label(main_ms, pr_ms, budget_ms)
        if budget_ms is not None and pr_ms > budget_ms:
            violations += 1
        budget_text = "n/a" if budget_ms is None else str(int(budget_ms))
        sections.append(
            f"| `{label}` | {budget_text} | {main_ms} | {pr_ms} | {delta(pr_ms, main_ms)} | {status} |"
        )
    sections.append("")

with open(violations_path, "w", encoding="utf-8") as handle:
    handle.write(str(violations))

sys.stdout.write("\n".join(sections).rstrip() + "\n")
PY
}

collect_phase_markdown() {
	local scenario_id="$1"
	local fixture_dir="$2"
	local main_bin="$3"
	local pr_bin="$4"
	local phase_markdown_path="$5"
	local scenario_violations_path="$6"
	local dry_main_events
	local dry_pr_events
	local release_main_events
	local release_pr_events
	local dry_main_summary
	local dry_pr_summary
	local release_main_summary
	local release_pr_summary
	dry_main_events="$(mktemp -t monochange-bench-dry-main.XXXXXX.jsonl)"
	dry_pr_events="$(mktemp -t monochange-bench-dry-pr.XXXXXX.jsonl)"
	release_main_events="$(mktemp -t monochange-bench-release-main.XXXXXX.jsonl)"
	release_pr_events="$(mktemp -t monochange-bench-release-pr.XXXXXX.jsonl)"
	dry_main_summary="$(mktemp -t monochange-bench-dry-main-summary.XXXXXX.json)"
	dry_pr_summary="$(mktemp -t monochange-bench-dry-pr-summary.XXXXXX.json)"
	release_main_summary="$(mktemp -t monochange-bench-release-main-summary.XXXXXX.json)"
	release_pr_summary="$(mktemp -t monochange-bench-release-pr-summary.XXXXXX.json)"
	run_phase_capture "$main_bin" "$fixture_dir" "${PHASE_COMMAND_ARGS[0]}" "$dry_main_events"
	run_phase_capture "$pr_bin" "$fixture_dir" "${PHASE_COMMAND_ARGS[0]}" "$dry_pr_events"
	run_phase_capture "$main_bin" "$fixture_dir" "${PHASE_COMMAND_ARGS[1]}" "$release_main_events"
	run_phase_capture "$pr_bin" "$fixture_dir" "${PHASE_COMMAND_ARGS[1]}" "$release_pr_events"
	summarize_progress_events "$dry_main_events" "$dry_main_summary"
	summarize_progress_events "$dry_pr_events" "$dry_pr_summary"
	summarize_progress_events "$release_main_events" "$release_main_summary"
	summarize_progress_events "$release_pr_events" "$release_pr_summary"
	render_phase_markdown \
		"$scenario_id" \
		"$phase_markdown_path" \
		"$scenario_violations_path" \
		"$dry_main_summary" \
		"$dry_pr_summary" \
		"$release_main_summary" \
		"$release_pr_summary"
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
	local fixture_dir="$3"
	local table_path="$4"

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
	local violations_output=""

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
		--violations-output)
			violations_output="$2"
			shift 2
			;;
		*)
			echo "unknown argument: $1" >&2
			exit 1
			;;
		esac
	done

	local scenario_render_args=()
	local total_violations=0
	local idx
	for idx in "${!SCENARIO_IDS[@]}"; do
		local table_path
		local phase_path
		local scenario_violations_path
		table_path="$(mktemp -t monochange-bench-table.XXXXXX.md)"
		phase_path="$(mktemp -t monochange-bench-phases.XXXXXX.md)"
		scenario_violations_path="$(mktemp -t monochange-bench-violations.XXXXXX.txt)"
		local fixture_dir
		fixture_dir="$(mktemp -d -t monochange-bench.XXXXXX)"
		generate_fixture \
			"$fixture_dir" \
			"${SCENARIO_PACKAGES[$idx]}" \
			"${SCENARIO_CHANGESETS[$idx]}" \
			"${SCENARIO_COMMITS[$idx]}"
		run_scenario \
			"$main_bin" \
			"$pr_bin" \
			"$fixture_dir" \
			"$table_path"
		collect_phase_markdown \
			"${SCENARIO_IDS[$idx]}" \
			"$fixture_dir" \
			"$main_bin" \
			"$pr_bin" \
			"$phase_path" \
			"$scenario_violations_path"
		total_violations=$((total_violations + $(cat "$scenario_violations_path")))
		rm -rf "$fixture_dir"
		scenario_render_args+=(
			"${SCENARIO_NAMES[$idx]}"
			"${SCENARIO_PACKAGES[$idx]} packages, ${SCENARIO_CHANGESETS[$idx]} changesets, ${SCENARIO_COMMITS[$idx]} commits"
			"$table_path"
			"$phase_path"
		)
	done

	render_comment "$output_path" "${scenario_render_args[@]}"
	if [ -n "$violations_output" ]; then
		printf '%s\n' "$total_violations" >"$violations_output"
	fi
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
		"$fixture_dir/baseline-phases.md" \
		"Large history fixture" \
		"200 packages, 500 changesets, 500 commits" \
		"$fixture_dir/history_x10.md" \
		"$fixture_dir/history_x10-phases.md"
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
