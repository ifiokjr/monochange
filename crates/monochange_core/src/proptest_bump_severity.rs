#[cfg(test)]
mod proptest_bump_severity {
	use proptest::prelude::*;
	use proptest::proptest;
	use proptest::prop_compose;
	use semver::Version;

	use crate::BumpSeverity;

	fn arbitrary_version() -> impl Strategy<Value = Version> {
		(0..=99u64, 0..=99u64, 0..=99u64).prop_map(|(major, minor, patch)| {
			Version::new(major, minor, patch)
		})
	}

	prop_compose! {
		fn arbitrary_bump_severity()(n in 0..4u8) -> BumpSeverity {
			match n {
				0 => BumpSeverity::None,
				1 => BumpSeverity::Patch,
				2 => BumpSeverity::Minor,
				3 => BumpSeverity::Major,
				_ => unreachable!(),
			}
		}
	}

	proptest! {
		#[test]
		fn apply_to_version_is_strictly_increasing_for_release_severity(
			version in arbitrary_version()
		) {
			for severity in [BumpSeverity::Patch, BumpSeverity::Minor, BumpSeverity::Major] {
				let next = severity.apply_to_version(&version);
				let version_s = version.to_string();
				let next_s = next.to_string();
				prop_assert!(
					next > version,
					"apply_to_version({:?}, {}) = {} should be strictly greater",
					severity, version_s, next_s
				);
			}
		}

		#[test]
		fn apply_to_version_preserves_version_for_none_severity(
			version in arbitrary_version()
		) {
			let next = BumpSeverity::None.apply_to_version(&version);
			prop_assert_eq!(next, version);
		}

		#[test]
		fn apply_to_version_resets_pre_and_build_metadata(
			mut version in arbitrary_version(),
			pre in "[a-z]*",
			build in "[a-z]*"
		) {
			if !pre.is_empty() {
				version.pre = semver::Prerelease::new(&pre).unwrap_or_default();
			}
			if !build.is_empty() {
				version.build = semver::BuildMetadata::new(&build).unwrap_or_default();
			}
			for severity in [BumpSeverity::Patch, BumpSeverity::Minor, BumpSeverity::Major] {
				let next = severity.apply_to_version(&version);
				let next_s = next.to_string();
				let version_s = version.to_string();
				prop_assert!(
					next.pre.is_empty(),
					"pre-release metadata should be cleared: {version_s} -> {next_s}"
				);
				prop_assert!(
					next.build.is_empty(),
					"build metadata should be cleared: {version_s} -> {next_s}"
				);
			}
		}

		#[test]
		fn apply_to_version_is_idempotent_for_none_severity(
			version in arbitrary_version()
		) {
			let once = BumpSeverity::None.apply_to_version(&version);
			let twice = BumpSeverity::None.apply_to_version(&once);
			prop_assert_eq!(
				once, twice,
				"None.apply_to_version should be idempotent"
			);
		}

		#[test]
		fn pre_stable_shifting_preserves_release_order(
			version in arbitrary_version()
		) {
			let is_pre = BumpSeverity::is_pre_stable(&version);
			let patch_next = BumpSeverity::Patch.apply_to_version(&version);
			let minor_next = BumpSeverity::Minor.apply_to_version(&version);
			let major_next = BumpSeverity::Major.apply_to_version(&version);

			let (patch_s, minor_s, major_s, version_s) = (
				patch_next.to_string(),
				minor_next.to_string(),
				major_next.to_string(),
				version.to_string(),
			);
			prop_assert!(
				patch_next >= version,
				"Patch should not decrease version: {version_s} -> {patch_s}"
			);
			prop_assert!(
				minor_next >= patch_next,
				"Minor should be >= Patch: {version_s} Patch={patch_s} Minor={minor_s}"
			);
			prop_assert!(
				major_next >= minor_next,
				"Major should be >= Minor: {version_s} Minor={minor_s} Major={major_s}"
			);

			if is_pre {
				prop_assert!(
					minor_next == patch_next,
					"Pre-stable: Minor ({}) should equal Patch ({}) for {}",
					minor_s, patch_s, version_s
				);
			}
		}
	}
}
