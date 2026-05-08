use std::fs;

use monochange_test_helpers::git::git;
use monochange_test_helpers::git_output_trimmed;
use temp_env::with_vars;
use tempfile::tempdir;

use super::*;

fn init_repo() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("README.md"), "hello\n")
		.unwrap_or_else(|error| panic!("write fixture: {error}"));

	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "initial"]);
	git(tempdir.path(), &["branch", "-M", "main"]);

	tempdir
}

#[test]
fn detect_uses_github_pr_environment_when_refs_exist() {
	let tempdir = init_repo();
	git(tempdir.path(), &["branch", "feature-branch"]);

	let frame = with_vars(
		[
			("GITHUB_EVENT_NAME", Some("pull_request")),
			("GITHUB_HEAD_REF", Some("feature-branch")),
			("GITHUB_BASE_REF", Some("main")),
		],
		|| ChangeFrame::detect(tempdir.path()),
	)
	.unwrap_or_else(|error| panic!("detect frame: {error}"));

	assert_eq!(
		frame,
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature-branch".to_string(),
		}
	);
}

#[test]
fn detect_ignores_unresolvable_pr_environment_for_local_repos() {
	let tempdir = init_repo();

	let frame = with_vars(
		[
			("GITHUB_EVENT_NAME", Some("pull_request")),
			("GITHUB_HEAD_REF", Some("feature-branch")),
			("GITHUB_BASE_REF", Some("main")),
		],
		|| ChangeFrame::detect(tempdir.path()),
	)
	.unwrap_or_else(|error| panic!("detect frame: {error}"));

	assert_eq!(frame, ChangeFrame::WorkingDirectory);
}

#[test]
fn detect_raw_pr_environment_ignores_non_pr_github_events() {
	let pr = with_vars(
		[
			("GITHUB_EVENT_NAME", Some("push")),
			("GITHUB_HEAD_REF", Some("feature-branch")),
			("GITHUB_BASE_REF", Some("main")),
		],
		detect_raw_pr_environment,
	);
	assert_eq!(pr, None);
}

#[test]
fn detect_raw_pr_environment_treats_travis_false_as_non_pr() {
	// Use `Some("push")` rather than `None` for `GITHUB_EVENT_NAME` because
	// `temp_env::with_vars` with `None` does not reliably unset env vars
	// under CI runners (GitHub Actions keeps `GITHUB_EVENT_NAME` set to
	// `"pull_request"` regardless). Setting it to a non-PR event value is
	// the only hermetic approach that passes in CI.
	let pr = with_vars(
		[
			("GITHUB_EVENT_NAME", Some("push")),
			("GITHUB_HEAD_REF", Some("")),
			("GITHUB_BASE_REF", Some("")),
			("GITHUB_EVENT_NUMBER", Some("")),
			("TRAVIS", Some("true")),
			("TRAVIS_PULL_REQUEST", Some("false")),
			("TRAVIS_PULL_REQUEST_BRANCH", Some("feature-branch")),
			("TRAVIS_BRANCH", Some("main")),
		],
		detect_raw_pr_environment,
	);
	assert_eq!(pr, None);
}

#[test]
fn default_branch_name_prefers_origin_head_symbolic_ref() {
	let tempdir = init_repo();
	let root = tempdir.path();
	git(root, &["checkout", "-b", "develop"]);
	let head = git_output_trimmed(root, &["rev-parse", "HEAD"]);
	git(root, &["update-ref", "refs/remotes/origin/develop", &head]);
	git(
		root,
		&[
			"symbolic-ref",
			"refs/remotes/origin/HEAD",
			"refs/remotes/origin/develop",
		],
	);

	assert_eq!(
		default_branch_name(root).unwrap_or_else(|error| panic!("default branch name: {error}")),
		"develop"
	);
}

#[test]
fn get_merge_base_returns_actual_merge_base_when_available() {
	let tempdir = init_repo();
	let root = tempdir.path();
	let base_commit = git_output_trimmed(root, &["rev-parse", "HEAD"]);
	git(root, &["checkout", "-b", "feature"]);
	fs::write(root.join("feature.txt"), "feature\n")
		.unwrap_or_else(|error| panic!("write feature file: {error}"));
	git(root, &["add", "feature.txt"]);
	git(root, &["commit", "-m", "feature"]);
	git(root, &["checkout", "main"]);
	fs::write(root.join("main.txt"), "main\n")
		.unwrap_or_else(|error| panic!("write main file: {error}"));
	git(root, &["add", "main.txt"]);
	git(root, &["commit", "-m", "main"]);

	assert_eq!(
		get_merge_base(root, "main", "feature")
			.unwrap_or_else(|error| panic!("merge base: {error}")),
		base_commit
	);
}

