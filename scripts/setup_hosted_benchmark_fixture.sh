#!/usr/bin/env bash
set -euo pipefail

PACKAGE_COUNT=8
FILLER_COMMITS=220
RELEASE_PRS=6
COMMITS_PER_PR=2
LOCAL_ONLY=false
VISIBILITY="public"
OUTPUT_DIR=""
OWNER="fixture"
REPO="monochange-release-benchmark-fixture"

usage() {
	cat <<'EOF' >&2
usage: scripts/setup_hosted_benchmark_fixture.sh [args...]

Creates a repeatable monochange benchmark fixture repository with:
- multiple cargo workspace packages
- more than 200 commits by default
- release changesets introduced through pull-request-like branches

Options:
  --output-dir <dir>        required destination directory
  --owner <owner>           hosted repo owner (default: fixture)
  --repo <repo>             hosted repo name (default: monochange-release-benchmark-fixture)
  --package-count <count>   workspace package count (default: 8)
  --filler-commits <count>  direct-history commits before release PRs (default: 220)
  --release-prs <count>     number of release PR branches to seed (default: 6)
  --commits-per-pr <count>  non-changeset commits per PR branch (default: 2)
  --local-only              build the fixture locally without GitHub repo creation
  --private                 create the hosted repo as private instead of public
EOF
	exit 1
}

git_commit() {
	local root="$1"
	local message="$2"
	env -u GIT_DIR \
		-u GIT_WORK_TREE \
		-u GIT_INDEX_FILE \
		-u GIT_OBJECT_DIRECTORY \
		-u GIT_ALTERNATE_OBJECT_DIRECTORIES \
		-u GIT_COMMON_DIR \
		git -C "$root" \
		-c core.hooksPath=/dev/null \
		-c user.name=fixture \
		-c user.email=fixture@example.com \
		commit -m "$message" >/dev/null
}

run_git() {
	local root="$1"
	shift
	env -u GIT_DIR \
		-u GIT_WORK_TREE \
		-u GIT_INDEX_FILE \
		-u GIT_OBJECT_DIRECTORY \
		-u GIT_ALTERNATE_OBJECT_DIRECTORIES \
		-u GIT_COMMON_DIR \
		git -C "$root" "$@"
}

remote_push_url() {
	local owner="$1"
	local repo="$2"
	printf 'https://x-access-token:%s@github.com/%s/%s.git\n' \
		"$(gh auth token)" \
		"$owner" \
		"$repo"
}

create_workspace() {
	local root="$1"
	local owner="$2"
	local repo="$3"

	mkdir -p "$root/.changeset" "$root/crates"

	{
		echo '[workspace]'
		echo 'members = ['
		local package_index
		for package_index in $(seq 0 $((PACKAGE_COUNT - 1))); do
			echo "  \"crates/pkg-${package_index}\","
		done
		echo ']'
		echo 'resolver = "2"'
	} >"$root/Cargo.toml"

	{
		echo '[defaults]'
		echo 'package_type = "cargo"'
		echo
		echo '[source]'
		echo 'provider = "github"'
		echo "owner = \"${owner}\""
		echo "repo = \"${repo}\""
		echo
		local package_index
		for package_index in $(seq 0 $((PACKAGE_COUNT - 1))); do
			echo "[package.pkg-${package_index}]"
			echo "path = \"crates/pkg-${package_index}\""
			echo
			mkdir -p "$root/crates/pkg-${package_index}/src"
			{
				echo '[package]'
				echo "name = \"pkg-${package_index}\""
				echo 'version = "1.0.0"'
				echo 'edition = "2021"'
				echo
				echo '[lib]'
				echo 'path = "src/lib.rs"'
			} >"$root/crates/pkg-${package_index}/Cargo.toml"
			cat >"$root/crates/pkg-${package_index}/src/lib.rs" <<EOF
pub fn package_${package_index}_version() -> &'static str {
	"1.0.0"
}
EOF
		done
		echo '[ecosystems.cargo]'
		echo 'enabled = true'
	} >"$root/monochange.toml"

	cat >"$root/README.md" <<EOF
# monochange hosted benchmark fixture

