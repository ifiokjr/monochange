//! Durable release scheduling contracts and worker orchestration.

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

use crate::ReleaseCadence;

/// Error category used by the worker to decide retry behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum AutomationErrorKind {
	#[error("store error")]
	Store,
	#[error("release planner error")]
	Planner,
	#[error("github automation error")]
	GitHub,
	#[error("permission error")]
	Permission,
	#[error("permanent error")]
	Permanent,
}

/// Error returned by automation traits.
#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
#[error("{kind}: {message}")]
pub struct AutomationError {
	pub kind: AutomationErrorKind,
	pub message: String,
}

impl AutomationError {
	pub fn new(kind: AutomationErrorKind, message: impl Into<String>) -> Self {
		Self {
			kind,
			message: message.into(),
		}
	}

	pub fn store(message: impl Into<String>) -> Self {
		Self::new(AutomationErrorKind::Store, message)
	}

	pub fn planner(message: impl Into<String>) -> Self {
		Self::new(AutomationErrorKind::Planner, message)
	}

	pub fn github(message: impl Into<String>) -> Self {
		Self::new(AutomationErrorKind::GitHub, message)
	}

	pub fn permanent(message: impl Into<String>) -> Self {
		Self::new(AutomationErrorKind::Permanent, message)
	}

	pub const fn is_retryable(&self) -> bool {
		matches!(
			self.kind,
			AutomationErrorKind::Store | AutomationErrorKind::Planner | AutomationErrorKind::GitHub
		)
	}
}

/// Repository metadata needed to run a scheduled release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRepository {
	pub db_id: i32,
	pub github_repo_id: i64,
	pub full_name: String,
	pub default_branch: String,
	pub github_installation_id: i64,
}

impl ReleaseRepository {
	pub fn owner(&self) -> Option<&str> {
		self.full_name.split_once('/').map(|(owner, _)| owner)
	}

	pub fn name(&self) -> Option<&str> {
		self.full_name.split_once('/').map(|(_, name)| name)
	}
}

/// A durable schedule that can enqueue release jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseSchedule {
	pub id: i32,
	pub repository: ReleaseRepository,
	pub enabled: bool,
	pub cadence: ReleaseCadence,
	pub next_run_at: DateTime<Utc>,
	pub window_batch_index: u16,
	pub last_enqueued_at: Option<DateTime<Utc>>,
	pub base_ref: String,
	pub requested_by_user_id: Option<i32>,
}

impl ReleaseSchedule {
	pub fn due(&self, now: DateTime<Utc>) -> bool {
		self.enabled && self.next_run_at <= now
	}

	pub fn advance_after_enqueue(&mut self) {
		let next = self
			.cadence
			.next_after(self.next_run_at, self.window_batch_index);
		self.last_enqueued_at = Some(self.next_run_at);
		self.next_run_at = next.run_at;
		self.window_batch_index = next.window_batch_index;
	}
}

/// Release job kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseJobKind {
	PlanRelease,
}

/// Worker-visible job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
	Queued,
	Running,
	Succeeded,
	Retryable,
	Dead,
}

impl JobStatus {
	pub const fn claimable(self) -> bool {
		matches!(self, Self::Queued | Self::Retryable)
	}
}

/// Data needed by adapters to plan and dispatch the release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseJobPayload {
	pub repository: ReleaseRepository,
	pub base_ref: String,
	pub requested_by_user_id: Option<i32>,
	pub window_batch_index: u16,
}

/// Durable release job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseJob {
	pub id: Uuid,
	pub schedule_id: i32,
	pub kind: ReleaseJobKind,
	pub status: JobStatus,
	pub run_after: DateTime<Utc>,
	pub scheduled_for: DateTime<Utc>,
	pub attempts: u16,
	pub max_attempts: u16,
	pub locked_by: Option<String>,
	pub locked_until: Option<DateTime<Utc>>,
	pub idempotency_key: String,
	pub payload: ReleaseJobPayload,
	pub last_error: Option<String>,
}

