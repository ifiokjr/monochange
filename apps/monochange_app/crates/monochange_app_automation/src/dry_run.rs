//! Dry-run automation adapters for local end-to-end scheduler testing.

use async_trait::async_trait;

use crate::AutomationError;
use crate::DispatchReleaseRequest;
use crate::GitHubAutomationClient;
use crate::ReleaseDispatchOutcome;
use crate::ReleasePlanInput;
use crate::ReleasePlanOutput;
use crate::ReleasePlanner;

/// Release planner that proves the scheduler pipeline works without touching git or GitHub.
#[derive(Debug, Clone, Copy, Default)]
pub struct DryRunReleasePlanner;

#[async_trait]
impl ReleasePlanner for DryRunReleasePlanner {
	async fn plan_release(
		&self,
		input: ReleasePlanInput,
	) -> Result<ReleasePlanOutput, AutomationError> {
		Ok(ReleasePlanOutput {
			has_changes: false,
			summary: format!(
				"dry run: planned release job {} for {} without dispatching GitHub automation",
				input.job_id, input.repository.full_name,
			),
			release_branch: format!("monochange/release/{}", input.job_id),
			commit_message: "chore: release".to_string(),
		})
	}
}

/// GitHub adapter that never performs network calls.
#[derive(Debug, Clone, Copy, Default)]
pub struct DryRunGitHubAutomationClient;

#[async_trait]
impl GitHubAutomationClient for DryRunGitHubAutomationClient {
	async fn dispatch_release(
		&self,
		request: DispatchReleaseRequest,
	) -> Result<ReleaseDispatchOutcome, AutomationError> {
		Ok(ReleaseDispatchOutcome {
			external_id: format!("dry-run-{}", request.job.id),
			url: None,
		})
	}
}

#[cfg(test)]
mod tests {
	use chrono::TimeZone;
	use chrono::Utc;
	use uuid::Uuid;

	use super::*;
	use crate::ReleaseRepository;

	#[tokio::test]
	async fn dry_run_planner_reports_no_dispatchable_changes() {
		let input = ReleasePlanInput {
			job_id: Uuid::nil(),
			repository: ReleaseRepository {
				db_id: 1,
				github_repo_id: 123,
				full_name: "monochange/demo".to_string(),
				default_branch: "main".to_string(),
				github_installation_id: 99,
			},
			base_ref: "main".to_string(),
			scheduled_for: Utc.with_ymd_and_hms(2026, 5, 7, 12, 0, 0).unwrap(),
			requested_by_user_id: Some(7),
			idempotency_key: "release-job:1:2026-05-07T12:00:00Z".to_string(),
		};

		let output = DryRunReleasePlanner.plan_release(input).await.unwrap();

		assert!(!output.has_changes);
		assert!(output.summary.contains("dry run"));
	}
}
