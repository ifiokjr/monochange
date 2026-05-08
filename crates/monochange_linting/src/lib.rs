#![forbid(clippy::indexing_slicing)]

//! Authoring helpers and macros for monochange lint suites.
//!
//! Use [`declare_lint_rule`] for straightforward rules whose customization mostly
//! lives in `run(...)`. The Cargo suite uses the macro in real lint
//! implementations, while rules that need additional constructor state can still
//! use an explicit `struct` plus [`LintRule::new`].

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
#[path = "__tests.rs"]
mod tests;