#[test]
fn changed_files_distinguishes_working_directory_and_staged_only() {
	let tempdir = init_repo();
	let root = tempdir.path();
	fs::write(root.join("staged.txt"), "staged\n")
		.unwrap_or_else(|error| panic!("write staged file: {error}"));
	git(root, &["add", "staged.txt"]);
	fs::write(root.join("README.md"), "hello updated\n")
		.unwrap_or_else(|error| panic!("write unstaged tracked file: {error}"));

	assert_eq!(
		ChangeFrame::StagedOnly
			.changed_files(root)
			.unwrap_or_else(|error| panic!("staged changed files: {error}")),
		vec![std::path::PathBuf::from("staged.txt")]
	);
	assert_eq!(
		ChangeFrame::WorkingDirectory
			.changed_files(root)
			.unwrap_or_else(|error| panic!("working changed files: {error}")),
		vec![
			std::path::PathBuf::from("README.md"),
			std::path::PathBuf::from("staged.txt"),
		]
	);
}

#[test]
fn resolve_pr_source_branch_falls_back_to_head_for_detached_repos() {
	let tempdir = init_repo();
	let head = git_output_trimmed(tempdir.path(), &["rev-parse", "HEAD"]);
	git(tempdir.path(), &["checkout", &head]);

	assert_eq!(
		resolve_pr_source_branch(tempdir.path(), "missing-branch"),
		Some("HEAD".to_string())
	);
}

#[test]
fn revision_helpers_return_false_for_missing_repositories() {
	let missing = tempdir()
		.unwrap_or_else(|error| panic!("tempdir: {error}"))
		.path()
		.join("missing");

	assert!(!revision_exists(&missing, "HEAD"));
	assert!(!is_detached_head(&missing));
}

#[test]
fn change_frame_display() {
	assert_eq!(
		ChangeFrame::WorkingDirectory.to_string(),
		"working directory"
	);

	assert_eq!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.to_string(),
		"main...feature"
	);

	assert_eq!(
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.to_string(),
		"PR: main <- feature"
	);
}

#[test]
fn revision_ranges() {
	assert_eq!(ChangeFrame::WorkingDirectory.revision_range(), "HEAD");
	assert_eq!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.revision_range(),
		"main...feature"
	);
	assert_eq!(
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.revision_range(),
		"main...feature"
	);
	assert_eq!(ChangeFrame::StagedOnly.revision_range(), "--staged");
}

#[test]
fn includes_unstaged() {
	assert!(ChangeFrame::WorkingDirectory.includes_unstaged());
	assert!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.includes_unstaged()
	);
	assert!(!ChangeFrame::StagedOnly.includes_unstaged());
}

#[test]
fn includes_staged() {
	assert!(ChangeFrame::WorkingDirectory.includes_staged());
	assert!(ChangeFrame::StagedOnly.includes_staged());
	assert!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.includes_staged()
	);
	assert!(
		!ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.includes_staged()
	);
}

#[test]
fn test_base_revision() {
	assert_eq!(ChangeFrame::WorkingDirectory.base_revision(), Some("HEAD"));
	assert_eq!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.base_revision(),
		Some("main")
	);
	assert_eq!(
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.base_revision(),
		Some("main")
	);
	assert_eq!(
		ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.base_revision(),
		Some("v1.0.0")
	);
	assert_eq!(ChangeFrame::StagedOnly.base_revision(), Some("HEAD"));
}

#[test]
fn test_head_revision() {
	assert_eq!(ChangeFrame::WorkingDirectory.head_revision(), None);
	assert_eq!(
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		}
		.head_revision(),
		Some("feature")
	);
	assert_eq!(
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.head_revision(),
		Some("feature")
	);
	assert_eq!(
		ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.head_revision(),
		Some("v2.0.0")
	);
	assert_eq!(ChangeFrame::StagedOnly.head_revision(), Some("HEAD"));
}

