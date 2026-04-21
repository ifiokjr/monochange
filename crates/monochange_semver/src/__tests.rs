use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use crate::CompatibilityProvider;
use crate::collect_assessments;
use crate::direct_release_severity;
use crate::merge_severities;
use crate::propagated_release_severity;
use crate::strongest_assessment;
use crate::strongest_assessment_for_package;

fn make_assessment(package_id: &str, severity: BumpSeverity) -> CompatibilityAssessment {
	CompatibilityAssessment {
		package_id: package_id.to_string(),
		provider_id: "test".to_string(),
		severity,
		confidence: "high".to_string(),
		summary: format!("{severity:?} change"),
		evidence_location: None,
	}
}

// -- merge_severities --

#[test]
fn merge_severities_prefers_the_highest_bump() {
	assert_eq!(
		merge_severities(BumpSeverity::Patch, BumpSeverity::Minor),
		BumpSeverity::Minor
	);
	assert_eq!(
		merge_severities(BumpSeverity::Major, BumpSeverity::Patch),
		BumpSeverity::Major
	);
}

#[test]
fn merge_severities_handles_none_identity() {
	assert_eq!(
		merge_severities(BumpSeverity::None, BumpSeverity::Patch),
		BumpSeverity::Patch
	);
	assert_eq!(
		merge_severities(BumpSeverity::Minor, BumpSeverity::None),
		BumpSeverity::Minor
	);
	assert_eq!(
		merge_severities(BumpSeverity::None, BumpSeverity::None),
		BumpSeverity::None
	);
}

#[test]
fn merge_severities_is_commutative() {
	for left in [
		BumpSeverity::None,
		BumpSeverity::Patch,
		BumpSeverity::Minor,
		BumpSeverity::Major,
	] {
		for right in [
			BumpSeverity::None,
			BumpSeverity::Patch,
			BumpSeverity::Minor,
			BumpSeverity::Major,
		] {
			assert_eq!(
				merge_severities(left, right),
				merge_severities(right, left),
				"merge_severities({left:?}, {right:?}) is not commutative"
			);
		}
	}
}

// -- strongest_assessment --

#[test]
fn strongest_assessment_returns_highest_severity_assessment() {
	let strongest = strongest_assessment(&[
		make_assessment("cargo:core", BumpSeverity::Patch),
		make_assessment("cargo:core", BumpSeverity::Major),
	])
	.unwrap_or_else(|| panic!("expected strongest assessment"));

	assert_eq!(strongest.severity, BumpSeverity::Major);
}

#[test]
fn strongest_assessment_returns_none_for_empty_slice() {
	assert!(strongest_assessment(&[]).is_none());
}

#[test]
fn strongest_assessment_for_package_filters_by_package_id() {
	let assessments = vec![
		make_assessment("core", BumpSeverity::Major),
		make_assessment("app", BumpSeverity::Patch),
		make_assessment("core", BumpSeverity::Minor),
	];

	let core_strongest = strongest_assessment_for_package(&assessments, "core")
		.unwrap_or_else(|| panic!("expected assessment for core"));
	assert_eq!(core_strongest.severity, BumpSeverity::Major);

	let app_strongest = strongest_assessment_for_package(&assessments, "app")
		.unwrap_or_else(|| panic!("expected assessment for app"));
	assert_eq!(app_strongest.severity, BumpSeverity::Patch);

	assert!(strongest_assessment_for_package(&assessments, "missing").is_none());
}

// -- direct_release_severity --

#[test]
fn direct_release_severity_defaults_to_patch_without_requested_bump() {
	assert_eq!(direct_release_severity(None, None), BumpSeverity::Patch);
}

#[test]
fn direct_release_severity_uses_requested_bump_without_assessment() {
	assert_eq!(
		direct_release_severity(Some(BumpSeverity::Minor), None),
		BumpSeverity::Minor
	);
}

#[test]
fn direct_release_severity_escalates_to_assessment_when_higher() {
	let assessment = make_assessment("core", BumpSeverity::Major);
	assert_eq!(
		direct_release_severity(Some(BumpSeverity::Patch), Some(&assessment)),
		BumpSeverity::Major
	);
}

#[test]
fn direct_release_severity_keeps_requested_bump_when_higher_than_assessment() {
	let assessment = make_assessment("core", BumpSeverity::Patch);
	assert_eq!(
		direct_release_severity(Some(BumpSeverity::Minor), Some(&assessment)),
		BumpSeverity::Minor
	);
}

// -- propagated_release_severity --

#[test]
fn propagated_release_severity_uses_default_parent_bump_without_assessment() {
	assert_eq!(
		propagated_release_severity(BumpSeverity::Patch, None),
		BumpSeverity::Patch
	);
}

#[test]
fn propagated_release_severity_escalates_to_assessment_when_higher() {
	let assessment = make_assessment("core", BumpSeverity::Major);
	assert_eq!(
		propagated_release_severity(BumpSeverity::Patch, Some(&assessment)),
		BumpSeverity::Major
	);
}

#[test]
fn propagated_release_severity_keeps_parent_bump_when_higher_than_assessment() {
	let assessment = make_assessment("core", BumpSeverity::Patch);
	assert_eq!(
		propagated_release_severity(BumpSeverity::Minor, Some(&assessment)),
		BumpSeverity::Minor
	);
}

// -- property tests --

