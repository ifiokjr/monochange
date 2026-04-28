use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde_json::Value;

pub(crate) fn apply_jq_filter(output: &str, expression: &str) -> MonochangeResult<String> {
	let value = serde_json::from_str::<Value>(output.trim()).map_err(|error| {
		MonochangeError::Config(format!(
			"--jq requires JSON output; add `--format json` before filtering: {error}"
		))
	})?;
	let values = evaluate_pipeline(vec![value], expression)?;
	Ok(render_values(&values))
}

fn evaluate_pipeline(values: Vec<Value>, expression: &str) -> MonochangeResult<Vec<Value>> {
	let mut current = values;
	for stage in split_top_level(expression, '|') {
		let stage = stage.trim();
		if stage.is_empty() {
			continue;
		}

		if let Some(condition) = select_condition(stage) {
			let mut filtered = Vec::new();
			for value in current {
				if condition_matches(&value, condition)? {
					filtered.push(value);
				}
			}
			current = filtered;
			continue;
		}

		if is_comparison(stage) {
			current = current
				.into_iter()
				.map(|value| condition_matches(&value, stage).map(Value::Bool))
				.collect::<MonochangeResult<Vec<_>>>()?;
			continue;
		}

		current = current
			.iter()
			.flat_map(|value| evaluate_path(value, stage))
			.collect();
	}
	Ok(current)
}

fn select_condition(stage: &str) -> Option<&str> {
	stage
		.strip_prefix("select(")
		.and_then(|inner| inner.strip_suffix(')'))
		.map(str::trim)
}

fn is_comparison(stage: &str) -> bool {
	find_top_level_operator(stage, "==").is_some() || find_top_level_operator(stage, "!=").is_some()
}

fn condition_matches(value: &Value, condition: &str) -> MonochangeResult<bool> {
	for part in split_top_level_and(condition) {
		if !single_condition_matches(value, part.trim())? {
			return Ok(false);
		}
	}
	Ok(true)
}

fn single_condition_matches(value: &Value, condition: &str) -> MonochangeResult<bool> {
	if let Some(index) = find_top_level_operator(condition, "==") {
		return compare_operands(value, &condition[..index], &condition[index + 2..], true);
	}
	if let Some(index) = find_top_level_operator(condition, "!=") {
		return compare_operands(value, &condition[..index], &condition[index + 2..], false);
	}

	Ok(evaluate_operand(value, condition)?
		.into_iter()
		.any(|candidate| truthy(&candidate)))
}

fn compare_operands(value: &Value, left: &str, right: &str, equal: bool) -> MonochangeResult<bool> {
	let left_values = evaluate_operand(value, left.trim())?;
	let right_values = evaluate_operand(value, right.trim())?;
	let matched = left_values.iter().any(|left_value| {
		right_values
			.iter()
			.any(|right_value| left_value == right_value)
	});
	Ok(if equal { matched } else { !matched })
}

fn evaluate_operand(value: &Value, operand: &str) -> MonochangeResult<Vec<Value>> {
	let operand = operand.trim();
	if operand.starts_with('.') {
		return Ok(evaluate_path(value, operand));
	}
	parse_literal(operand).map(|literal| vec![literal])
}

fn parse_literal(value: &str) -> MonochangeResult<Value> {
	match value {
		"true" => Ok(Value::Bool(true)),
		"false" => Ok(Value::Bool(false)),
		"null" => Ok(Value::Null),
		_ if value.starts_with('"') => {
			serde_json::from_str::<Value>(value).map_err(|error| {
				MonochangeError::Config(format!("invalid --jq string literal `{value}`: {error}"))
			})
		}
		_ => {
			serde_json::from_str::<Value>(value).map_err(|error| {
				MonochangeError::Config(format!("unsupported --jq operand `{value}`: {error}"))
			})
		}
	}
}

