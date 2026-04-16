#![forbid(clippy::indexing_slicing)]

//! Dart and Flutter manifest lint suite.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use monochange_core::MonochangeResult;
use monochange_core::PublishState;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::LintTargetMetadata;
use monochange_core::relative_to_root;
use serde_yaml_ng::Mapping;

use crate::discover_dart_packages;

/// Return the shared Dart lint suite.
#[must_use]
pub fn lint_suite() -> DartLintSuite {
	DartLintSuite
}

/// Dart lint suite implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct DartLintSuite;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DartLintFile {
	pub manifest: Mapping,
	pub workspace_package_names: Arc<BTreeSet<String>>,
}

impl LintSuite for DartLintSuite {
	fn suite_id(&self) -> &'static str {
		"dart"
	}

	fn rules(&self) -> Vec<Box<dyn monochange_core::lint::LintRuleRunner>> {
		Vec::new()
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let discovery = discover_dart_packages(workspace_root)?;
		let workspace_package_names = Arc::new(
			discovery
				.packages
				.iter()
				.map(|package| package.name.clone())
				.collect::<BTreeSet<_>>(),
		);

		discovery
			.packages
			.into_iter()
			.filter(|package| {
				is_lintable_workspace_manifest(workspace_root, &package.manifest_path)
			})
			.map(|package| {
				let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
					monochange_core::MonochangeError::IoSource {
						path: package.manifest_path.clone(),
						source: error,
					}
				})?;
				let manifest = serde_yaml_ng::from_str::<Mapping>(&contents).map_err(|error| {
					monochange_core::MonochangeError::Parse {
						path: package.manifest_path.clone(),
						source: Box::new(error),
					}
				})?;
				let manifest_dir = package.manifest_path.parent().unwrap_or(workspace_root);
				let configured_package =
					configured_package(configuration, workspace_root, manifest_dir);
				let package_id = configured_package.map(ToString::to_string);
				let group_id = configured_package.and_then(|package_id| {
					configuration
						.group_for_package(package_id)
						.map(|group| group.id.clone())
				});
				let relative_path = relative_to_root(workspace_root, &package.manifest_path)
					.unwrap_or_else(|| package.manifest_path.clone());
				let private = matches!(package.publish_state, PublishState::Private);

				Ok(LintTarget::new(
					workspace_root.to_path_buf(),
					package.manifest_path.clone(),
					contents,
					LintTargetMetadata {
						ecosystem: "dart".to_string(),
						relative_path,
						package_name: Some(package.name),
						package_id,
						group_id,
						managed: configured_package.is_some(),
						private: Some(private),
						publishable: Some(!private),
					},
					Box::new(DartLintFile {
						manifest,
						workspace_package_names: Arc::clone(&workspace_package_names),
					}),
				))
			})
			.collect()
	}
}

fn is_lintable_workspace_manifest(workspace_root: &Path, manifest_path: &Path) -> bool {
	!(manifest_path.starts_with(workspace_root.join("fixtures"))
		|| manifest_path.starts_with(workspace_root.join("target"))
		|| manifest_path.starts_with(workspace_root.join(".git")))
}

fn configured_package<'a>(
	configuration: &'a WorkspaceConfiguration,
	workspace_root: &Path,
	manifest_dir: &Path,
) -> Option<&'a str> {
	let relative_dir = relative_to_root(workspace_root, manifest_dir)?;
	configuration
		.packages
		.iter()
		.find_map(|package| (package.path == relative_dir).then_some(package.id.as_str()))
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use monochange_config::load_workspace_configuration;
	use monochange_core::lint::LintSuite;

	use super::DartLintFile;
	use super::lint_suite;

	#[test]
	fn collect_targets_loads_managed_workspace_dart_packages() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/workspace");
		let configuration = load_workspace_configuration(&root)
			.unwrap_or_else(|error| panic!("load dart workspace config: {error}"));
		let targets = lint_suite()
			.collect_targets(&root, &configuration)
			.unwrap_or_else(|error| panic!("collect dart lint targets: {error}"));

		assert_eq!(targets.len(), 2);
		assert!(
			targets
				.iter()
				.all(|target| target.metadata.ecosystem == "dart")
		);
		assert!(targets.iter().all(|target| target.metadata.managed));
		assert!(
			targets
				.iter()
				.all(|target| target.parsed.downcast_ref::<DartLintFile>().is_some())
		);
		assert!(
			targets
				.iter()
				.any(|target| target.metadata.package_name.as_deref() == Some("dart_app"))
		);
		assert!(
			targets
				.iter()
				.any(|target| target.metadata.package_name.as_deref() == Some("dart_shared"))
		);
	}

	#[test]
	fn collect_targets_ignores_fixture_manifests_outside_workspace_packages() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
		let configuration = load_workspace_configuration(&root)
			.unwrap_or_else(|error| panic!("load workspace config: {error}"));
		let targets = lint_suite()
			.collect_targets(&root, &configuration)
			.unwrap_or_else(|error| panic!("collect repo dart lint targets: {error}"));

		assert!(
			targets
				.iter()
				.all(|target| !target.manifest_path.starts_with(root.join("fixtures")))
		);
	}
}
