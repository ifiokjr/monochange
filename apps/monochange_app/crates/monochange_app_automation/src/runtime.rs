//! Runtime wiring for background release automation workers.

use std::time::Duration as StdDuration;

use sqlx::PgPool;
use tokio::task::JoinHandle;
use tracing::error;
use tracing::info;

use crate::AutomationError;
use crate::DryRunGitHubAutomationClient;
use crate::DryRunReleasePlanner;
use crate::PostgresReleaseJobStore;
use crate::ReleaseWorker;
use crate::SystemClock;

const AUTOMATION_ENV: &str = "MONOCHANGE_APP_AUTOMATION";
const AUTOMATION_MODE_ENV: &str = "MONOCHANGE_APP_AUTOMATION_MODE";
const AUTOMATION_TICK_SECONDS_ENV: &str = "MONOCHANGE_APP_AUTOMATION_TICK_SECONDS";
const AUTOMATION_WORKER_ID_ENV: &str = "MONOCHANGE_APP_AUTOMATION_WORKER_ID";
const DEFAULT_TICK_SECONDS: u64 = 30;

/// Supported background automation runtime modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationRuntimeMode {
	/// Run the scheduler pipeline without dispatching GitHub writes.
	DryRun,
}

/// Background automation runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationRuntimeConfig {
	pub enabled: bool,
	pub mode: AutomationRuntimeMode,
	pub tick_interval: StdDuration,
	pub worker_id: String,
}

impl AutomationRuntimeConfig {
	/// Parse runtime configuration from process environment variables.
	pub fn from_env() -> Result<Self, AutomationError> {
		Self::from_env_reader(|key| std::env::var(key).ok())
	}

	/// Parse runtime configuration from an injected environment reader.
	pub fn from_env_reader(get: impl Fn(&str) -> Option<String>) -> Result<Self, AutomationError> {
		let automation = get(AUTOMATION_ENV);
		let mode = get(AUTOMATION_MODE_ENV);
		let enabled = automation
			.as_deref()
			.map(parse_enabled)
			.transpose()?
			.unwrap_or(false);
		let mode = parse_mode(automation.as_deref(), mode.as_deref())?;
		let tick_interval = get(AUTOMATION_TICK_SECONDS_ENV)
			.as_deref()
			.map(parse_tick_interval)
			.transpose()?
			.unwrap_or_else(|| StdDuration::from_secs(DEFAULT_TICK_SECONDS));
		let worker_id = get(AUTOMATION_WORKER_ID_ENV)
			.filter(|value| !value.trim().is_empty())
			.unwrap_or_else(default_worker_id);

		Ok(Self {
			enabled,
			mode,
			tick_interval,
			worker_id,
		})
	}
}

impl Default for AutomationRuntimeConfig {
	fn default() -> Self {
		Self {
			enabled: false,
			mode: AutomationRuntimeMode::DryRun,
			tick_interval: StdDuration::from_secs(DEFAULT_TICK_SECONDS),
			worker_id: default_worker_id(),
		}
	}
}

/// Spawn the configured PostgreSQL-backed automation worker.
///
/// Returns `None` when automation is disabled. The current supported mode is
/// dry-run so local app startup can exercise durable scheduling without GitHub
/// credentials or repository writes.
pub fn spawn_postgres_automation_worker(
	pool: PgPool,
	config: AutomationRuntimeConfig,
) -> Option<JoinHandle<()>> {
	if !config.enabled {
		info!("release automation worker disabled");
		return None;
	}

	Some(match config.mode {
		AutomationRuntimeMode::DryRun => spawn_dry_run_worker(pool, config),
	})
}

