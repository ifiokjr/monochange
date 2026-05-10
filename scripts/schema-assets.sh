#!/usr/bin/env bash
set -euo pipefail

mode="${1:-check}"
case "$mode" in
update | check) ;;
*)
	echo "usage: $0 [update|check]" >&2
	exit 2
	;;
esac

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
schema_source="$repo_root/crates/monochange_schema/src/lib.rs"
version_toml="$(grep '^version = ' "$repo_root/crates/monochange_schema/Cargo.toml" | head -1 | sed 's/.*"\([^"]*\)".*/\1/')"
version="$(echo "$version_toml" | sed 's/\.[0-9]*$//')"
kind="$(sed -n 's/^[[:space:]]*pub const KIND: &str = "\([^"]*\)";$/\1/p' "$schema_source")"

if [[ -z "$version" ]]; then
	echo "failed to read CURRENT_SCHEMA_VERSION_TEXT from $schema_source" >&2
	exit 1
fi
if [[ -z "$kind" ]]; then
	echo "failed to read release_record::KIND from $schema_source" >&2
	exit 1
fi

release_template="$repo_root/schemas/templates/release-record.schema.template.json"
config_template="$repo_root/schemas/templates/monochange.schema.template.json"

files=(
	"crates/monochange_schema/schemas/release-record.schema.json"
	"docs/src/schemas/release-record.schema.json"
	"docs/src/schemas/release-record.v${version}.schema.json"
	"docs/src/schemas/monochange.schema.json"
	"docs/src/schemas/monochange.v${version}.schema.json"
)

if [[ "$mode" == "update" ]]; then
	output_root="$repo_root"
else
	output_root="$repo_root/.schema-assets-check"
	rm -rf "$output_root"
	trap 'rm -rf "$output_root"' EXIT
fi

write_release_schema() {
	local relative_path="$1"
	local schema_id="$2"
	local output_path="$output_root/$relative_path"
	mkdir -p "$(dirname "$output_path")"
	jq \
		--arg id "$schema_id" \
		--arg version "$version" \
		--arg kind "$kind" \
		'.["$id"] = $id | .description = ("Durable commit-embedded release record schema for monochange artifact version " + $version + ".") | .properties.schemaVersion.const = $version | .properties.kind.const = $kind' \
		"$release_template" >"$output_path"
}

write_config_schema() {
	local relative_path="$1"
	local schema_id="$2"
	local output_path="$output_root/$relative_path"
	mkdir -p "$(dirname "$output_path")"
	jq --arg id "$schema_id" '.["$id"] = $id' "$config_template" >"$output_path"
}

write_release_schema \
	"crates/monochange_schema/schemas/release-record.schema.json" \
	"https://monochange.github.io/monochange/schemas/release-record.schema.json"
write_release_schema \
	"docs/src/schemas/release-record.schema.json" \
	"https://monochange.github.io/monochange/schemas/release-record.schema.json"
write_release_schema \
	"docs/src/schemas/release-record.v${version}.schema.json" \
	"https://monochange.github.io/monochange/schemas/release-record.v${version}.schema.json"
write_config_schema \
	"docs/src/schemas/monochange.schema.json" \
	"https://monochange.github.io/monochange/schemas/monochange.schema.json"
write_config_schema \
	"docs/src/schemas/monochange.v${version}.schema.json" \
	"https://monochange.github.io/monochange/schemas/monochange.v${version}.schema.json"

generated_files=()
for relative_path in "${files[@]}"; do
	generated_files+=("$output_root/$relative_path")
done

dprint fmt --config "$repo_root/dprint.json" "${generated_files[@]}"

if [[ "$mode" == "update" ]]; then
	exit 0
fi

stale=0
for relative_path in "${files[@]}"; do
	expected="$output_root/$relative_path"
	actual="$repo_root/$relative_path"
	if ! cmp -s "$expected" "$actual"; then
		echo "schema asset is stale: $relative_path" >&2
		diff -u "$actual" "$expected" >&2 || true
		stale=1
	fi
done

if [[ "$stale" -ne 0 ]]; then
	echo "schema assets are stale; run schema:update" >&2
	exit 1
fi
