use super::*;

#[test]
fn convert_changeset_adds_heading_to_body() {
	let input = "---\nmy_crate: minor\n---\nAdd new LSP feature.\n";
	let result = convert_changeset_text(input);
	assert!(result.contains("# New feature"), "heading was not added: {result}");
	assert!(result.contains("Add new LSP feature"), "body was lost");
}

#[test]
fn convert_changeset_preserves_existing_heading() {
	let input = "---\nmy_crate: minor\n---\n# Add new LSP feature\n\nDetails here.\n";
	let result = convert_changeset_text(input);
	assert!(result.contains("# Add new LSP feature"));
	assert!(!result.contains("# New feature"), "should not add duplicate heading");
}

#[test]
fn convert_changeset_replaces_default_with_main() {
	let input = "---\ndefault: minor\n---\nAdd new feature.\n";
	let result = convert_changeset_text(input);
	assert!(result.contains("main: minor"), "default not replaced with main");
	assert!(!result.contains("default: minor"), "default should be replaced");
}

#[test]
fn convert_changeset_preserves_package_ids() {
	let input = "---\nmy_crate: minor\nother_crate: patch\n---\nAdd new feature.\n";
	let result = convert_changeset_text(input);
	assert!(result.contains("my_crate: minor"));
	assert!(result.contains("other_crate: patch"));
}

#[test]
fn split_frontmatter_extracts_correctly() {
	let text = "---\nmy_crate: minor\n---\nSome body text\n";
	let result = split_frontmatter(text);
	assert!(result.is_some());
	let (fm, body) = result.unwrap();
	assert!(fm.contains("my_crate: minor"));
	assert!(body.contains("Some body text"));
}

#[test]
fn split_frontmatter_returns_none_without_delimiters() {
	let text = "Just some text without frontmatter";
	assert!(split_frontmatter(text).is_none());
}

#[test]
fn detect_ecosystem_cargo() {
	let files = vec![toml::Value::String("Cargo.toml".to_string())];
	assert_eq!(detect_ecosystem_from_versioned_files(&files), "cargo");
}

#[test]
fn detect_ecosystem_npm() {
	let files = vec![toml::Value::String("package.json".to_string())];
	assert_eq!(detect_ecosystem_from_versioned_files(&files), "npm");
}

#[test]
fn extract_heading_minor() {
	let fm = "my_crate: minor";
	assert_eq!(extract_heading_from_frontmatter(fm), "New feature");
}

#[test]
fn extract_heading_patch() {
	let fm = "my_crate: patch";
	assert_eq!(extract_heading_from_frontmatter(fm), "Bug fix");
}

#[test]
fn extract_heading_major() {
	let fm = "my_crate: major";
	assert_eq!(extract_heading_from_frontmatter(fm), "Breaking change");
}