use std::io;
use std::io::IsTerminal;

use monochange_publish::EcosystemProgressPresentation;
use monochange_publish::PublishProgressEvent;
use monochange_publish::PublishProgressPackage;
use monochange_publish::PublishProgressReporter;

pub(crate) struct StderrPublishProgressReporter {
	enabled: bool,
	interactive: bool,
}

impl StderrPublishProgressReporter {
	pub(crate) fn new(quiet: bool) -> Self {
		let no_progress = std::env::var_os("MONOCHANGE_NO_PROGRESS").is_some();
		let interactive = io::stderr().is_terminal() && std::env::var_os("CI").is_none();
		Self {
			enabled: !quiet && !no_progress,
			interactive,
		}
	}

	pub(crate) fn render_event(event: &PublishProgressEvent, interactive: bool) -> String {
		match event {
			PublishProgressEvent::RunStarted {
				mode,
				dry_run,
				total,
				ecosystems,
			} => {
				let ecosystems = ecosystems
					.iter()
					.map(|ecosystem| {
						format!(
							"{} {}",
							ecosystem.progress_emoji(),
							ecosystem.progress_label()
						)
					})
					.collect::<Vec<_>>()
					.join(", ");
				let dry_run = if *dry_run { " dry-run" } else { "" };
				format!("◆ Publishing {total} packages ({mode:?}{dry_run}) across {ecosystems}")
			}
			PublishProgressEvent::RegistryCheckStarted(package) => {
				format!(
					"{} {} checking {} on {}",
					start_symbol(interactive),
					package_prefix(package),
					package.version,
					package.registry
				)
			}
			PublishProgressEvent::PackageStarted(package) => {
				format!(
					"{} {} publishing {} to {}",
					start_symbol(interactive),
					package_prefix(package),
					package.version,
					package.registry
				)
			}
			PublishProgressEvent::PackageSkipped { package, message } => {
				format!("⏭️ {} {message}", package_prefix(package))
			}
			PublishProgressEvent::PackagePlanned(package) => {
				format!(
					"📝 {} would publish {} to {}",
					package_prefix(package),
					package.version,
					package.registry
				)
			}
			PublishProgressEvent::PackagePublished(package) => {
				format!(
					"✅ {} published {} to {}",
					package_prefix(package),
					package.version,
					package.registry
				)
			}
			PublishProgressEvent::PackageFailed { package, message } => {
				format!("❌ {} failed: {message}", package_prefix(package))
			}
			PublishProgressEvent::RunFinished {
				total,
				published,
				skipped,
				failed,
				..
			} => {
				format!(
					"◆ Publish complete: {total} packages, ✅ {published} published, ⏭️ {skipped} skipped, ❌ {failed} failed"
				)
			}
		}
	}
}

impl PublishProgressReporter for StderrPublishProgressReporter {
	fn report(&self, event: PublishProgressEvent) {
		if !self.enabled {
			return;
		}
		eprintln!("{}", Self::render_event(&event, self.interactive));
	}
}

fn start_symbol(interactive: bool) -> &'static str {
	if interactive { "⠋" } else { "→" }
}

fn package_prefix(package: &PublishProgressPackage) -> String {
	format!(
		"{} {} {}",
		package.ecosystem.progress_emoji(),
		package.ecosystem.progress_label(),
		package.package_name
	)
}

#[cfg(test)]
#[path = "__tests__/publish_progress_tests.rs"]
mod tests;