fn spawn_dry_run_worker(pool: PgPool, config: AutomationRuntimeConfig) -> JoinHandle<()> {
	tokio::spawn(async move {
		info!(
			worker_id = %config.worker_id,
			tick_seconds = config.tick_interval.as_secs(),
			"starting dry-run release automation worker"
		);

		let worker = ReleaseWorker::new(
			PostgresReleaseJobStore::new(pool),
			DryRunGitHubAutomationClient,
			DryRunReleasePlanner,
			SystemClock,
			config.worker_id,
		);

		loop {
			match worker.tick().await {
				Ok(outcome) => info!(?outcome, "release automation worker tick completed"),
				Err(error) => error!(%error, "release automation worker tick failed"),
			}

			tokio::time::sleep(config.tick_interval).await;
		}
	})
}

fn parse_enabled(value: &str) -> Result<bool, AutomationError> {
	match value.trim().to_ascii_lowercase().as_str() {
		"1" | "true" | "on" | "yes" | "dry-run" | "dry_run" => Ok(true),
		"0" | "false" | "off" | "no" | "disabled" | "disable" => Ok(false),
		other => {
			Err(AutomationError::store(format!(
				"invalid {AUTOMATION_ENV} value {other:?}",
			)))
		}
	}
}

fn parse_mode(
	automation: Option<&str>,
	mode: Option<&str>,
) -> Result<AutomationRuntimeMode, AutomationError> {
	let value = mode.or(automation).unwrap_or("dry-run");
	match value.trim().to_ascii_lowercase().as_str() {
		"" | "1" | "true" | "on" | "yes" | "dry-run" | "dry_run" => {
			Ok(AutomationRuntimeMode::DryRun)
		}
		"0" | "false" | "off" | "no" | "disabled" | "disable" => Ok(AutomationRuntimeMode::DryRun),
		other => {
			Err(AutomationError::store(format!(
				"unsupported {AUTOMATION_MODE_ENV} value {other:?}; only dry-run is implemented"
			)))
		}
	}
}

fn parse_tick_interval(value: &str) -> Result<StdDuration, AutomationError> {
	let seconds: u64 = value.trim().parse().map_err(|error| {
		AutomationError::store(format!(
			"invalid {AUTOMATION_TICK_SECONDS_ENV} value {value:?}: {error}",
		))
	})?;

	if seconds == 0 {
		return Err(AutomationError::store(format!(
			"{AUTOMATION_TICK_SECONDS_ENV} must be greater than zero",
		)));
	}

	Ok(StdDuration::from_secs(seconds))
}

fn default_worker_id() -> String {
	format!("monochange-app-{}", std::process::id())
}

#[cfg(test)]
mod tests {
	use std::collections::HashMap;

	use super::*;

	#[test]
	fn env_defaults_to_disabled_dry_run_worker() {
		let config = AutomationRuntimeConfig::from_env_reader(|_| None).unwrap();

		assert!(!config.enabled);
		assert_eq!(config.mode, AutomationRuntimeMode::DryRun);
		assert_eq!(config.tick_interval, StdDuration::from_secs(30));
		assert!(config.worker_id.starts_with("monochange-app-"));
	}

	#[test]
	fn env_enables_dry_run_worker_with_overrides() {
		let env = HashMap::from([
			(AUTOMATION_ENV, "dry-run"),
			(AUTOMATION_TICK_SECONDS_ENV, "5"),
			(AUTOMATION_WORKER_ID_ENV, "worker-test"),
		]);
		let config = AutomationRuntimeConfig::from_env_reader(|key| {
			env.get(key).map(std::string::ToString::to_string)
		})
		.unwrap();

		assert!(config.enabled);
		assert_eq!(config.mode, AutomationRuntimeMode::DryRun);
		assert_eq!(config.tick_interval, StdDuration::from_secs(5));
		assert_eq!(config.worker_id, "worker-test");
	}

	#[test]
	fn env_rejects_unknown_mode() {
		let env = HashMap::from([(AUTOMATION_ENV, "true"), (AUTOMATION_MODE_ENV, "write")]);
		let error = AutomationRuntimeConfig::from_env_reader(|key| {
			env.get(key).map(std::string::ToString::to_string)
		})
		.unwrap_err();

		assert!(error.message.contains("only dry-run is implemented"));
	}
}
