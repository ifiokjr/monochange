use super::*;

declare_lint_rule! {
	pub ExampleRule,
	id: "example/rule",
	name: "Example rule",
	description: "Example description",
	category: LintCategory::BestPractice,
	maturity: LintMaturity::Experimental,
	autofixable: true,
	options: vec![LintOptionDefinition::new(
		"fix",
		"apply an autofix",
		LintOptionKind::Boolean,
	)],
}

#[test]
fn declare_lint_rule_macro_builds_expected_metadata() {
	let rule = ExampleRule::new();
	assert_eq!(rule.rule.id, "example/rule");
	assert_eq!(rule.rule.name, "Example rule");
	assert_eq!(rule.rule.description, "Example description");
	assert_eq!(rule.rule.category, LintCategory::BestPractice);
	assert_eq!(rule.rule.maturity, LintMaturity::Experimental);
	assert!(rule.rule.autofixable);
	assert_eq!(rule.rule.options.len(), 1);
	assert_eq!(
		rule.rule.options.first().map(|option| option.name.as_str()),
		Some("fix")
	);
}
