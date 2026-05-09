use super::*;

#[test]
fn test_workspace_protocol_rule_applies_to_package_json() {
	let rule = WorkspaceProtocolRule::new();
	assert!(rule.applies_to(std::path::Path::new("package.json")));
	assert!(!rule.applies_to(std::path::Path::new("Cargo.toml")));
}

#[test]
fn test_sorted_dependencies_rule() {
	let rule = SortedDependenciesRule::new();
	assert!(rule.applies_to(std::path::Path::new("package.json")));
}
