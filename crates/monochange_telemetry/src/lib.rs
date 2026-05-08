//! # `monochange_telemetry`
//!
//! <!-- {=monochangeTelemetryCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_telemetry` provides local-only telemetry primitives for the `monochange` CLI.
//!
//! Reach for this crate when you need the reusable event sink, event payloads, and privacy-preserving error classification that power opt-in local JSONL telemetry. The crate intentionally keeps transport simple: it appends OpenTelemetry-style JSON Lines records to a local file and does not send telemetry over the network.
//!
//! ## Why use it?
//!
//! - keep telemetry capture separate from CLI orchestration and package discovery
//! - share one local JSONL event schema across command and step instrumentation
//! - classify errors into low-cardinality categories without exposing raw error text
//! - make telemetry writes best-effort so observability cannot change command outcomes
//!
//! ## Best for
//!
//! - embedding monochange's local telemetry sink in the CLI runtime
//! - smoke-testing event schemas without provisioning a backend
//! - building future telemetry commands, exporters, or redaction tests on top of a small public API
//!
//! ## Public entry points
//!
//! - `TelemetrySink::from_env()` resolves `MC_TELEMETRY` and `MC_TELEMETRY_FILE` into either a disabled sink or a local JSONL sink
//! - `TelemetrySink::capture_command(...)` writes `command_run` events
//! - `TelemetrySink::capture_step(...)` writes `command_step` events
//! - `CommandTelemetry`, `StepTelemetry`, and `TelemetryOutcome` describe the stable event payloads
//!
//! ## Privacy boundaries
//!
//! The crate only accepts low-cardinality command metadata, booleans, counts, durations, enum outcomes, and sanitized `error_kind` values. It does not collect package names, paths, repository URLs, branch names, refs, commit hashes, shell command strings, environment values, changeset text, release notes, issue or pull request IDs, or raw errors.
//!
//! ## Example
//!
//! ```rust
//! use monochange_telemetry::CommandTelemetry;
//! use monochange_telemetry::TelemetryOutcome;
//! use monochange_telemetry::TelemetrySink;
//! use std::time::Duration;
//!
//! let sink = TelemetrySink::Disabled;
//! sink.capture_command(CommandTelemetry {
//!     command_name: "validate",
//!     dry_run: false,
//!     show_diff: false,
//!     progress_format: "auto",
//!     step_count: 1,
//!     duration: Duration::from_millis(42),
//!     outcome: TelemetryOutcome::Success,
//!     error: None,
//! });
//! ```
//! <!-- {/monochangeTelemetryCrateDocs} -->

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::LazyLock;
#[cfg(test)]
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use monochange_core::MonochangeError;
use serde::Serialize;
use serde_json::json;

const TELEMETRY_ENV: &str = "MC_TELEMETRY";
const TELEMETRY_FILE_ENV: &str = "MC_TELEMETRY_FILE";
const TELEMETRY_SCOPE_NAME: &str = "monochange.telemetry";
const TELEMETRY_SCOPE_VERSION: &str = "0.1.0";

#[cfg(test)]
static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone)]
pub enum TelemetrySink {
	Disabled,
	LocalJsonl { path: PathBuf },
}

#[derive(Debug, Clone, Copy)]
pub struct CommandTelemetry<'a> {
	pub command_name: &'a str,
	pub dry_run: bool,
	pub show_diff: bool,
	pub progress_format: &'a str,
	pub step_count: usize,
	pub duration: Duration,
	pub outcome: TelemetryOutcome,
	pub error: Option<&'a MonochangeError>,
}

#[derive(Debug, Clone, Copy)]
pub struct StepTelemetry<'a> {
	pub command_name: &'a str,
	pub step_index: usize,
	pub step_kind: &'a str,
	pub skipped: bool,
	pub duration: Duration,
	pub outcome: TelemetryOutcome,
	pub error: Option<&'a MonochangeError>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TelemetryOutcome {
	Success,
	Skipped,
	Error,
}

impl TelemetrySink {
	pub fn from_env() -> Self {
		let explicit_file = env::var_os(TELEMETRY_FILE_ENV).map(PathBuf::from);
		let Some(mode) = env::var_os(TELEMETRY_ENV) else {
			return explicit_file.map_or(Self::Disabled, Self::local_jsonl);
		};
		let mode = mode.to_string_lossy().to_ascii_lowercase();
		if matches!(
			mode.as_str(),
			"1" | "true" | "on" | "yes" | "local" | "jsonl"
		) {
			return Self::local_jsonl(explicit_file.unwrap_or_else(default_telemetry_file));
		}
		Self::Disabled
	}

