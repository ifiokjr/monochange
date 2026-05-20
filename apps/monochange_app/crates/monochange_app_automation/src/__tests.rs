use chrono::Duration;
use chrono::TimeZone;
use chrono::Utc;
use rstest::rstest;
use uuid::Uuid;

use crate::AutomationCapability;
use crate::AutomationError;
use crate::DurationMinutes;
use crate::GitHubAppPermissions;
use crate::GitHubPermission;
use crate::JobStatus;
use crate::PermissionLevel;
use crate::ReleaseCadence;
use crate::ReleaseJobStore;
use crate::ReleaseRepository;
use crate::ReleaseSchedule;
use crate::ReleaseWorker;
use crate::WorkerTickOutcome;
use crate::testing::FakeGitHubAutomationClient;
use crate::testing::FakeReleasePlanner;
use crate::testing::FixedClock;
use crate::testing::InMemoryReleaseJobStore;

fn at(hour: u32) -> chrono::DateTime<Utc> {
	Utc.with_ymd_and_hms(2026, 1, 1, hour, 0, 0)
		.single()
		.expect("valid test timestamp")
}

fn repository() -> ReleaseRepository {
	ReleaseRepository {
		db_id: 42,
		github_repo_id: 1_234,
		full_name: "monochange/demo".to_string(),
		default_branch: "main".to_string(),
		github_installation_id: 99,
	}
}

fn due_schedule() -> ReleaseSchedule {
	ReleaseSchedule {
		id: 7,
		repository: repository(),
		enabled: true,
		cadence: ReleaseCadence::daily(),
		next_run_at: at(0),
		window_batch_index: 0,
		last_enqueued_at: None,
		base_ref: "main".to_string(),
		requested_by_user_id: Some(11),
	}
}

#[test]
fn staged_cadence_models_four_releases_then_cooldown() {
	let cadence = ReleaseCadence::four_batches_every_four_hours_then_daily();
	let occurrences = cadence.occurrences(at(0), 0, 6);

	assert_eq!(
		occurrences,
		vec![
			at(0),
			at(4),
			at(8),
			at(12),
			at(12) + Duration::hours(24),
			at(12) + Duration::hours(28),
		]
	);
}

#[rstest]
#[case(
	AutomationCapability::InspectRepository,
	GitHubPermission::Contents,
	PermissionLevel::Read
)]
#[case(
	AutomationCapability::CommitOnBehalfOfUser,
	GitHubPermission::Contents,
	PermissionLevel::Write
)]
#[case(
	AutomationCapability::TriggerReleaseWorkflow,
	GitHubPermission::Actions,
	PermissionLevel::Write
)]
#[case(
	AutomationCapability::ManageWorkflowFiles,
	GitHubPermission::Workflows,
	PermissionLevel::Write
)]
fn recommended_permissions_cover_capability_requirements(
	#[case] capability: AutomationCapability,
	#[case] permission: GitHubPermission,
	#[case] minimum: PermissionLevel,
) {
	let permissions = GitHubAppPermissions::recommended_for(&[capability]);

	assert!(permissions.level(permission) >= minimum);
	assert!(permissions.missing_requirements(&[capability]).is_empty());
}

#[test]
fn missing_requirements_explain_insufficient_installation_permissions() {
	let missing = GitHubAppPermissions::release_planning_read_only().missing_requirements(&[
		AutomationCapability::CommitOnBehalfOfUser,
		AutomationCapability::TriggerReleaseWorkflow,
	]);

	assert!(missing.iter().any(|requirement| {
		requirement.permission == GitHubPermission::Contents
			&& requirement.minimum == PermissionLevel::Write
	}));
	assert!(missing.iter().any(|requirement| {
		requirement.permission == GitHubPermission::Actions
			&& requirement.minimum == PermissionLevel::Write
	}));
}

#[tokio::test]
async fn enqueue_due_schedules_is_idempotent_and_advances_schedule() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(1)]);

	assert_eq!(store.enqueue_due_schedules(at(0)).await.unwrap(), 1);
	assert_eq!(store.enqueue_due_schedules(at(0)).await.unwrap(), 0);

	let schedules = store.schedules();
	assert_eq!(schedules[0].last_enqueued_at, Some(at(0)));
	assert_eq!(schedules[0].next_run_at, at(0) + Duration::hours(24));
	assert_eq!(store.jobs().len(), 1);
}