This repository is generated for hosted \`mc release\` performance benchmarking.

- owner: ${owner}
- repo: ${repo}
- packages: ${PACKAGE_COUNT}
- filler commits: ${FILLER_COMMITS}
- release pull requests: ${RELEASE_PRS}
EOF
}

append_package_change() {
	local root="$1"
	local package_index="$2"
	local label="$3"
	cat >>"$root/crates/pkg-${package_index}/src/lib.rs" <<EOF

pub fn ${label}_${package_index}() -> &'static str {
	"${label}"
}
EOF
}

seed_filler_history() {
	local root="$1"
	local commit_index
	for commit_index in $(seq 1 "$FILLER_COMMITS"); do
		local package_index=$(((commit_index - 1) % PACKAGE_COUNT))
		append_package_change "$root" "$package_index" "history_commit_${commit_index}"
		run_git "$root" add .
		git_commit "$root" "chore: history commit ${commit_index}"
	done
}

merge_local_release_pr() {
	local root="$1"
	local pr_index="$2"
	local branch_name="$3"
	run_git "$root" checkout main >/dev/null
	run_git "$root" merge --no-ff "$branch_name" -m "Merge pull request #${pr_index} from fixture/${branch_name}" >/dev/null
	run_git "$root" branch -D "$branch_name" >/dev/null
}

merge_hosted_release_pr() {
	local root="$1"
	local owner="$2"
	local repo="$3"
	local pr_index="$4"
	local branch_name="$5"
	local pr_url

	run_git "$root" push -u origin "$branch_name" >/dev/null
	pr_url="$(
		gh pr create \
			--repo "${owner}/${repo}" \
			--base main \
			--head "$branch_name" \
			--title "fixture: seed release PR ${pr_index}" \
			--body "Seed hosted release benchmark fixture PR ${pr_index}."
	)"
	gh pr merge \
		"$pr_url" \
		--squash \
		--delete-branch \
		--admin >/dev/null 2>&1 || gh pr merge \
		"$pr_url" \
		--squash \
		--delete-branch >/dev/null
	run_git "$root" checkout main >/dev/null
	run_git "$root" pull --ff-only origin main >/dev/null
}

seed_release_prs() {
	local root="$1"
	local owner="$2"
	local repo="$3"
	local pr_index
	for pr_index in $(seq 1 "$RELEASE_PRS"); do
		local branch_name
		branch_name="fixture/release-pr-${pr_index}"
		local package_index=$(((pr_index - 1) % PACKAGE_COUNT))
		run_git "$root" checkout -b "$branch_name" main >/dev/null

		local commit_index
		for commit_index in $(seq 1 "$COMMITS_PER_PR"); do
			append_package_change "$root" "$package_index" "release_pr_${pr_index}_commit_${commit_index}"
			run_git "$root" add .
			git_commit "$root" "feat: release fixture commit ${pr_index}.${commit_index}"
		done

		cat >"$root/.changeset/release-pr-$(printf '%02d' "$pr_index").md" <<EOF
---
pkg-${package_index}: patch
---

Release fixture PR ${pr_index}.
EOF
		run_git "$root" add .
		git_commit "$root" "docs: add release changeset ${pr_index}"

		if [ "$LOCAL_ONLY" = true ]; then
			merge_local_release_pr "$root" "$pr_index" "$branch_name"
		else
			merge_hosted_release_pr "$root" "$owner" "$repo" "$pr_index" "$branch_name"
		fi
	done
}

ensure_remote_repo() {
	local owner="$1"
	local repo="$2"

	if gh repo view "${owner}/${repo}" >/dev/null 2>&1; then
		return
	fi

	gh repo create \
		"${owner}/${repo}" \
		"--${VISIBILITY}" \
		--description "Hosted benchmark fixture for monochange release performance" \
		--disable-issues \
		--clone=false >/dev/null
}

parse_args() {
	while [ "$#" -gt 0 ]; do
		case "$1" in
		--output-dir)
			OUTPUT_DIR="$2"
			shift 2
			;;
		--owner)
			OWNER="$2"
			shift 2
			;;
		--repo)
			REPO="$2"
			shift 2
			;;
		--package-count)
			PACKAGE_COUNT="$2"
			shift 2
			;;
		--filler-commits)
			FILLER_COMMITS="$2"
			shift 2
			;;
		--release-prs)
			RELEASE_PRS="$2"
			shift 2
			;;
		--commits-per-pr)
			COMMITS_PER_PR="$2"
			shift 2
			;;
		--local-only)
			LOCAL_ONLY=true
			shift
			;;
		--private)
			VISIBILITY="private"
			shift
			;;
		-h | --help)
			usage
			;;
		*)
			echo "unknown argument: $1" >&2
			usage
			;;
		esac
	done

	if [ -z "$OUTPUT_DIR" ]; then
		echo "--output-dir is required" >&2
		usage
	fi
}

main() {
	parse_args "$@"

	rm -rf "$OUTPUT_DIR"
	mkdir -p "$OUTPUT_DIR"

	create_workspace "$OUTPUT_DIR" "$OWNER" "$REPO"
	run_git "$OUTPUT_DIR" init -b main >/dev/null
	run_git "$OUTPUT_DIR" add .
	git_commit "$OUTPUT_DIR" "chore: initialize hosted benchmark fixture"

	seed_filler_history "$OUTPUT_DIR"

	if [ "$LOCAL_ONLY" = false ]; then
		ensure_remote_repo "$OWNER" "$REPO"
		run_git "$OUTPUT_DIR" remote add origin "$(remote_push_url "$OWNER" "$REPO")"
		run_git "$OUTPUT_DIR" push -u origin main >/dev/null
	fi

	seed_release_prs "$OUTPUT_DIR" "$OWNER" "$REPO"

	printf 'fixture directory: %s\n' "$OUTPUT_DIR"
	printf 'commit count: %s\n' "$(run_git "$OUTPUT_DIR" rev-list --count HEAD)"
	if [ "$LOCAL_ONLY" = false ]; then
		printf 'repository url: https://github.com/%s/%s\n' "$OWNER" "$REPO"
	fi
}

main "$@"
