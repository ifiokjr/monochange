//! GitHub App permission requirements for monochange automation capabilities.

use serde::Deserialize;
use serde::Serialize;

/// GitHub App repository permission names monochange cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitHubPermission {
	Actions,
	Checks,
	CommitStatuses,
	Contents,
	Deployments,
	Issues,
	Metadata,
	PullRequests,
	Workflows,
}

/// GitHub App permission access level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
	None,
	Read,
	Write,
}

/// A single permission requirement for an automation capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequirement {
	pub permission: GitHubPermission,
	pub minimum: PermissionLevel,
}

impl PermissionRequirement {
	pub const fn new(permission: GitHubPermission, minimum: PermissionLevel) -> Self {
		Self {
			permission,
			minimum,
		}
	}
}

use GitHubPermission as P;
use PermissionLevel as L;

const INSPECT_REPOSITORY_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Contents, L::Read),
	PermissionRequirement::new(P::PullRequests, L::Read),
	PermissionRequirement::new(P::Issues, L::Read),
	PermissionRequirement::new(P::Actions, L::Read),
	PermissionRequirement::new(P::Checks, L::Read),
];
const CREATE_RELEASE_PULL_REQUEST_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Contents, L::Write),
	PermissionRequirement::new(P::PullRequests, L::Write),
];
const COMMIT_ON_BEHALF_OF_USER_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Contents, L::Write),
];
const TRIGGER_RELEASE_WORKFLOW_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Actions, L::Write),
	PermissionRequirement::new(P::Contents, L::Write),
];
const MONITOR_RELEASE_WORKFLOW_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Actions, L::Read),
	PermissionRequirement::new(P::Checks, L::Read),
];
const PUBLISH_GITHUB_RELEASE_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Contents, L::Write),
];
const COMMENT_RELEASED_ISSUES_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Issues, L::Write),
];
const MANAGE_WORKFLOW_FILES_REQUIREMENTS: &[PermissionRequirement] = &[
	PermissionRequirement::new(P::Metadata, L::Read),
	PermissionRequirement::new(P::Contents, L::Write),
	PermissionRequirement::new(P::Workflows, L::Write),
];

/// GitHub App permission snapshot for an installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubAppPermissions {
	pub actions: PermissionLevel,
	pub checks: PermissionLevel,
	pub commit_statuses: PermissionLevel,
	pub contents: PermissionLevel,
	pub deployments: PermissionLevel,
	pub issues: PermissionLevel,
	pub metadata: PermissionLevel,
	pub pull_requests: PermissionLevel,
	pub workflows: PermissionLevel,
}

impl GitHubAppPermissions {
	/// The minimum permission GitHub grants every installation.
	pub const fn metadata_only() -> Self {
		Self {
			actions: PermissionLevel::None,
			checks: PermissionLevel::None,
			commit_statuses: PermissionLevel::None,
			contents: PermissionLevel::None,
			deployments: PermissionLevel::None,
			issues: PermissionLevel::None,
			metadata: PermissionLevel::Read,
			pull_requests: PermissionLevel::None,
			workflows: PermissionLevel::None,
		}
	}

	/// Read-only repo inspection permissions.
	pub const fn release_planning_read_only() -> Self {
		Self {
			actions: PermissionLevel::Read,
			checks: PermissionLevel::Read,
			commit_statuses: PermissionLevel::Read,
			contents: PermissionLevel::Read,
			deployments: PermissionLevel::None,
			issues: PermissionLevel::Read,
			metadata: PermissionLevel::Read,
			pull_requests: PermissionLevel::Read,
			workflows: PermissionLevel::None,
		}
	}

	/// Recommended default for hosted release automation.
	pub const fn hosted_release_automation() -> Self {
		Self {
			actions: PermissionLevel::Write,
			checks: PermissionLevel::Read,
			commit_statuses: PermissionLevel::Write,
			contents: PermissionLevel::Write,
			deployments: PermissionLevel::Write,
			issues: PermissionLevel::Write,
			metadata: PermissionLevel::Read,
			pull_requests: PermissionLevel::Write,
			workflows: PermissionLevel::None,
		}
	}