use proptest::prelude::*;
use proptest::prop_compose;
use proptest::proptest;

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
	fn merge_severities_is_commutative_proptest(
		left in arbitrary_bump_severity(),
		right in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(left, right),
			merge_severities(right, left)
		);
	}

	#[test]
	fn merge_severities_is_associative_proptest(
		a in arbitrary_bump_severity(),
		b in arbitrary_bump_severity(),
		c in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(merge_severities(a, b), c),
			merge_severities(a, merge_severities(b, c))
		);
	}

	#[test]
	fn merge_severities_with_none_is_identity_proptest(
		severity in arbitrary_bump_severity()
	) {
		prop_assert_eq!(
			merge_severities(severity, BumpSeverity::None),
			severity
		);
		prop_assert_eq!(
			merge_severities(BumpSeverity::None, severity),
			severity
		);
	}

	#[test]
	fn merge_severities_result_is_gte_each_input_proptest(
		left in arbitrary_bump_severity(),
		right in arbitrary_bump_severity()
	) {
		let merged = merge_severities(left, right);
		prop_assert!(
			merged >= left,
			"merge({left:?}, {right:?}) = {merged:?} < {left:?}"
		);
		prop_assert!(
			merged >= right,
			"merge({left:?}, {right:?}) = {merged:?} < {right:?}"
		);
	}

	#[test]
	fn strongest_assessment_returns_highest_severity_proptest(
		assessments in proptest::collection::vec(
			(arbitrary_bump_severity(), "[a-z]{1,16}"),
			0..20
		)
	) {
		let inputs: Vec<CompatibilityAssessment> = assessments
			.iter()
			.map(|(severity, package_id)| {
				make_assessment(package_id, *severity)
			})
			.collect();

		let strongest = strongest_assessment(&inputs);

		if let Some(ref strongest) = strongest {
			for assessment in &inputs {
				prop_assert!(
					strongest.severity >= assessment.severity,
					"strongest {strongest:?} is weaker than {assessment:?}"
				);
			}
		} else {
			prop_assert!(
				inputs.is_empty(),
				"non-empty input produced None for strongest"
			);
		}
	}

	#[test]
	fn direct_release_severity_never_less_than_requested_proptest(
		requested in arbitrary_bump_severity(),
		assessment in arbitrary_bump_severity()
	) {
		let assessment = make_assessment("core", assessment);
		let result = direct_release_severity(Some(requested), Some(&assessment));
		prop_assert!(
			result >= requested,
			"direct_release_severity({requested:?}, Some({assessment:?})) = {result:?}"
		);
		prop_assert!(
			result >= assessment.severity,
			"direct_release_severity({requested:?}, Some({assessment:?})) = {result:?}"
		);
	}

	#[test]
	fn propagated_release_severity_never_less_than_parent_proptest(
		parent in arbitrary_bump_severity(),
		assessment in arbitrary_bump_severity()
	) {
		let assessment = make_assessment("core", assessment);
		let result = propagated_release_severity(parent, Some(&assessment));
		prop_assert!(
			result >= parent,
			"propagated_release_severity({parent:?}, Some({assessment:?})) = {result:?}"
		);
		prop_assert!(
			result >= assessment.severity,
			"propagated_release_severity({parent:?}, Some({assessment:?})) = {result:?}"
		);
	}
}

// -- collect_assessments --

struct TestProvider {
	severity: BumpSeverity,
}

impl CompatibilityProvider for TestProvider {
	fn provider_id(&self) -> &'static str {
		"test-provider"
	}

	fn assess(
		&self,
		package: &PackageRecord,
		_change_signal: &ChangeSignal,
	) -> Option<CompatibilityAssessment> {
		Some(CompatibilityAssessment {
			package_id: package.id.clone(),
			provider_id: self.provider_id().to_string(),
			severity: self.severity,
			confidence: "high".to_string(),
			summary: "test assessment".to_string(),
			evidence_location: None,
		})
	}
}

#[test]
fn collect_assessments_gathers_from_matching_packages() {
	let root = PathBuf::from("/workspace");
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let package_id = packages.first().unwrap().id.clone();
	let signals = vec![ChangeSignal {
		package_id: package_id.clone(),
		requested_bump: Some(BumpSeverity::Minor),
		explicit_version: None,
		change_type: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: Vec::new(),
		notes: None,
		details: None,
		caused_by: Vec::new(),
		source_path: PathBuf::from(".changeset/feature.md"),
	}];
	let provider = TestProvider {
		severity: BumpSeverity::Major,
	};
	let assessments = collect_assessments(&[&provider], &packages, &signals);

	assert_eq!(assessments.len(), 1);
	assert_eq!(assessments.first().unwrap().severity, BumpSeverity::Major);
	assert_eq!(assessments.first().unwrap().package_id, package_id);
}

#[test]
fn collect_assessments_returns_empty_for_unmatched_signals() {
	let root = PathBuf::from("/workspace");
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let signals = vec![ChangeSignal {
		package_id: "nonexistent".to_string(),
		requested_bump: Some(BumpSeverity::Patch),
		explicit_version: None,
		change_type: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: Vec::new(),
		notes: None,
		details: None,
		caused_by: Vec::new(),
		source_path: PathBuf::from(".changeset/fix.md"),
	}];
	let provider = TestProvider {
		severity: BumpSeverity::Patch,
	};
	let assessments = collect_assessments(&[&provider], &packages, &signals);
	assert!(assessments.is_empty());
}

#[test]
fn collect_assessments_returns_empty_without_providers() {
	let root = PathBuf::from("/workspace");
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let signals = vec![ChangeSignal {
		package_id: "core".to_string(),
		requested_bump: Some(BumpSeverity::Patch),
		explicit_version: None,
		change_type: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: Vec::new(),
		notes: None,
		details: None,
		caused_by: Vec::new(),
		source_path: PathBuf::from(".changeset/fix.md"),
	}];
	let providers: Vec<&dyn CompatibilityProvider> = vec![];
	let assessments = collect_assessments(&providers, &packages, &signals);
	assert!(assessments.is_empty());
}
