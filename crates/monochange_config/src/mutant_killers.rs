#[cfg(test)]
mod mutant_killers {
	use std::path::PathBuf;

	use monochange_core::GroupChangelogInclude;

	use crate::load_workspace_configuration;

	fn fixture_path(name: &str) -> PathBuf {
		PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/config")
			.join(name)
	}

	// -- Kill mutant: is_disabled() guard replaced with false in build_group_definitions --
	// A disabled group changelog with no path should produce None, not an error.

	#[test]
	fn load_workspace_configuration_allows_disabled_group_changelog_without_path() {
		let root = fixture_path("group-changelog-disabled");
		let configuration = load_workspace_configuration(&root)
			.unwrap_or_else(|error| panic!("configuration: {error}"));

		let sdk = configuration
			.group_by_id("sdk")
			.unwrap_or_else(|| panic!("expected sdk group"));

		assert!(
			sdk.changelog.is_none(),
			"disabled group changelog should be None, got: {:?}",
			sdk.changelog
		);
	}

	// -- Kill mutant: "all" arm deleted in parse_group_changelog_include --

	#[test]
	fn load_workspace_configuration_supports_group_changelog_include_all() {
		let root = fixture_path("group-changelog-include-all");
		let configuration = load_workspace_configuration(&root)
			.unwrap_or_else(|error| panic!("configuration: {error}"));

		let sdk = configuration
			.group_by_id("sdk")
			.unwrap_or_else(|| panic!("expected sdk group"));

		assert!(
			sdk.changelog.is_some(),
			"group with changelog path should have changelog target"
		);
		assert_eq!(
			sdk.changelog_include,
			GroupChangelogInclude::All,
			"include = \"all\" should produce GroupChangelogInclude::All"
		);
	}

	// -- Kill mutant: matches!(table.enabled, Some(false)) replaced with false --
	// A package should have no inherited changelog when defaults have enabled=false.

	#[test]
	fn load_workspace_configuration_disabled_default_changelog_produces_no_package_changelog() {
		let root = fixture_path("defaults-changelog-disabled");
		let configuration = load_workspace_configuration(&root)
			.unwrap_or_else(|error| panic!("configuration: {error}"));

		let core = configuration
			.packages
			.iter()
			.find(|pkg| pkg.id == "core")
			.unwrap_or_else(|| panic!("expected core package"));

		assert!(
			core.changelog.is_none(),
			"package should inherit disabled default as no changelog, got: {:?}",
			core.changelog
		);
	}
}
