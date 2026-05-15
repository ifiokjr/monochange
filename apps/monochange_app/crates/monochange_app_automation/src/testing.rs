//! In-memory fakes for testing automation flows without GitHub, git, or SQLite.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use uuid::Uuid;

use crate::AutomationError;
use crate::Clock;
use crate::DispatchReleaseRequest;
use crate::GitHubAutomationClient;
use crate::JobResult;
use crate::JobStatus;
use crate::ReleaseDispatchOutcome;
use crate::ReleaseJob;
use crate::ReleaseJobStore;
use crate::ReleasePlanInput;
use crate::ReleasePlanOutput;
use crate::ReleasePlanner;
use crate::ReleaseSchedule;

#[derive(Debug, Default)]
struct StoreState {
	schedules: Vec<ReleaseSchedule>,
	jobs: Vec<ReleaseJob>,
	results: HashMap<Uuid, JobResult>,
	ids: VecDeque<Uuid>,
}

/// In-memory implementation of the durable job store contract.
#[derive(Debug, Clone, Default)]
pub struct InMemoryReleaseJobStore {
	state: Arc<Mutex<StoreState>>,
}

impl InMemoryReleaseJobStore {
	pub fn new(schedules: Vec<ReleaseSchedule>) -> Self {
		Self {
			state: Arc::new(Mutex::new(StoreState {
				schedules,
				..StoreState::default()
			})),
		}
	}

	pub fn with_job_ids(self, ids: impl IntoIterator<Item = Uuid>) -> Self {
		self.state
			.lock()
			.expect("in-memory store state poisoned")
			.ids = ids.into_iter().collect();
		self
	}

	pub fn schedules(&self) -> Vec<ReleaseSchedule> {
		self.state
			.lock()
			.expect("in-memory store state poisoned")
			.schedules
			.clone()
	}

	pub fn jobs(&self) -> Vec<ReleaseJob> {
		self.state
			.lock()
			.expect("in-memory store state poisoned")
			.jobs
			.clone()
	}

	pub fn results(&self) -> HashMap<Uuid, JobResult> {
		self.state
			.lock()
			.expect("in-memory store state poisoned")
			.results
			.clone()
	}

	fn next_job_id(state: &mut StoreState) -> Uuid {
		state.ids.pop_front().unwrap_or_else(Uuid::new_v4)
	}
}

#[async_trait]
impl ReleaseJobStore for InMemoryReleaseJobStore {
	async fn enqueue_due_schedules(&self, now: DateTime<Utc>) -> Result<usize, AutomationError> {
		let mut state = self.state.lock().expect("in-memory store state poisoned");
		let mut enqueued = 0;

		for index in 0..state.schedules.len() {
			if !state.schedules[index].due(now) {
				continue;
			}

			let job_id = Self::next_job_id(&mut state);
			let candidate = ReleaseJob::from_schedule(&state.schedules[index], job_id);
			let already_enqueued = state
				.jobs
				.iter()
				.any(|job| job.idempotency_key == candidate.idempotency_key);

			if !already_enqueued {
				state.jobs.push(candidate);
				enqueued += 1;
			}

			state.schedules[index].advance_after_enqueue();
		}

		Ok(enqueued)
	}

	async fn claim_next_job(
		&self,
		worker_id: &str,
		now: DateTime<Utc>,
		lock_for: Duration,
	) -> Result<Option<ReleaseJob>, AutomationError> {
		let mut state = self.state.lock().expect("in-memory store state poisoned");
		let Some(index) = state.jobs.iter().position(|job| {
			(job.status.claimable() && job.run_after <= now) || job.lock_expired(now)
		}) else {
			return Ok(None);
		};

		let job = &mut state.jobs[index];
		job.status = JobStatus::Running;
		job.locked_by = Some(worker_id.to_string());
		job.locked_until = Some(now + lock_for);
		job.attempts = job.attempts.saturating_add(1);

		Ok(Some(job.clone()))
	}

	async fn mark_succeeded(&self, job_id: Uuid, result: JobResult) -> Result<(), AutomationError> {
		let mut state = self.state.lock().expect("in-memory store state poisoned");
		let job = state
			.jobs
			.iter_mut()
			.find(|job| job.id == job_id)
			.ok_or_else(|| AutomationError::store(format!("unknown job {job_id}")))?;
		job.status = JobStatus::Succeeded;
		job.locked_by = None;
		job.locked_until = None;
		job.last_error = None;
		state.results.insert(job_id, result);
		Ok(())
	}

	async fn mark_retryable(
		&self,
		job_id: Uuid,
		error: String,
		next_run_at: DateTime<Utc>,
	) -> Result<(), AutomationError> {
		let mut state = self.state.lock().expect("in-memory store state poisoned");
		let job = state
			.jobs
			.iter_mut()
			.find(|job| job.id == job_id)
			.ok_or_else(|| AutomationError::store(format!("unknown job {job_id}")))?;
		job.status = JobStatus::Retryable;
		job.run_after = next_run_at;
		job.locked_by = None;
		job.locked_until = None;
		job.last_error = Some(error);
		Ok(())
	}

