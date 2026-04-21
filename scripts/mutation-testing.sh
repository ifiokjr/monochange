#!/usr/bin/env bash
# Run mutation testing across all workspace crates with per-crate timeouts.
# Heavy crates (monochange_core, monochange_config) get longer timeouts.
#
# Usage:
#   ./scripts/mutation-testing.sh [crate-name]
#   ./scripts/mutation-testing.sh --all

set -euo pipefail

REPORT_DIR="${MUTANTS_REPORT_DIR:-mutants-report}"
mkdir -p "$REPORT_DIR"

CRATES=(
	monochange_semver
	monochange_core
	monochange_graph
	monochange_config
	monochange_analysis
	monochange_cargo
	monochange_hosting
	monochange_lint
	monochange_test_helpers
	monochange_npm
	monochange_dart
	monochange_deno
	monochange_ecmascript
	monochange_gitea
	monochange_github
	monochange_gitlab
	monochange_linting
	monochange
)

# Crates with many mutants need more time.
get_timeout() {
	local crate="$1"
	case "$crate" in
	monochange_core) echo "1800" ;;    # 30 minutes
	monochange_config) echo "1800" ;;  # 30 minutes
	monochange) echo "1200" ;;         # 20 minutes
	monochange_analysis) echo "900" ;; # 15 minutes
	*) echo "600" ;;                   # 10 minutes
	esac
}

run_mutants() {
	local crate="$1"
	local timeout_seconds
	timeout_seconds=$(get_timeout "$crate")

	echo "=== Running mutants for $crate (timeout: ${timeout_seconds}s) ==="
	if timeout "$timeout_seconds" cargo mutants -p "$crate" --no-shuffle --output "$REPORT_DIR/$crate" 2>&1 | tee -a "$REPORT_DIR/$crate.out"; then
		echo "ok       $crate"
	else
		local exit_code=$?
		if [ "$exit_code" -eq 124 ]; then
			echo "TIMEOUT  $crate (exceeded ${timeout_seconds}s)"
		else
			echo "FAIL     $crate (exit $exit_code)"
		fi
	fi
	echo ""
}

if [ "${1:-}" = "--all" ] || [ -z "${1:-}" ]; then
	for crate in "${CRATES[@]}"; do
		run_mutants "$crate"
	done
else
	run_mutants "$1"
fi

# Generate a summary report.
echo "=== Mutation Testing Summary ===" | tee "$REPORT_DIR/summary.txt"
for crate in "${CRATES[@]}"; do
	out="$REPORT_DIR/$crate.out"
	if [ -f "$out" ]; then
		caught=$(grep -c "^ok.*caught" "$out" 2>/dev/null || true)
		missed=$(grep -c "^MISSED" "$out" 2>/dev/null || true)
		timeouts=$(grep -c "^TIMEOUT" "$out" 2>/dev/null || true)
		echo "$crate: $missed missed, $caught caught, $timeouts timeouts" | tee -a "$REPORT_DIR/summary.txt"
	else
		echo "$crate: no report" | tee -a "$REPORT_DIR/summary.txt"
	fi
done
