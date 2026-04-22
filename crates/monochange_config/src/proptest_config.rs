use std::path::Path;

use monochange_core::EcosystemType;
use monochange_core::PackageType;
use proptest::prelude::*;

use crate::package_type_to_ecosystem_type;
use crate::render_changelog_path_template;

#[test]
fn package_type_cargo_maps_to_ecosystem_type_cargo() {
	assert_eq!(
		package_type_to_ecosystem_type(PackageType::Cargo),
		EcosystemType::Cargo
	);
}

#[test]
fn package_type_npm_maps_to_ecosystem_type_npm() {
	assert_eq!(
		package_type_to_ecosystem_type(PackageType::Npm),
		EcosystemType::Npm
	);
}

#[test]
fn package_type_deno_maps_to_ecosystem_type_deno() {
	assert_eq!(
		package_type_to_ecosystem_type(PackageType::Deno),
		EcosystemType::Deno
	);
}

#[test]
fn package_type_dart_maps_to_ecosystem_type_dart() {
	assert_eq!(
		package_type_to_ecosystem_type(PackageType::Dart),
		EcosystemType::Dart
	);
}

#[test]
fn package_type_flutter_maps_to_ecosystem_type_dart() {
	assert_eq!(
		package_type_to_ecosystem_type(PackageType::Flutter),
		EcosystemType::Dart
	);
}

proptest! {
	#[test]
	fn package_dir_always_replaced_with_directory_name(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));
		prop_assume!(!path_str.is_empty());

		let template = format!("{prefix}{{{{package_dir}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		let expected_dir = path
			.file_name()
			.map(|s| s.to_string_lossy().into_owned())
			.unwrap_or_default();

		prop_assert!(!result.contains("{{package_dir}}"));
		prop_assert_eq!(result, format!("{prefix}{expected_dir}{suffix}"));
	}

	#[test]
	fn package_name_always_replaced_with_file_stem(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));
		prop_assume!(!path_str.is_empty());

		let template = format!("{prefix}{{{{package_name}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		let expected_stem = path
			.file_stem()
			.map(|s| s.to_string_lossy().into_owned())
			.unwrap_or_default();

		prop_assert!(!result.contains("{{package_name}}"));
		prop_assert_eq!(result, format!("{prefix}{expected_stem}{suffix}"));
	}

	#[test]
	fn unknown_placeholders_are_left_unchanged(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));

		let template = format!("{prefix}{{{{unknown}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		prop_assert_eq!(result, template);
	}

	#[test]
	fn idempotent_when_no_placeholders_exist(
		template in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!template.contains("{{"));

		let path = Path::new(&path_str);
		let once = render_changelog_path_template(&template, path);
		let twice = render_changelog_path_template(&once, path);

		prop_assert_eq!(once, twice);
	}
}