	async fn mark_dead(&self, job_id: Uuid, error: String) -> Result<(), AutomationError> {
		let mut state = self.state.lock().expect("in-memory store state poisoned");
		let job = state
			.jobs
			.iter_mut()
			.find(|job| job.id == job_id)
			.ok_or_else(|| AutomationError::store(format!("unknown job {job_id}")))?;
		job.status = JobStatus::Dead;
		job.locked_by = None;
		job.locked_until = None;
		job.last_error = Some(error);
		Ok(())
	}
}

/// Deterministic clock for scheduler tests.
#[derive(Debug, Clone)]
pub struct FixedClock {
	now: Arc<Mutex<DateTime<Utc>>>,
}

impl FixedClock {
	pub fn new(now: DateTime<Utc>) -> Self {
		Self {
			now: Arc::new(Mutex::new(now)),
		}
	}

	pub fn set(&self, now: DateTime<Utc>) {
		*self.now.lock().expect("fixed clock state poisoned") = now;
	}

	pub fn advance(&self, duration: Duration) {
		let mut now = self.now.lock().expect("fixed clock state poisoned");
		*now += duration;
	}
}

impl Clock for FixedClock {
	fn now(&self) -> DateTime<Utc> {
		*self.now.lock().expect("fixed clock state poisoned")
	}
}

/// Fake release planner with queued responses and recorded calls.
#[derive(Debug, Clone, Default)]
pub struct FakeReleasePlanner {
	responses: Arc<Mutex<VecDeque<Result<ReleasePlanOutput, AutomationError>>>>,
	calls: Arc<Mutex<Vec<ReleasePlanInput>>>,
}

impl FakeReleasePlanner {
	pub fn new(
		responses: impl IntoIterator<Item = Result<ReleasePlanOutput, AutomationError>>,
	) -> Self {
		Self {
			responses: Arc::new(Mutex::new(responses.into_iter().collect())),
			calls: Arc::default(),
		}
	}

	pub fn success() -> Self {
		Self::new([Ok(ReleasePlanOutput {
			has_changes: true,
			summary: "planned release".to_string(),
			release_branch: "monochange/release".to_string(),
			commit_message: "chore: release".to_string(),
		})])
	}

	pub fn no_changes() -> Self {
		Self::new([Ok(ReleasePlanOutput {
			has_changes: false,
			summary: "no changes to release".to_string(),
			release_branch: "monochange/release".to_string(),
			commit_message: "chore: release".to_string(),
		})])
	}

	pub fn calls(&self) -> Vec<ReleasePlanInput> {
		self.calls
			.lock()
			.expect("fake planner calls poisoned")
			.clone()
	}
}

#[async_trait]
impl ReleasePlanner for FakeReleasePlanner {
	async fn plan_release(
		&self,
		input: ReleasePlanInput,
	) -> Result<ReleasePlanOutput, AutomationError> {
		self.calls
			.lock()
			.expect("fake planner calls poisoned")
			.push(input);
		self.responses
			.lock()
			.expect("fake planner responses poisoned")
			.pop_front()
			.unwrap_or_else(|| Err(AutomationError::planner("no fake planner response queued")))
	}
}

/// Fake GitHub client with queued responses and recorded dispatch requests.
#[derive(Debug, Clone, Default)]
pub struct FakeGitHubAutomationClient {
	responses: Arc<Mutex<VecDeque<Result<ReleaseDispatchOutcome, AutomationError>>>>,
	calls: Arc<Mutex<Vec<DispatchReleaseRequest>>>,
}

impl FakeGitHubAutomationClient {
	pub fn new(
		responses: impl IntoIterator<Item = Result<ReleaseDispatchOutcome, AutomationError>>,
	) -> Self {
		Self {
			responses: Arc::new(Mutex::new(responses.into_iter().collect())),
			calls: Arc::default(),
		}
	}

	pub fn success() -> Self {
		Self::new([Ok(ReleaseDispatchOutcome {
			external_id: "workflow-run-123".to_string(),
			url: Some("https://github.com/monochange/demo/actions/runs/123".to_string()),
		})])
	}

	pub fn calls(&self) -> Vec<DispatchReleaseRequest> {
		self.calls
			.lock()
			.expect("fake github calls poisoned")
			.clone()
	}
}

#[async_trait]
impl GitHubAutomationClient for FakeGitHubAutomationClient {
	async fn dispatch_release(
		&self,
		request: DispatchReleaseRequest,
	) -> Result<ReleaseDispatchOutcome, AutomationError> {
		self.calls
			.lock()
			.expect("fake github calls poisoned")
			.push(request);
		self.responses
			.lock()
			.expect("fake github responses poisoned")
			.pop_front()
			.unwrap_or_else(|| Err(AutomationError::github("no fake github response queued")))
	}
}
