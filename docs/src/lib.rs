//! monochange documentation book
//!
//! This crate exists to register the book's markdown content as rustdoc
//! doctests so they are validated during `cargo test --doc`.

#[doc = include_str!("readme.md")]
#[doc = include_str!("guide/00-start-here.md")]
#[doc = include_str!("guide/01-installation.md")]
#[doc = include_str!("guide/02-setup.md")]
#[doc = include_str!("guide/03-discovery.md")]
#[doc = include_str!("guide/04-configuration.md")]
#[doc = include_str!("guide/05-version-groups.md")]
#[doc = include_str!("guide/06-release-planning.md")]
#[doc = include_str!("guide/08-github-automation.md")]
#[doc = include_str!("guide/09-assistant-setup.md")]
#[doc = include_str!("guide/10-migrating-from-knope.md")]
#[doc = include_str!("guide/11-diagnostics.md")]
#[doc = include_str!("guide/12-repairable-releases.md")]
#[doc = include_str!("reference/progress-output.md")]
#[doc = include_str!("reference/hosted-release-benchmarks.md")]
#[doc = include_str!("reference/cli-steps/00-index.md")]
mod _book {}
