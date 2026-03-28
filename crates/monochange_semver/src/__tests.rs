use monochange_core::BumpSeverity;
use monochange_core::CompatibilityAssessment;

use crate::direct_release_severity;
use crate::merge_severities;
use crate::propagated_release_severity;
use crate::strongest_assessment;

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
fn strongest_assessment_returns_highest_severity_assessment() {
	let strongest = strongest_assessment(&[
		CompatibilityAssessment {
			package_id: "cargo:core".to_string(),
			provider_id: "test".to_string(),
			severity: BumpSeverity::Patch,
			confidence: "high".to_string(),
			summary: "patch".to_string(),
			evidence_location: None,
		},
		CompatibilityAssessment {
			package_id: "cargo:core".to_string(),
			provider_id: "test".to_string(),
			severity: BumpSeverity::Major,
			confidence: "high".to_string(),
			summary: "major".to_string(),
			evidence_location: None,
		},
	])
	.unwrap_or_else(|| panic!("expected strongest assessment"));

	assert_eq!(strongest.severity, BumpSeverity::Major);
}

#[test]
fn direct_and_propagated_release_severity_follow_semver_rules() {
	let assessment = CompatibilityAssessment {
		package_id: "cargo:core".to_string(),
		provider_id: "rust-semver".to_string(),
		severity: BumpSeverity::Major,
		confidence: "high".to_string(),
		summary: "public API break detected".to_string(),
		evidence_location: None,
	};

	assert_eq!(
		direct_release_severity(Some(BumpSeverity::Minor), Some(&assessment)),
		BumpSeverity::Major
	);
	assert_eq!(
		propagated_release_severity(BumpSeverity::Patch, Some(&assessment)),
		BumpSeverity::Major
	);
	assert_eq!(
		propagated_release_severity(BumpSeverity::Patch, None),
		BumpSeverity::Patch
	);
}