	pub const fn level(self, permission: GitHubPermission) -> PermissionLevel {
		match permission {
			GitHubPermission::Actions => self.actions,
			GitHubPermission::Checks => self.checks,
			GitHubPermission::CommitStatuses => self.commit_statuses,
			GitHubPermission::Contents => self.contents,
			GitHubPermission::Deployments => self.deployments,
			GitHubPermission::Issues => self.issues,
			GitHubPermission::Metadata => self.metadata,
			GitHubPermission::PullRequests => self.pull_requests,
			GitHubPermission::Workflows => self.workflows,
		}
	}

	pub fn grant(&mut self, permission: GitHubPermission, level: PermissionLevel) {
		let slot = match permission {
			GitHubPermission::Actions => &mut self.actions,
			GitHubPermission::Checks => &mut self.checks,
			GitHubPermission::CommitStatuses => &mut self.commit_statuses,
			GitHubPermission::Contents => &mut self.contents,
			GitHubPermission::Deployments => &mut self.deployments,
			GitHubPermission::Issues => &mut self.issues,
			GitHubPermission::Metadata => &mut self.metadata,
			GitHubPermission::PullRequests => &mut self.pull_requests,
			GitHubPermission::Workflows => &mut self.workflows,
		};
		*slot = (*slot).max(level);
	}

	pub fn satisfies(self, requirement: PermissionRequirement) -> bool {
		self.level(requirement.permission) >= requirement.minimum
	}

	pub fn missing_requirements(
		self,
		capabilities: &[AutomationCapability],
	) -> Vec<PermissionRequirement> {
		capabilities
			.iter()
			.flat_map(|capability| capability.requirements().iter().copied())
			.fold(Vec::new(), |mut missing, requirement| {
				if !self.satisfies(requirement) && !missing.contains(&requirement) {
					missing.push(requirement);
				}
				missing
			})
	}

	pub fn recommended_for(capabilities: &[AutomationCapability]) -> Self {
		capabilities
			.iter()
			.flat_map(|capability| capability.requirements().iter().copied())
			.fold(Self::metadata_only(), |mut permissions, requirement| {
				permissions.grant(requirement.permission, requirement.minimum);
				permissions
			})
	}
}

impl Default for GitHubAppPermissions {
	fn default() -> Self {
		Self::metadata_only()
	}
}

/// High-level monochange automation capabilities that map to GitHub App permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCapability {
	InspectRepository,
	CreateReleasePullRequest,
	CommitOnBehalfOfUser,
	TriggerReleaseWorkflow,
	MonitorReleaseWorkflow,
	PublishGitHubRelease,
	CommentReleasedIssues,
	ManageWorkflowFiles,
}

impl AutomationCapability {
	pub const fn requirements(self) -> &'static [PermissionRequirement] {
		match self {
			Self::InspectRepository => INSPECT_REPOSITORY_REQUIREMENTS,
			Self::CreateReleasePullRequest => CREATE_RELEASE_PULL_REQUEST_REQUIREMENTS,
			Self::CommitOnBehalfOfUser => COMMIT_ON_BEHALF_OF_USER_REQUIREMENTS,
			Self::TriggerReleaseWorkflow => TRIGGER_RELEASE_WORKFLOW_REQUIREMENTS,
			Self::MonitorReleaseWorkflow => MONITOR_RELEASE_WORKFLOW_REQUIREMENTS,
			Self::PublishGitHubRelease => PUBLISH_GITHUB_RELEASE_REQUIREMENTS,
			Self::CommentReleasedIssues => COMMENT_RELEASED_ISSUES_REQUIREMENTS,
			Self::ManageWorkflowFiles => MANAGE_WORKFLOW_FILES_REQUIREMENTS,
		}
	}
}
