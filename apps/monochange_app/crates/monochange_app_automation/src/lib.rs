//! Testable automation primitives for GitHub App driven release orchestration.
//!
//! This crate intentionally keeps network, database, and git operations behind
//! small traits so the app can exercise release scheduling behavior without a
//! real GitHub App installation or a fixture repository.

pub mod cadence;
pub mod dry_run;
pub mod permissions;
pub mod postgres_store;
pub mod runtime;
pub mod scheduler;
pub mod testing;

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

pub use cadence::DurationMinutes;
pub use cadence::ReleaseCadence;
pub use dry_run::DryRunGitHubAutomationClient;
pub use dry_run::DryRunReleasePlanner;
pub use permissions::AutomationCapability;
pub use permissions::GitHubAppPermissions;
pub use permissions::GitHubPermission;
pub use permissions::PermissionLevel;
pub use permissions::PermissionRequirement;
pub use postgres_store::PostgresReleaseJobStore;
pub use runtime::AutomationRuntimeConfig;
pub use runtime::AutomationRuntimeMode;
pub use runtime::spawn_postgres_automation_worker;
pub use scheduler::AutomationError;
pub use scheduler::AutomationErrorKind;
pub use scheduler::Clock;
pub use scheduler::DispatchReleaseRequest;
pub use scheduler::GitHubAutomationClient;
pub use scheduler::JobResult;
pub use scheduler::JobStatus;
pub use scheduler::ReleaseDispatchOutcome;
pub use scheduler::ReleaseJob;
pub use scheduler::ReleaseJobKind;
pub use scheduler::ReleaseJobPayload;
pub use scheduler::ReleaseJobStore;
pub use scheduler::ReleasePlanInput;
pub use scheduler::ReleasePlanOutput;
pub use scheduler::ReleasePlanner;
pub use scheduler::ReleaseRepository;
pub use scheduler::ReleaseSchedule;
pub use scheduler::ReleaseWorker;
pub use scheduler::SystemClock;
pub use scheduler::WorkerTickOutcome;
