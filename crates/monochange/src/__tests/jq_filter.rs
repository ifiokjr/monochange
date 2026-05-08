use super::*;

#[test]
fn jq_filter_extracts_array_fields_and_selects_matches() {
	let output = r#"{"assets":[{"name":"a"},{"name":"b"}],"record":{"releaseTargets":[{"id":"main","kind":"group","release":true,"tagName":"v1.2.3"},{"id":"core","kind":"package","release":false,"tagName":"core/v1.2.3"}]},"resolvedCommit":"abc","recordCommit":"abc"}"#;

	assert_eq!(apply_jq_filter(output, ".assets[].name").unwrap(), "a\nb");
	assert_eq!(
		apply_jq_filter(
			output,
			r#".record.releaseTargets[] | select(.id == "main" and .kind == "group" and .release == true) | .tagName"#,
		)
		.unwrap(),
		"v1.2.3"
	);
	assert_eq!(
		apply_jq_filter(output, ".resolvedCommit == .recordCommit").unwrap(),
		"true"
	);
}

#[test]
fn jq_filter_reports_non_json_output() {
	let error = apply_jq_filter("not json", ".assets[].name").unwrap_err();
	assert!(error.to_string().contains("--jq requires JSON output"));
}

#[test]
fn jq_filter_renders_scalar_array_and_object_values() {
	let output = r#"{"number":42,"nothing":null,"items":[{"name":"a"}]}"#;

	assert_eq!(
		apply_jq_filter(output, ".").unwrap(),
		r#"{"items":[{"name":"a"}],"nothing":null,"number":42}"#
	);
	assert_eq!(apply_jq_filter(output, ".number").unwrap(), "42");
	assert_eq!(apply_jq_filter(output, ".nothing").unwrap(), "null");
	assert_eq!(
		apply_jq_filter(output, ".items").unwrap(),
		r#"[{"name":"a"}]"#
	);
}

#[test]
fn jq_filter_handles_empty_stages_and_truthy_selectors() {
	let output = r#"{"items":[{"id":"main","release":true},{"id":"draft","release":false},{"id":"missing"}]}"#;

	assert_eq!(
		apply_jq_filter(output, ".items[] | | select(.release) | .id").unwrap(),
		"main"
	);
	assert_eq!(
		apply_jq_filter(output, ".items[] | select(.missing) | .id").unwrap(),
		""
	);
}

#[test]
fn jq_filter_handles_not_equal_and_string_escapes() {
	let output = r#"{"items":[{"id":"main","name":"a\"b","tags":["x"],"candy":true},{"id":"draft","name":"plain","tags":["y"],"candy":true}]}"#;

	assert_eq!(
		apply_jq_filter(output, r#".items[] | select(.id != "draft") | .id"#).unwrap(),
		"main"
	);
	assert_eq!(
		apply_jq_filter(output, r#".items[] | select("a\"b" == .name) | .id"#).unwrap(),
		"main"
	);
	assert_eq!(
		apply_jq_filter(
			output,
			r#".items[] | select(.tags[0] == "x" and .candy == true and .name == "a\"b") | .id"#
		)
		.unwrap(),
		"main"
	);
}

#[test]
fn jq_filter_reports_invalid_operands() {
	let output = r#"{"id":"main"}"#;

	let invalid_string = apply_jq_filter(output, r#".id == "unterminated"#).unwrap_err();
	assert!(
		invalid_string
			.to_string()
			.contains("invalid --jq string literal")
	);

	let unsupported = apply_jq_filter(output, ".id == unsupported").unwrap_err();
	assert!(unsupported.to_string().contains("unsupported --jq operand"));
}

#[test]
fn jq_filter_ignores_invalid_or_missing_paths() {
	let output = r#"{"items":[{"name":"a"}],"name":"top"}"#;

	assert_eq!(apply_jq_filter(output, "items").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".name[]").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".items[0].name").unwrap(), "a");
	assert_eq!(apply_jq_filter(output, ".items[9].name").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".items[bad].name").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".items[0.name").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".name[0]").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".%").unwrap(), "");
	assert_eq!(apply_jq_filter(output, ".items[].name.foo").unwrap(), "");
}
