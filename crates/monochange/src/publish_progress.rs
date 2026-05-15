use std::fmt::Write as _;
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
		let mut output = String::with_capacity(128);
		match event {
			PublishProgressEvent::RunStarted {
				mode,
				dry_run,
				total,
				ecosystems,
			} => {
				let dry_run = if *dry_run { " dry-run" } else { "" };
				write!(
					output,
					"◆ Publishing {total} packages ({mode:?}{dry_run}) across "
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
				append_ecosystems(&mut output, ecosystems);
			}
			PublishProgressEvent::RegistryCheckStarted(package) => {
				output.push_str(start_symbol(interactive));
				output.push(' ');
				append_package_prefix(&mut output, package);
				write!(
					output,
					" checking {} on {}",
					package.version, package.registry
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::PackageStarted(package) => {
				output.push_str(start_symbol(interactive));
				output.push(' ');
				append_package_prefix(&mut output, package);
				write!(
					output,
					" publishing {} to {}",
					package.version, package.registry
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::PackageSkipped { package, message } => {
				output.push_str("⏭️ ");
				append_package_prefix(&mut output, package);
				write!(output, " {message}")
					.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::PackagePlanned(package) => {
				output.push_str("📝 ");
				append_package_prefix(&mut output, package);
				write!(
					output,
					" would publish {} to {}",
					package.version, package.registry
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::PackagePublished(package) => {
				output.push_str("✅ ");
				append_package_prefix(&mut output, package);
				write!(
					output,
					" published {} to {}",
					package.version, package.registry
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::PackageFailed { package, message } => {
				output.push_str("❌ ");
				append_package_prefix(&mut output, package);
				write!(output, " failed: {message}")
					.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
			PublishProgressEvent::RunFinished {
				total,
				published,
				skipped,
				failed,
				..
			} => {
				write!(
					output,
					"◆ Publish complete: {total} packages, ✅ {published} published, ⏭️ {skipped} skipped, ❌ {failed} failed"
				)
				.unwrap_or_else(|error| panic!("writing to String cannot fail: {error}"));
			}
		}
		output
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

fn append_ecosystems(output: &mut String, ecosystems: &[monochange_core::Ecosystem]) {
	for (index, ecosystem) in ecosystems.iter().enumerate() {
		if index > 0 {
			output.push_str(", ");
		}
		output.push_str(ecosystem.progress_emoji());
		output.push(' ');
		output.push_str(ecosystem.progress_label());
	}
}

fn append_package_prefix(output: &mut String, package: &PublishProgressPackage) {
	output.push_str(package.ecosystem.progress_emoji());
	output.push(' ');
	output.push_str(package.ecosystem.progress_label());
	output.push(' ');
	output.push_str(&package.package_name);
}

#[cfg(test)]
#[path = "__tests__/publish_progress_tests.rs"]
mod tests;
