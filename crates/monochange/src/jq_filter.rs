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
	while let Some(character) = chars.get(index).copied() {
		match character {
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
				let Some(end) = chars
					.get(index..)
					.and_then(|remaining| remaining.iter().position(|character| *character == ']'))
				else {
					return Vec::new();
				};
				let end = index + end;
				let Some(array_index) = chars
					.get(index + 1..end)
					.and_then(|range| range.iter().collect::<String>().parse::<usize>().ok())
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
				while chars
					.get(index)
					.is_some_and(|character| is_field_character(*character))
				{
					index += 1;
				}
				if start == index {
					return Vec::new();
				}
				let field = chars
					.iter()
					.skip(start)
					.take(index - start)
					.collect::<String>();
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
				let before =
					index == 0 || bytes.get(index - 1).is_some_and(u8::is_ascii_whitespace);
				let after_index = index + 3;
				let after = after_index >= bytes.len()
					|| bytes.get(after_index).is_some_and(u8::is_ascii_whitespace);
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
	let mut rendered = String::new();
	for (index, value) in values.iter().enumerate() {
		if index > 0 {
			rendered.push('\n');
		}
		rendered.push_str(&render_value(value));
	}
	rendered
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
#[path = "__tests__/jq_filter_tests.rs"]
mod tests;