#[tokio::test]
async fn claim_next_job_skips_future_jobs() {
	let mut schedule = due_schedule();
	schedule.next_run_at = at(4);
	let store = InMemoryReleaseJobStore::new(vec![schedule]);

	assert_eq!(store.enqueue_due_schedules(at(0)).await.unwrap(), 0);
	assert!(
		store
			.claim_next_job("worker", at(0), Duration::minutes(15))
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn claim_next_job_recovers_expired_locks() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(2)]);
	store.enqueue_due_schedules(at(0)).await.unwrap();

	let first_claim = store
		.claim_next_job("worker-a", at(0), Duration::minutes(15))
		.await
		.unwrap()
		.expect("job is due");
	assert_eq!(first_claim.attempts, 1);
	assert!(
		store
			.claim_next_job(
				"worker-b",
				at(0) + Duration::minutes(5),
				Duration::minutes(15)
			)
			.await
			.unwrap()
			.is_none()
	);

	let second_claim = store
		.claim_next_job(
			"worker-b",
			at(0) + Duration::minutes(16),
			Duration::minutes(15),
		)
		.await
		.unwrap()
		.expect("expired lock can be reclaimed");

	assert_eq!(second_claim.attempts, 2);
	assert_eq!(second_claim.locked_by.as_deref(), Some("worker-b"));
}

#[tokio::test]
async fn worker_tick_marks_success_when_planner_and_github_succeed() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(3)]);
	let planner = FakeReleasePlanner::success();
	let github = FakeGitHubAutomationClient::success();
	let worker = ReleaseWorker::new(
		store.clone(),
		github.clone(),
		planner.clone(),
		FixedClock::new(at(0)),
		"worker",
	);

	let outcome = worker.tick().await.unwrap();

	assert_eq!(
		outcome,
		WorkerTickOutcome::Succeeded {
			enqueued: 1,
			job_id: Uuid::from_u128(3),
		}
	);
	assert_eq!(store.jobs()[0].status, JobStatus::Succeeded);
	assert_eq!(planner.calls().len(), 1);
	assert_eq!(github.calls().len(), 1);
	assert_eq!(
		store.results()[&Uuid::from_u128(3)].external_id.as_deref(),
		Some("workflow-run-123")
	);
}

#[tokio::test]
async fn worker_tick_marks_no_change_plan_success_without_github_dispatch() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(4)]);
	let planner = FakeReleasePlanner::no_changes();
	let github = FakeGitHubAutomationClient::success();
	let worker = ReleaseWorker::new(
		store.clone(),
		github.clone(),
		planner,
		FixedClock::new(at(0)),
		"worker",
	);

	let outcome = worker.tick().await.unwrap();

	assert!(matches!(outcome, WorkerTickOutcome::Succeeded { .. }));
	assert_eq!(store.jobs()[0].status, JobStatus::Succeeded);
	assert!(github.calls().is_empty());
	assert_eq!(
		store.results()[&Uuid::from_u128(4)].summary,
		"no changes to release"
	);
}

#[tokio::test]
async fn worker_tick_marks_retryable_on_transient_planner_failure() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(5)]);
	let planner =
		FakeReleasePlanner::new([Err(AutomationError::planner("temporary planner outage"))]);
	let github = FakeGitHubAutomationClient::success();
	let worker = ReleaseWorker::new(
		store.clone(),
		github,
		planner,
		FixedClock::new(at(0)),
		"worker",
	);

	let outcome = worker.tick().await.unwrap();

	assert_eq!(
		outcome,
		WorkerTickOutcome::Retried {
			enqueued: 1,
			job_id: Uuid::from_u128(5),
			next_run_at: at(0) + Duration::minutes(1),
		}
	);
	let job = &store.jobs()[0];
	assert_eq!(job.status, JobStatus::Retryable);
	assert_eq!(job.run_after, at(0) + Duration::minutes(1));
	assert_eq!(job.last_error.as_deref(), Some("temporary planner outage"));
}

#[tokio::test]
async fn worker_tick_marks_dead_after_max_attempts() {
	let store =
		InMemoryReleaseJobStore::new(vec![due_schedule()]).with_job_ids([Uuid::from_u128(6)]);
	let planner =
		FakeReleasePlanner::new((0..5).map(|_| Err(AutomationError::planner("still down"))));
	let github = FakeGitHubAutomationClient::success();
	let clock = FixedClock::new(at(0));
	let worker = ReleaseWorker::new(store.clone(), github, planner, clock.clone(), "worker");

	for _ in 0..4 {
		let WorkerTickOutcome::Retried { next_run_at, .. } = worker.tick().await.unwrap() else {
			panic!("expected retry outcome before max attempts");
		};
		clock.set(next_run_at);
	}

	let outcome = worker.tick().await.unwrap();

	assert_eq!(
		outcome,
		WorkerTickOutcome::Dead {
			enqueued: 0,
			job_id: Uuid::from_u128(6),
		}
	);
	assert_eq!(store.jobs()[0].status, JobStatus::Dead);
	assert_eq!(store.jobs()[0].attempts, 5);
}

#[test]
fn duration_minutes_rejects_zero() {
	assert!(DurationMinutes::new(0).is_none());
	assert_eq!(DurationMinutes::new(30).unwrap().get(), 30);
}
