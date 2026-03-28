#![deny(clippy::all)]
#![forbid(clippy::indexing_slicing)]

doc_comment::doctest!("../readme.md");

use monochange_core::BumpSeverity;
use monochange_core::ChangeSignal;
use monochange_core::CompatibilityAssessment;
use monochange_core::PackageRecord;

pub trait CompatibilityProvider {
	fn provider_id(&self) -> &'static str;

	fn assess(
		&self,
		package: &PackageRecord,
		change_signal: &ChangeSignal,
	) -> Option<CompatibilityAssessment>;
}

#[must_use]
pub fn collect_assessments(
	providers: &[&dyn CompatibilityProvider],
	packages: &[PackageRecord],
	change_signals: &[ChangeSignal],
) -> Vec<CompatibilityAssessment> {
	change_signals
		.iter()
		.filter_map(|change_signal| {
			packages
				.iter()
				.find(|package| package.id == change_signal.package_id)
				.map(|package| (package, change_signal))
		})
		.flat_map(|(package, change_signal)| {
			providers
				.iter()
				.filter_map(|provider| provider.assess(package, change_signal))
		})
		.collect()
}

#[must_use]
pub fn merge_severities(left: BumpSeverity, right: BumpSeverity) -> BumpSeverity {
	left.max(right)
}

#[must_use]
pub fn strongest_assessment(
	assessments: &[CompatibilityAssessment],
) -> Option<CompatibilityAssessment> {
	assessments
		.iter()
		.cloned()
		.max_by_key(|assessment| assessment.severity)
}

#[must_use]
pub fn strongest_assessment_for_package(
	assessments: &[CompatibilityAssessment],
	package_id: &str,
) -> Option<CompatibilityAssessment> {
	let matching = assessments
		.iter()
		.filter(|assessment| assessment.package_id == package_id)
		.cloned()
		.collect::<Vec<_>>();
	strongest_assessment(&matching)
}

#[must_use]
pub fn direct_release_severity(
	requested_bump: Option<BumpSeverity>,
	assessment: Option<&CompatibilityAssessment>,
) -> BumpSeverity {
	merge_severities(
		requested_bump.unwrap_or(BumpSeverity::Patch),
		assessment.map_or(BumpSeverity::None, |value| value.severity),
	)
}

#[must_use]
pub fn propagated_release_severity(
	default_parent_bump: BumpSeverity,
	assessment: Option<&CompatibilityAssessment>,
) -> BumpSeverity {
	merge_severities(
		default_parent_bump,
		assessment.map_or(BumpSeverity::None, |value| value.severity),
	)
}

#[cfg(test)]
mod __tests;