#[test]
fn test_pull_request_includes_staged() {
	// PullRequest frame does NOT include staged changes
	assert!(
		!ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.includes_staged()
	);
}

#[test]
fn test_custom_range_display() {
	assert_eq!(
		ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.to_string(),
		"v1.0.0...v2.0.0"
	);
}

#[test]
fn test_staged_only_revision_range() {
	assert_eq!(ChangeFrame::StagedOnly.revision_range(), "--staged");
}

#[test]
fn test_custom_range_revision_range() {
	assert_eq!(
		ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.revision_range(),
		"v1.0.0...v2.0.0"
	);
}

#[test]
fn test_pull_request_includes_unstaged() {
	assert!(
		!ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		}
		.includes_unstaged()
	);
}

#[test]
fn test_custom_range_includes_unstaged() {
	assert!(
		!ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		}
		.includes_unstaged()
	);
}

#[test]
fn test_frame_error_conversions() {
	let frame_error = FrameError::Git("test error".to_string());
	let monochange_error: MonochangeError = frame_error.into();
	assert!(matches!(monochange_error, MonochangeError::Discovery(_)));
}

#[test]
fn test_pr_environment_display_debug() {
	let pr_env = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	// Test that debug formatting works
	let debug_str = format!("{pr_env:?}");
	assert!(debug_str.contains("PrEnvironment"));
	assert!(debug_str.contains("feature"));
	assert!(debug_str.contains("main"));
}

#[test]
fn test_change_frame_custom_range() {
	let frame = ChangeFrame::CustomRange {
		base: "v1.0.0".to_string(),
		head: "v2.0.0".to_string(),
	};
	assert_eq!(frame.base_revision(), Some("v1.0.0"));
	assert_eq!(frame.head_revision(), Some("v2.0.0"));
	assert!(!frame.includes_unstaged());
	assert!(!frame.includes_staged());
}

#[test]
fn test_change_frame_staged_only() {
	let frame = ChangeFrame::StagedOnly;
	assert_eq!(frame.base_revision(), Some("HEAD"));
	assert_eq!(frame.head_revision(), Some("HEAD"));
	assert!(!frame.includes_unstaged());
	assert!(frame.includes_staged());
}

#[test]
fn test_change_frame_branch_range() {
	let frame = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "feature".to_string(),
	};
	assert_eq!(frame.base_revision(), Some("main"));
	assert_eq!(frame.head_revision(), Some("feature"));
	assert!(frame.includes_unstaged());
	assert!(frame.includes_staged());
}

#[test]
fn test_change_frame_pull_request() {
	let frame = ChangeFrame::PullRequest {
		target: "main".to_string(),
		pr_branch: "feature".to_string(),
	};
	assert_eq!(frame.base_revision(), Some("main"));
	assert_eq!(frame.head_revision(), Some("feature"));
	assert!(!frame.includes_unstaged());
	// PullRequest does NOT include staged changes
	assert!(!frame.includes_staged());
}

#[test]
fn test_change_frame_working_directory() {
	let frame = ChangeFrame::WorkingDirectory;
	assert_eq!(frame.base_revision(), Some("HEAD"));
	assert_eq!(frame.head_revision(), None);
	assert!(frame.includes_unstaged());
	assert!(frame.includes_staged());
}

#[test]
fn test_change_frame_serialize_deserialize() {
	// Test that ChangeFrame can be serialized and deserialized
	let frame = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "feature".to_string(),
	};
	let json = serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
	let deserialized: ChangeFrame =
		serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
	assert_eq!(frame.to_string(), deserialized.to_string());
}

#[test]
fn test_change_frame_working_directory_serialize() {
	let frame = ChangeFrame::WorkingDirectory;
	let json = serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
	let deserialized: ChangeFrame =
		serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
	assert!(matches!(deserialized, ChangeFrame::WorkingDirectory));
}

#[test]
fn test_change_frame_staged_only_serialize() {
	let frame = ChangeFrame::StagedOnly;
	let json = serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
	let deserialized: ChangeFrame =
		serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
	assert!(matches!(deserialized, ChangeFrame::StagedOnly));
}

#[test]
fn test_change_frame_pull_request_serialize() {
	let frame = ChangeFrame::PullRequest {
		target: "main".to_string(),
		pr_branch: "feature".to_string(),
	};
	let json = serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
	let deserialized: ChangeFrame =
		serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
	assert!(matches!(deserialized, ChangeFrame::PullRequest { .. }));
}