impl ReleaseJob {
	pub fn from_schedule(schedule: &ReleaseSchedule, id: Uuid) -> Self {
		let scheduled_for = schedule.next_run_at;
		Self {
			id,
			schedule_id: schedule.id,
			kind: ReleaseJobKind::PlanRelease,
			status: JobStatus::Queued,
			run_after: scheduled_for,
			scheduled_for,
			attempts: 0,
			max_attempts: 5,
			locked_by: None,
			locked_until: None,
			idempotency_key: idempotency_key(schedule.repository.db_id, scheduled_for),
			payload: ReleaseJobPayload {
				repository: schedule.repository.clone(),
				base_ref: schedule.base_ref.clone(),
				requested_by_user_id: schedule.requested_by_user_id,
				window_batch_index: schedule.window_batch_index,
			},
			last_error: None,
		}
	}

	pub fn lock_expired(&self, now: DateTime<Utc>) -> bool {
		self.status == JobStatus::Running && self.locked_until.is_some_and(|until| until <= now)
	}
}

/// Planner input for one release job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePlanInput {
	pub job_id: Uuid,
	pub repository: ReleaseRepository,
	pub base_ref: String,
	pub scheduled_for: DateTime<Utc>,
	pub requested_by_user_id: Option<i32>,
	pub idempotency_key: String,
}

impl From<&ReleaseJob> for ReleasePlanInput {
	fn from(job: &ReleaseJob) -> Self {
		Self {
			job_id: job.id,
			repository: job.payload.repository.clone(),
			base_ref: job.payload.base_ref.clone(),
			scheduled_for: job.scheduled_for,
			requested_by_user_id: job.payload.requested_by_user_id,
			idempotency_key: job.idempotency_key.clone(),
		}
	}
}

/// Planner output that can be dispatched by the GitHub adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleasePlanOutput {
	pub has_changes: bool,
	pub summary: String,
	pub release_branch: String,
	pub commit_message: String,
}

/// Request sent to the GitHub App automation adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchReleaseRequest {
	pub job: ReleaseJob,
	pub plan: ReleasePlanOutput,
}

/// GitHub-side outcome after dispatching a release plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseDispatchOutcome {
	pub external_id: String,
	pub url: Option<String>,
}

/// Final job result persisted by the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobResult {
	pub summary: String,
	pub external_id: Option<String>,
	pub url: Option<String>,
}

/// Clock abstraction for deterministic scheduler tests.
pub trait Clock: Send + Sync {
	fn now(&self) -> DateTime<Utc>;
}

/// System clock implementation for production wiring.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
	fn now(&self) -> DateTime<Utc> {
		Utc::now()
	}
}

/// Durable job store abstraction.
#[async_trait]
pub trait ReleaseJobStore: Send + Sync {
	async fn enqueue_due_schedules(&self, now: DateTime<Utc>) -> Result<usize, AutomationError>;

	async fn claim_next_job(
		&self,
		worker_id: &str,
		now: DateTime<Utc>,
		lock_for: Duration,
	) -> Result<Option<ReleaseJob>, AutomationError>;

	async fn mark_succeeded(&self, job_id: Uuid, result: JobResult) -> Result<(), AutomationError>;

	async fn mark_retryable(
		&self,
		job_id: Uuid,
		error: String,
		next_run_at: DateTime<Utc>,
	) -> Result<(), AutomationError>;

	async fn mark_dead(&self, job_id: Uuid, error: String) -> Result<(), AutomationError>;
}

/// Release planner abstraction.
#[async_trait]
pub trait ReleasePlanner: Send + Sync {
	async fn plan_release(
		&self,
		input: ReleasePlanInput,
	) -> Result<ReleasePlanOutput, AutomationError>;
}