fn evaluate_path(value: &Value, path: &str) -> Vec<Value> {
	let path = path.trim();
	if path == "." {
		return vec![value.clone()];
	}
	if !path.starts_with('.') {
		return Vec::new();
	}

	let mut current = vec![value.clone()];
	let chars = path.chars().collect::<Vec<_>>();
	let mut index = 1;
	while index < chars.len() {
		match chars[index] {
			'.' => index += 1,
			'[' if chars.get(index + 1) == Some(&']') => {
				current = current
					.into_iter()
					.flat_map(|candidate| {
						match candidate {
							Value::Array(values) => values,
							_ => Vec::new(),
						}
					})
					.collect();
				index += 2;
			}
			'[' => {
				let Some(end) = chars[index..]
					.iter()
					.position(|character| *character == ']')
				else {
					return Vec::new();
				};
				let end = index + end;
				let Ok(array_index) = chars[index + 1..end]
					.iter()
					.collect::<String>()
					.parse::<usize>()
				else {
					return Vec::new();
				};
				current = current
					.into_iter()
					.filter_map(|candidate| {
						match candidate {
							Value::Array(values) => values.get(array_index).cloned(),
							_ => None,
						}
					})
					.collect();
				index = end + 1;
			}
			_ => {
				let start = index;
				while index < chars.len() && is_field_character(chars[index]) {
					index += 1;
				}
				if start == index {
					return Vec::new();
				}
				let field = chars[start..index].iter().collect::<String>();
				current = current
					.into_iter()
					.filter_map(|candidate| {
						match candidate {
							Value::Object(map) => map.get(&field).cloned(),
							_ => None,
						}
					})
					.collect();
			}
		}
	}
	current
}

fn is_field_character(character: char) -> bool {
	character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn split_top_level(expression: &str, delimiter: char) -> Vec<&str> {
	let mut parts = Vec::new();
	let mut start = 0;
	let mut depth = 0usize;
	let mut in_string = false;
	let mut escaped = false;
	for (index, character) in expression.char_indices() {
		if in_string {
			if escaped {
				escaped = false;
			} else if character == '\\' {
				escaped = true;
			} else if character == '"' {
				in_string = false;
			}
			continue;
		}
		match character {
			'"' => in_string = true,
			'(' | '[' => depth += 1,
			')' | ']' => depth = depth.saturating_sub(1),
			_ if character == delimiter && depth == 0 => {
				parts.push(&expression[start..index]);
				start = index + character.len_utf8();
			}
			_ => {}
		}
	}
	parts.push(&expression[start..]);
	parts
}

fn split_top_level_and(expression: &str) -> Vec<&str> {
	let mut parts = Vec::new();
	let mut start = 0;
	let mut depth = 0usize;
	let mut in_string = false;
	let mut escaped = false;
	let bytes = expression.as_bytes();
	let mut index = 0;
	while index < bytes.len() {
		let character = expression[index..].chars().next().unwrap_or_default();
		if in_string {
			if escaped {
				escaped = false;
			} else if character == '\\' {
				escaped = true;
			} else if character == '"' {
				in_string = false;
			}
			index += character.len_utf8();
			continue;
		}
		match character {
			'"' => in_string = true,
			'(' | '[' => depth += 1,
			')' | ']' => depth = depth.saturating_sub(1),
			'a' if depth == 0 && expression[index..].starts_with("and") => {
				let before = index == 0 || bytes[index - 1].is_ascii_whitespace();
				let after_index = index + 3;
				let after = after_index >= bytes.len() || bytes[after_index].is_ascii_whitespace();
				if before && after {
					parts.push(&expression[start..index]);
					start = after_index;
					index = after_index;
					continue;
				}
			}
			_ => {}
		}
		index += character.len_utf8();
	}
	parts.push(&expression[start..]);
	parts
}

fn find_top_level_operator(expression: &str, operator: &str) -> Option<usize> {
	let mut depth = 0usize;
	let mut in_string = false;
	let mut escaped = false;
	for (index, character) in expression.char_indices() {
		if in_string {
			if escaped {
				escaped = false;
			} else if character == '\\' {
				escaped = true;
			} else if character == '"' {
				in_string = false;
			}
			continue;
		}
		match character {
			'"' => in_string = true,
			'(' | '[' => depth += 1,
			')' | ']' => depth = depth.saturating_sub(1),
			_ if depth == 0 && expression[index..].starts_with(operator) => return Some(index),
			_ => {}
		}
	}
	None
}

fn truthy(value: &Value) -> bool {
	!matches!(value, Value::Null | Value::Bool(false))
}

fn render_values(values: &[Value]) -> String {
	values
		.iter()
		.map(render_value)
		.collect::<Vec<_>>()
		.join("\n")
}

fn render_value(value: &Value) -> String {
	match value {
		Value::Null => "null".to_string(),
		Value::Bool(value) => value.to_string(),
		Value::Number(value) => value.to_string(),
		Value::String(value) => value.clone(),
		Value::Array(_) | Value::Object(_) => {
			serde_json::to_string(value)
				.unwrap_or_else(|error| panic!("serializing filtered JSON should succeed: {error}"))
		}
	}
}

#[cfg(test)]
mod tests {
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
}
