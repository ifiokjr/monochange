#![forbid(clippy::indexing_slicing)]

//! Authoring helpers and macros for monochange lint suites.

pub use monochange_core::lint::LintCategory;
pub use monochange_core::lint::LintMaturity;
pub use monochange_core::lint::LintOptionDefinition;
pub use monochange_core::lint::LintOptionKind;
pub use monochange_core::lint::LintRule;

/// Construct a [`LintRule`] with less boilerplate.
#[macro_export]
macro_rules! declare_lint_rule {
    (
        $vis:vis $name:ident,
        id: $id:expr,
        name: $title:expr,
        description: $description:expr,
        category: $category:expr,
        maturity: $maturity:expr,
        autofixable: $autofixable:expr $(,
        options: $options:expr)? $(,)?
    ) => {
        #[derive(Debug)]
        $vis struct $name {
            rule: $crate::LintRule,
        }

        impl $name {
            #[must_use]
            $vis fn new() -> Self {
                let rule = $crate::LintRule::new(
                    $id,
                    $title,
                    $description,
                    $category,
                    $maturity,
                    $autofixable,
                ) $(.with_options($options))?;
                Self { rule }
            }
        }
    };
}

#[cfg(test)]
mod tests {
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
}
