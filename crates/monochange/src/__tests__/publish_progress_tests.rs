use monochange_core::Ecosystem;
use monochange_publish::PackagePublishRunMode;
use monochange_publish::PublishProgressEvent;
use monochange_publish::PublishProgressPackage;
use monochange_publish::PublishProgressReporter;

use super::*;

fn package() -> PublishProgressPackage {
	PublishProgressPackage {
		package_id: "cli".to_string(),
		package_name: "monochange".to_string(),
		version: "1.2.3".to_string(),
		ecosystem: Ecosystem::Cargo,
		registry: "crates.io".to_string(),
	}
}

#[test]
fn render_event_uses_clean_ci_start_marker_without_spinner_noise() {
	let line = StderrPublishProgressReporter::render_event(
		&PublishProgressEvent::PackageStarted(package()),
		false,
	);

	assert_eq!(line, "→ 🦀 cargo monochange publishing 1.2.3 to crates.io");
}

#[test]
fn render_event_uses_terminal_spinner_marker_when_interactive() {
	let line = StderrPublishProgressReporter::render_event(
		&PublishProgressEvent::RegistryCheckStarted(package()),
		true,
	);

	assert_eq!(line, "⠋ 🦀 cargo monochange checking 1.2.3 on crates.io");
}

#[test]
fn render_event_summarizes_publish_run_with_emojis() {
	let line = StderrPublishProgressReporter::render_event(
		&PublishProgressEvent::RunFinished {
			mode: PackagePublishRunMode::Release,
			total: 3,
			published: 2,
			skipped: 1,
			failed: 0,
		},
		false,
	);

	assert_eq!(
		line,
		"◆ Publish complete: 3 packages, ✅ 2 published, ⏭️ 1 skipped, ❌ 0 failed"
	);
}

#[test]
fn render_event_reports_published_and_failed_packages() {
	let package = package();

	assert_eq!(
		StderrPublishProgressReporter::render_event(
			&PublishProgressEvent::PackagePublished(package.clone()),
			false,
		),
		"✅ 🦀 cargo monochange published 1.2.3 to crates.io"
	);
	assert_eq!(
		StderrPublishProgressReporter::render_event(
			&PublishProgressEvent::PackageFailed {
				package,
				message: "registry rejected package".to_string(),
			},
			false,
		),
		"❌ 🦀 cargo monochange failed: registry rejected package"
	);
}

#[test]
fn disabled_reporter_ignores_events() {
	StderrPublishProgressReporter::new(true)
		.report(PublishProgressEvent::PackageStarted(package()));
}