/// GitHub App automation abstraction.
#[async_trait]
pub trait GitHubAutomationClient: Send + Sync {
	async fn dispatch_release(
		&self,
		request: DispatchReleaseRequest,
	) -> Result<ReleaseDispatchOutcome, AutomationError>;
}

/// Summary of one worker tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerTickOutcome {
	Idle {
		enqueued: usize,
	},
	Succeeded {
		enqueued: usize,
		job_id: Uuid,
	},
	Retried {
		enqueued: usize,
		job_id: Uuid,
		next_run_at: DateTime<Utc>,
	},
	Dead {
		enqueued: usize,
		job_id: Uuid,
	},
}

/// Single-job release worker.
pub struct ReleaseWorker<S, G, P, C> {
	pub store: S,
	pub github: G,
	pub planner: P,
	pub clock: C,
	pub worker_id: String,
	pub lock_for: Duration,
}

impl<S, G, P, C> ReleaseWorker<S, G, P, C>
where
	S: ReleaseJobStore,
	G: GitHubAutomationClient,
	P: ReleasePlanner,
	C: Clock,
{
	pub fn new(store: S, github: G, planner: P, clock: C, worker_id: impl Into<String>) -> Self {
		Self {
			store,
			github,
			planner,
			clock,
			worker_id: worker_id.into(),
			lock_for: Duration::minutes(15),
		}
	}

	pub async fn tick(&self) -> Result<WorkerTickOutcome, AutomationError> {
		let now = self.clock.now();
		let enqueued = self.store.enqueue_due_schedules(now).await?;
		let Some(job) = self
			.store
			.claim_next_job(&self.worker_id, now, self.lock_for)
			.await?
		else {
			return Ok(WorkerTickOutcome::Idle { enqueued });
		};

		match self.run_job(job.clone()).await {
			Ok(result) => {
				self.store.mark_succeeded(job.id, result).await?;
				Ok(WorkerTickOutcome::Succeeded {
					enqueued,
					job_id: job.id,
				})
			}
			Err(error) if should_retry(&job, &error) => {
				let next_run_at = retry_at(now, job.attempts);
				self.store
					.mark_retryable(job.id, error.message, next_run_at)
					.await?;
				Ok(WorkerTickOutcome::Retried {
					enqueued,
					job_id: job.id,
					next_run_at,
				})
			}
			Err(error) => {
				self.store.mark_dead(job.id, error.message).await?;
				Ok(WorkerTickOutcome::Dead {
					enqueued,
					job_id: job.id,
				})
			}
		}
	}

	async fn run_job(&self, job: ReleaseJob) -> Result<JobResult, AutomationError> {
		match job.kind {
			ReleaseJobKind::PlanRelease => {
				let plan = self
					.planner
					.plan_release(ReleasePlanInput::from(&job))
					.await?;

				if !plan.has_changes {
					return Ok(JobResult {
						summary: plan.summary,
						external_id: None,
						url: None,
					});
				}

				let dispatch = self
					.github
					.dispatch_release(DispatchReleaseRequest {
						job,
						plan: plan.clone(),
					})
					.await?;

				Ok(JobResult {
					summary: plan.summary,
					external_id: Some(dispatch.external_id),
					url: dispatch.url,
				})
			}
		}
	}
}

pub fn idempotency_key(repository_id: i32, scheduled_for: DateTime<Utc>) -> String {
	format!(
		"plan_release:{repository_id}:{}",
		scheduled_for.to_rfc3339()
	)
}

fn should_retry(job: &ReleaseJob, error: &AutomationError) -> bool {
	error.is_retryable() && job.attempts < job.max_attempts
}

fn retry_at(now: DateTime<Utc>, attempts: u16) -> DateTime<Utc> {
	let exponent = u32::from(attempts.saturating_sub(1).min(5));
	let delay_minutes = 2_i64.pow(exponent);
	now + Duration::minutes(delay_minutes)
}