	pub fn capture_command(&self, telemetry: CommandTelemetry<'_>) {
		let attributes = BTreeMap::from([
			("command_name".to_string(), json!(telemetry.command_name)),
			(
				"command_source".to_string(),
				json!(command_source(telemetry.command_name)),
			),
			("dry_run".to_string(), json!(telemetry.dry_run)),
			("show_diff".to_string(), json!(telemetry.show_diff)),
			(
				"progress_format".to_string(),
				json!(telemetry.progress_format),
			),
			("step_count".to_string(), json!(telemetry.step_count)),
			(
				"duration_ms".to_string(),
				json!(duration_millis(telemetry.duration)),
			),
			("outcome".to_string(), json!(telemetry.outcome.as_str())),
			(
				"error_kind".to_string(),
				json!(telemetry.error.map(error_kind)),
			),
		]);

		self.capture("command_run", attributes);
	}

	pub fn capture_step(&self, telemetry: StepTelemetry<'_>) {
		let attributes = BTreeMap::from([
			("command_name".to_string(), json!(telemetry.command_name)),
			("step_index".to_string(), json!(telemetry.step_index)),
			("step_kind".to_string(), json!(telemetry.step_kind)),
			("skipped".to_string(), json!(telemetry.skipped)),
			(
				"duration_ms".to_string(),
				json!(duration_millis(telemetry.duration)),
			),
			("outcome".to_string(), json!(telemetry.outcome.as_str())),
			(
				"error_kind".to_string(),
				json!(telemetry.error.map(error_kind)),
			),
		]);

		self.capture("command_step", attributes);
	}

	fn local_jsonl(path: PathBuf) -> Self {
		Self::LocalJsonl { path }
	}

	fn capture(&self, name: &'static str, attributes: BTreeMap<String, serde_json::Value>) {
		let Self::LocalJsonl { path } = self else {
			return;
		};
		if let Err(error) = write_event(path, name, attributes) {
			tracing::debug!(?error, path = %path.display(), "failed to write local telemetry event");
		}
	}
}

impl TelemetryOutcome {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Success => "success",
			Self::Skipped => "skipped",
			Self::Error => "error",
		}
	}
}

#[derive(Serialize)]
struct LocalOpenTelemetryEvent {
	resource: ResourceAttributes,
	scope: InstrumentationScope,
	time_unix_nano: u128,
	severity_text: &'static str,
	body: EventBody,
	attributes: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize)]
struct ResourceAttributes {
	#[serde(rename = "service.name")]
	service_name: &'static str,
	#[serde(rename = "service.version")]
	service_version: &'static str,
}

#[derive(Serialize)]
struct InstrumentationScope {
	name: &'static str,
	version: &'static str,
}

#[derive(Serialize)]
struct EventBody {
	#[serde(rename = "string_value")]
	value: &'static str,
}

fn write_event(
	path: &Path,
	name: &'static str,
	attributes: BTreeMap<String, serde_json::Value>,
) -> std::io::Result<()> {
	if let Some(parent) = path
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
	{
		fs::create_dir_all(parent)?;
	}
	let event = LocalOpenTelemetryEvent {
		resource: ResourceAttributes {
			service_name: "monochange",
			service_version: env!("CARGO_PKG_VERSION"),
		},
		scope: InstrumentationScope {
			name: TELEMETRY_SCOPE_NAME,
			version: TELEMETRY_SCOPE_VERSION,
		},
		time_unix_nano: timestamp_unix_nano(),
		severity_text: "INFO",
		body: EventBody { value: name },
		attributes,
	};
	let mut line = serde_json::to_vec(&event).map_err(std::io::Error::other)?;
	line.push(b'\n');

	let mut file = OpenOptions::new().create(true).append(true).open(path)?;
	file.write_all(&line)?;
	Ok(())
}

fn timestamp_unix_nano() -> u128 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |duration| duration.as_nanos())
}

fn duration_millis(duration: Duration) -> u128 {
	duration.as_millis()
}

fn default_telemetry_file() -> PathBuf {
	if let Some(state_home) = env::var_os("XDG_STATE_HOME") {
		return PathBuf::from(state_home)
			.join("monochange")
			.join("telemetry.jsonl");
	}
	if let Some(home) = env::var_os("HOME") {
		return PathBuf::from(home)
			.join(".local")
			.join("state")
			.join("monochange")
			.join("telemetry.jsonl");
	}
	PathBuf::from(".monochange").join("telemetry.jsonl")
}

fn command_source(command_name: &str) -> &'static str {
	if command_name.starts_with("step:") {
		"generated_step"
	} else {
		"configured"
	}
}

fn error_kind(error: &MonochangeError) -> &'static str {
	match error {
		MonochangeError::Io(_) | MonochangeError::IoSource { .. } => "io_error",
		MonochangeError::Config(_) => "config_error",
		MonochangeError::Discovery(_) => "discovery_error",
		MonochangeError::Diagnostic(_) => "diagnostic_error",
		MonochangeError::Parse { .. } => "parse_error",
		MonochangeError::Interactive { .. } => "interactive_error",
		MonochangeError::Cancelled => "cancelled",
		_ => "unknown_error",
	}
}

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;