#[test]
fn test_change_frame_custom_range_serialize() {
	let frame = ChangeFrame::CustomRange {
		base: "v1.0.0".to_string(),
		head: "v2.0.0".to_string(),
	};
	let json = serde_json::to_string(&frame).unwrap_or_else(|e| panic!("Should serialize: {e}"));
	let deserialized: ChangeFrame =
		serde_json::from_str(&json).unwrap_or_else(|e| panic!("Should deserialize: {e}"));
	assert!(matches!(deserialized, ChangeFrame::CustomRange { .. }));
}

#[test]
fn test_pr_environment_with_none_pr_number() {
	let pr_env = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: None,
		provider: "github".to_string(),
	};
	assert_eq!(pr_env.source_branch, "feature");
	assert_eq!(pr_env.target_branch, "main");
	assert_eq!(pr_env.pr_number, None);
	assert_eq!(pr_env.provider, "github");
}

#[test]
fn test_pr_environment_clone() {
	let pr_env = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	let cloned = pr_env.clone();
	assert_eq!(cloned.source_branch, "feature");
	assert_eq!(cloned.pr_number, Some("42".to_string()));
}

#[test]
fn test_frame_error_display() {
	let git_error = FrameError::Git("test error".to_string());
	assert_eq!(git_error.to_string(), "git error: test error");

	let env_error = FrameError::Environment("env error".to_string());
	assert_eq!(env_error.to_string(), "environment error: env error");

	let invalid_error = FrameError::InvalidFrame("invalid".to_string());
	assert_eq!(invalid_error.to_string(), "invalid frame: invalid");
}

#[test]
fn test_frame_error_source() {
	// Test that FrameError implements Error trait
	let error = FrameError::Git("test".to_string());
	let _ = std::error::Error::source(&error);
}

#[test]
fn test_pr_environment_eq() {
	let pr1 = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	let pr2 = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	let pr3 = PrEnvironment {
		source_branch: "other".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	assert_eq!(pr1, pr2);
	assert_ne!(pr1, pr3);
}

#[test]
fn test_change_frame_staged_only_display() {
	let frame = ChangeFrame::StagedOnly;
	assert_eq!(frame.to_string(), "staged only");
}

#[test]
fn test_change_frame_equality() {
	let frame1 = ChangeFrame::WorkingDirectory;
	let frame2 = ChangeFrame::WorkingDirectory;
	let frame3 = ChangeFrame::StagedOnly;
	assert_eq!(frame1, frame2);
	assert_ne!(frame1, frame3);
}

#[test]
fn test_change_frame_branch_range_equality() {
	let frame1 = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "feature".to_string(),
	};
	let frame2 = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "feature".to_string(),
	};
	let frame3 = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "other".to_string(),
	};
	assert_eq!(frame1, frame2);
	assert_ne!(frame1, frame3);
}

#[test]
fn test_change_frame_serialize_roundtrip() {
	let frames = vec![
		ChangeFrame::WorkingDirectory,
		ChangeFrame::StagedOnly,
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		},
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		},
		ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "v2.0.0".to_string(),
		},
	];

	for frame in frames {
		let json =
			serde_json::to_string(&frame).unwrap_or_else(|e| panic!("should serialize: {e}"));
		let deserialized: ChangeFrame =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
		assert_eq!(frame.to_string(), deserialized.to_string());
	}
}

#[test]
fn test_pr_environment_debug() {
	let pr_env = PrEnvironment {
		source_branch: "feature".to_string(),
		target_branch: "main".to_string(),
		pr_number: Some("42".to_string()),
		provider: "github".to_string(),
	};
	let debug = format!("{pr_env:?}");
	assert!(debug.contains("PrEnvironment"));
	assert!(debug.contains("feature"));
	assert!(debug.contains("main"));
	assert!(debug.contains("github"));
}

#[test]
fn test_change_frame_debug() {
	let frame = ChangeFrame::BranchRange {
		base: "main".to_string(),
		head: "feature".to_string(),
	};
	let debug = format!("{frame:?}");
	assert!(debug.contains("BranchRange"));
	assert!(debug.contains("main"));
	assert!(debug.contains("feature"));
}
