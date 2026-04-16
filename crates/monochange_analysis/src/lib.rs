#![forbid(clippy::indexing_slicing)]

//! # `monochange_analysis`
//!
//! <!-- {=monochangeAnalysisCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_analysis` turns git diff context into artifact-aware changeset suggestions.
//!
//! Reach for this crate when you want to classify changed packages as libraries, applications, CLI tools, or mixed artifacts and then extract the most user-facing parts of the diff.
//!
//! ## Why use it?
//!
//! - convert raw changed files into package-centric semantic summaries
//! - use different heuristics for libraries, applications, and CLI tools
//! - reuse one analysis pipeline across CLI, MCP, and CI automation
//!
//! ## Best for
//!
//! - suggesting changeset boundaries before writing `.changeset/*.md` files
//! - analyzing pull-request or branch diffs in assistant workflows
//! - experimenting with artifact-aware release note generation
//!
//! ## Public entry points
//!
//! - `ChangeFrame::detect(root)` selects the git frame to analyze
//! - `detect_artifact_type(package_path)` classifies a package as a library, application, CLI tool, or mixed artifact
//! - `analyze_changes(root, frame, config)` returns package analyses and suggested changesets
//!
//! ## Scope
//!
//! - git-aware frame detection
//! - artifact classification
//! - semantic diff extraction
//! - adaptive suggestion grouping
//! <!-- {/monochangeAnalysisCrateDocs} -->

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Deserialize;
use serde::Serialize;

pub mod extractors;
pub mod frame;

pub use frame::ChangeFrame;
pub use frame::FrameError;
pub use frame::PrEnvironment;

/// Classification of package artifact types for change detection.
///
/// Different artifact types have different "user-facing" boundaries:
/// - Libraries: public API signatures
/// - Applications: UI behavior, user workflows
/// - `CliTools`: commands, flags, output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ArtifactType {
	/// Library crate with public API surface.
	Library,
	/// Application with user-facing UI/UX.
	Application,
	/// CLI tool with commands and flags.
	CliTool,
	/// Mixed artifact with both library and binary components.
	Mixed,
}

impl fmt::Display for ArtifactType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Library => write!(f, "library"),
			Self::Application => write!(f, "application"),
			Self::CliTool => write!(f, "cli"),
			Self::Mixed => write!(f, "mixed"),
		}
	}
}

/// Detect the artifact type from package structure.
///
/// # Errors
///
/// Returns an error if the package path cannot be analyzed.
pub fn detect_artifact_type(package_path: &Path) -> MonochangeResult<ArtifactType> {
	let lib_rs = package_path.join("src/lib.rs");
	let main_rs = package_path.join("src/main.rs");
	let cargo_toml = package_path.join("Cargo.toml");

	let has_lib = lib_rs.exists();
	let has_main = main_rs.exists();

	if has_lib && has_main {
		return Ok(ArtifactType::Mixed);
	}

	if has_lib {
		return Ok(ArtifactType::Library);
	}

	if has_main {
		let content = std::fs::read_to_string(&main_rs)
			.map_err(|e| MonochangeError::Io(format!("failed to read main.rs: {e}")))?;

		// Check for CLI patterns
		if content.contains("clap")
			|| content.contains("structopt")
			|| content.contains("#[derive(Parser)]")
		{
			return Ok(ArtifactType::CliTool);
		}

		// Check for web framework patterns
		if content.contains("axum")
			|| content.contains("actix")
			|| content.contains("rocket")
			|| content.contains("warp")
		{
			return Ok(ArtifactType::Application);
		}

		// Default to CLI for single-binary packages
		return Ok(ArtifactType::CliTool);
	}

	// Check Cargo.toml for crate type
	if cargo_toml.exists() {
		let content = std::fs::read_to_string(&cargo_toml)
			.map_err(|e| MonochangeError::Io(format!("failed to read Cargo.toml: {e}")))?;

		if content.contains("crate-type = [\"cdylib\"]")
			|| content.contains("crate-type = [\"staticlib\"]")
		{
			return Ok(ArtifactType::Library);
		}

		if content.contains("[[bin]]") {
			return Ok(ArtifactType::CliTool);
		}
	}

	Err(MonochangeError::Discovery(format!(
		"unable to determine artifact type for {}",
		package_path.display()
	)))
}

/// Represents a detected semantic change in the codebase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticChange {
	/// Library API changes
	Api(ApiChange),
	/// Application UI/UX changes
	App(AppChange),
	/// CLI tool changes
	Cli(CliChange),
	/// Configuration changes
	Config(ConfigChange),
	/// Unknown or unclassified change
	Unknown { path: PathBuf, description: String },
}

/// Changes to library public API surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApiChange {
	/// The kind of API change
	pub kind: ApiChangeKind,
	/// Visibility level (public, crate, etc.)
	pub visibility: Visibility,
	/// Name of the item
	pub name: String,
	/// Full signature if available
	pub signature: Option<String>,
	/// Doc comment if present
	pub doc_comment: Option<String>,
	/// Whether this is a breaking change
	pub is_breaking: bool,
	/// File path where the change occurred
	pub file_path: PathBuf,
	/// Line number (if available)
	pub line_number: Option<usize>,
}

/// Visibility levels for API items.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
	/// `pub` - fully public
	Public,
	/// `pub(crate)` - crate-wide
	Crate,
	/// `pub(super)` - parent module
	Super,
	/// `pub(in path)` - restricted
	Restricted,
	/// Private (no visibility modifier)
	Private,
}

impl fmt::Display for Visibility {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Public => write!(f, "pub"),
			Self::Crate => write!(f, "pub(crate)"),
			Self::Super => write!(f, "pub(super)"),
			Self::Restricted => write!(f, "pub(in ...)"),
			Self::Private => write!(f, "private"),
		}
	}
}

/// Kinds of API changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ApiChangeKind {
	FunctionAdded,
	FunctionModified,
	FunctionRemoved,
	TypeAdded,
	TypeModified,
	TypeRemoved,
	TraitAdded,
	TraitModified,
	TraitRemoved,
	ConstantAdded,
	ConstantModified,
	ConstantRemoved,
	ModuleAdded,
	ModuleRemoved,
}

/// Changes to application user-facing behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppChange {
	/// The kind of application change
	pub kind: AppChangeKind,
	/// Route path if applicable
	pub route: Option<String>,
	/// Component name if applicable
	pub component: Option<String>,
	/// Human-readable description
	pub description: String,
	/// Whether this is user-visible
	pub is_user_visible: bool,
	/// File path where the change occurred
	pub file_path: PathBuf,
}

/// Kinds of application changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum AppChangeKind {
	RouteAdded,
	RouteRemoved,
	RouteModified,
	ComponentAdded,
	ComponentModified,
	ComponentRemoved,
	ApiEndpointAdded,
	ApiEndpointModified,
	ApiEndpointRemoved,
	StateManagementChanged,
	FormValidationChanged,
	StyleChanged,
	NavigationChanged,
}

/// Changes to CLI interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CliChange {
	/// The kind of CLI change
	pub kind: CliChangeKind,
	/// Command name if applicable
	pub command: Option<String>,
	/// Flag name if applicable
	pub flag: Option<String>,
	/// Human-readable description
	pub description: String,
	/// Whether this is a breaking change
	pub is_breaking: bool,
	/// File path where the change occurred
	pub file_path: PathBuf,
}

/// Kinds of CLI changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CliChangeKind {
	CommandAdded,
	CommandRemoved,
	CommandModified,
	FlagAdded,
	FlagRemoved,
	FlagModified,
	OutputFormatChanged,
	ExitCodeChanged,
	ConfigFileChanged,
	PromptChanged,
}

/// Configuration file changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigChange {
	/// Config file name
	pub file_name: String,
	/// What changed
	pub description: String,
	/// Whether this affects user configuration
	pub is_user_facing: bool,
}

/// Thresholds for deciding when to group vs. split changes.
///
/// These thresholds are adaptive based on artifact type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GroupingThresholds {
	/// Group public API changes when count exceeds this
	pub group_public_api: usize,
	/// Group internal changes when count exceeds this
	pub group_internal: usize,
	/// Group UI components when count exceeds this
	pub group_ui: usize,
	/// Group CLI commands when count exceeds this
	pub group_commands: usize,
	/// Group documentation changes when count exceeds this
	pub group_docs: usize,
}

impl Default for GroupingThresholds {
	fn default() -> Self {
		Self {
			group_public_api: 3,
			group_internal: 5,
			group_ui: 3,
			group_commands: 2,
			group_docs: 10,
		}
	}
}

impl GroupingThresholds {
	/// Get thresholds for a specific artifact type.
	#[must_use]
	pub fn for_artifact_type(self, artifact_type: ArtifactType) -> Self {
		match artifact_type {
			ArtifactType::Library => {
				Self {
					// Libraries: stricter about public API
					group_public_api: 3,
					group_internal: 5,
					group_ui: 0, // N/A for libraries
					group_commands: 0,
					group_docs: 10,
				}
			}
			ArtifactType::Application => {
				Self {
					// Apps: UI changes are user-facing
					group_public_api: 0,
					group_internal: 3,
					group_ui: 3,
					group_commands: 0,
					group_docs: 10,
				}
			}
			ArtifactType::CliTool => {
				Self {
					// CLI: commands and flags matter
					group_public_api: 0,
					group_internal: 3,
					group_ui: 0,
					group_commands: 2,
					group_docs: 10,
				}
			}
			ArtifactType::Mixed => self, // Use defaults
		}
	}
}

/// A group of related changes that should be in a single changeset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeGroup {
	/// Package this group belongs to
	pub package_id: String,
	/// Artifact type of the package
	pub artifact_type: ArtifactType,
	/// Suggested summary line
	pub suggested_summary: String,
	/// Suggested detailed description
	pub suggested_details: Option<String>,
	/// Suggested bump level
	pub suggested_bump: BumpSuggestion,
	/// Changes in this group
	pub changes: Vec<SemanticChange>,
	/// Whether this contains breaking changes
	pub has_breaking: bool,
	/// Confidence score (0.0-1.0)
	pub confidence: f32,
}

/// Suggested version bump based on changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BumpSuggestion {
	/// No version change needed
	None,
	/// Patch - bug fixes, internal changes
	Patch,
	/// Minor - new features, backward compatible
	Minor,
	/// Major - breaking changes
	Major,
}

impl fmt::Display for BumpSuggestion {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::None => write!(f, "none"),
			Self::Patch => write!(f, "patch"),
			Self::Minor => write!(f, "minor"),
			Self::Major => write!(f, "major"),
		}
	}
}

/// Complete analysis result for a set of changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeAnalysis {
	/// The frame used for this analysis
	pub frame: ChangeFrame,
	/// Changes grouped by package
	pub package_changes: BTreeMap<String, PackageChangeAnalysis>,
	/// Overall recommendations
	pub recommendations: Vec<String>,
}

/// Analysis for a single package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageChangeAnalysis {
	/// Package identifier
	pub package_id: String,
	/// Detected artifact type
	pub artifact_type: ArtifactType,
	/// Number of direct changes
	pub direct_change_count: usize,
	/// Whether this has propagated changes
	pub has_propagated_changes: bool,
	/// Suggested changesets
	pub suggested_changesets: Vec<SuggestedChangeset>,
}

/// A suggested changeset for a package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuggestedChangeset {
	/// Suggested summary (headline)
	pub summary: String,
	/// Suggested details (body)
	pub details: Option<String>,
	/// Suggested bump level
	pub bump: BumpSuggestion,
	/// Suggested change type (if configured)
	pub change_type: Option<String>,
	/// Confidence score
	pub confidence: f32,
	/// API changes (for libraries)
	pub api_changes: Vec<ApiChange>,
	/// Number of changes grouped in this suggestion
	pub grouped_count: usize,
	/// Files affected
	pub files_changed: Vec<PathBuf>,
	/// Whether this contains breaking changes
	pub has_breaking_changes: bool,
	/// Whether before/after examples are recommended
	pub before_after_suggested: bool,
}

/// Configuration for change analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
	/// Detection level for analysis
	pub detection_level: DetectionLevel,
	/// Grouping thresholds
	pub thresholds: GroupingThresholds,
	/// Maximum number of suggestions to generate
	pub max_suggestions: usize,
}

impl Default for AnalysisConfig {
	fn default() -> Self {
		Self {
			detection_level: DetectionLevel::Signature,
			thresholds: GroupingThresholds::default(),
			max_suggestions: 10,
		}
	}
}

/// Level of analysis detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetectionLevel {
	/// File-level only (fastest)
	Basic,
	/// Extract signatures (balanced)
	Signature,
	/// Full AST parsing (most detailed, slowest)
	Semantic,
}

/// Analyze changes within a given frame.
///
/// # Errors
///
/// Returns an error if the analysis cannot be completed.
pub fn analyze_changes(
	_repo_root: &Path,
	frame: &ChangeFrame,
	_config: &AnalysisConfig,
) -> MonochangeResult<ChangeAnalysis> {
	// This is a placeholder for the actual implementation
	// The full implementation would:
	// 1. Get the diff for the frame
	// 2. Map changed files to packages
	// 3. Extract semantic changes based on artifact type
	// 4. Apply grouping thresholds
	// 5. Generate suggestions

	let package_changes = BTreeMap::new();
	let recommendations = Vec::new();

	Ok(ChangeAnalysis {
		frame: frame.clone(),
		package_changes,
		recommendations,
	})
}

/// Group changes based on thresholds and artifact type.
///
/// # Errors
///
/// Returns an error if grouping fails.
pub fn group_changes(
	changes: Vec<SemanticChange>,
	artifact_type: ArtifactType,
	thresholds: &GroupingThresholds,
) -> MonochangeResult<Vec<ChangeGroup>> {
	let thresholds = thresholds.for_artifact_type(artifact_type);
	let mut groups = Vec::new();

	// Separate breaking changes first
	let (breaking, non_breaking): (Vec<_>, Vec<_>) = changes.into_iter().partition(|c| {
		matches!(
			c,
			SemanticChange::Api(ApiChange {
				is_breaking: true,
				..
			})
		)
	});

	// Each breaking change gets its own group
	for change in breaking {
		let summary = format!("breaking: {}", describe_change(&change));
		groups.push(ChangeGroup {
			package_id: String::new(), // Filled by caller
			artifact_type,
			suggested_summary: summary,
			suggested_details: None,
			suggested_bump: BumpSuggestion::Major,
			changes: vec![change],
			has_breaking: true,
			confidence: 0.95,
		});
	}

	// Group non-breaking changes
	let grouped = apply_grouping_logic(non_breaking, artifact_type, &thresholds);
	groups.extend(grouped);

	Ok(groups)
}

/// Describe a semantic change for display purposes.
fn describe_change(change: &SemanticChange) -> String {
	match change {
		SemanticChange::Api(api) => {
			format!("{:?} {} {}", api.kind, api.visibility, api.name)
		}
		SemanticChange::App(app) => format!("{:?} {}", app.kind, app.description),
		SemanticChange::Cli(cli) => format!("{:?} {}", cli.kind, cli.description),
		SemanticChange::Config(cfg) => format!("config change: {}", cfg.description),
		SemanticChange::Unknown { description, .. } => description.clone(),
	}
}

/// Apply grouping logic based on thresholds.
fn apply_grouping_logic(
	changes: Vec<SemanticChange>,
	artifact_type: ArtifactType,
	_thresholds: &GroupingThresholds,
) -> Vec<ChangeGroup> {
	// Placeholder implementation
	// Full implementation would:
	// 1. Categorize changes by type and proximity
	// 2. Count changes in each category
	// 3. Apply thresholds to decide group vs. separate
	// 4. Generate appropriate summaries

	changes
		.into_iter()
		.map(|change| {
			ChangeGroup {
				package_id: String::new(),
				artifact_type,
				suggested_summary: describe_change(&change),
				suggested_details: None,
				suggested_bump: BumpSuggestion::Patch,
				changes: vec![change],
				has_breaking: false,
				confidence: 0.8,
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::extractors::ExtractionResult;
	use super::extractors::SkipReason;
	use super::*;

	#[test]
	fn artifact_type_display() {
		assert_eq!(ArtifactType::Library.to_string(), "library");
		assert_eq!(ArtifactType::Application.to_string(), "application");
		assert_eq!(ArtifactType::CliTool.to_string(), "cli");
		assert_eq!(ArtifactType::Mixed.to_string(), "mixed");
	}

	#[test]
	fn bump_suggestion_display() {
		assert_eq!(BumpSuggestion::None.to_string(), "none");
		assert_eq!(BumpSuggestion::Patch.to_string(), "patch");
		assert_eq!(BumpSuggestion::Minor.to_string(), "minor");
		assert_eq!(BumpSuggestion::Major.to_string(), "major");
	}

	#[test]
	fn grouping_thresholds_default() {
		let defaults = GroupingThresholds::default();
		assert_eq!(defaults.group_public_api, 3);
		assert_eq!(defaults.group_internal, 5);
		assert_eq!(defaults.group_ui, 3);
		assert_eq!(defaults.group_commands, 2);
		assert_eq!(defaults.group_docs, 10);
	}

	#[test]
	fn grouping_thresholds_for_library() {
		let thresholds = GroupingThresholds::default();
		let library_thresholds = thresholds.for_artifact_type(ArtifactType::Library);

		assert_eq!(library_thresholds.group_public_api, 3);
		assert_eq!(library_thresholds.group_internal, 5);
		assert_eq!(library_thresholds.group_ui, 0);
		assert_eq!(library_thresholds.group_commands, 0);
	}

	#[test]
	fn grouping_thresholds_for_application() {
		let thresholds = GroupingThresholds::default();
		let app_thresholds = thresholds.for_artifact_type(ArtifactType::Application);

		assert_eq!(app_thresholds.group_public_api, 0);
		assert_eq!(app_thresholds.group_ui, 3);
		assert_eq!(app_thresholds.group_commands, 0);
	}

	#[test]
	fn grouping_thresholds_for_cli() {
		let thresholds = GroupingThresholds::default();
		let cli_thresholds = thresholds.for_artifact_type(ArtifactType::CliTool);

		assert_eq!(cli_thresholds.group_public_api, 0);
		assert_eq!(cli_thresholds.group_ui, 0);
		assert_eq!(cli_thresholds.group_commands, 2);
	}

	#[test]
	fn grouping_thresholds_for_mixed() {
		let thresholds = GroupingThresholds::default();
		let mixed_thresholds = thresholds.for_artifact_type(ArtifactType::Mixed);

		// Mixed uses defaults
		assert_eq!(mixed_thresholds.group_public_api, 3);
		assert_eq!(mixed_thresholds.group_internal, 5);
	}

	#[test]
	fn visibility_display() {
		assert_eq!(Visibility::Public.to_string(), "pub");
		assert_eq!(Visibility::Crate.to_string(), "pub(crate)");
		assert_eq!(Visibility::Super.to_string(), "pub(super)");
		assert_eq!(Visibility::Restricted.to_string(), "pub(in ...)");
		assert_eq!(Visibility::Private.to_string(), "private");
	}

	#[test]
	fn analysis_config_default() {
		let config = AnalysisConfig::default();
		assert!(matches!(config.detection_level, DetectionLevel::Signature));
		assert_eq!(config.max_suggestions, 10);
	}

	#[test]
	fn describe_change_api() {
		let api_change = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test_fn".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(42),
		};
		let change = SemanticChange::Api(api_change);
		let description = describe_change(&change);
		assert!(description.contains("FunctionAdded"));
		assert!(description.contains("test_fn"));
	}

	#[test]
	fn describe_change_app() {
		let app_change = AppChange {
			kind: AppChangeKind::RouteAdded,
			route: Some("/home".to_string()),
			component: Some("HomePage".to_string()),
			description: "Added home page".to_string(),
			is_user_visible: true,
			file_path: PathBuf::from("src/pages/home.tsx"),
		};
		let change = SemanticChange::App(app_change);
		let description = describe_change(&change);
		assert!(description.contains("RouteAdded"));
		assert!(description.contains("Added home page"));
	}

	#[test]
	fn describe_change_cli() {
		let cli_change = CliChange {
			kind: CliChangeKind::CommandAdded,
			command: Some("new-cmd".to_string()),
			flag: None,
			description: "Added new command".to_string(),
			is_breaking: false,
			file_path: PathBuf::from("src/cli.rs"),
		};
		let change = SemanticChange::Cli(cli_change);
		let description = describe_change(&change);
		assert!(description.contains("CommandAdded"));
		assert!(description.contains("Added new command"));
	}

	#[test]
	fn describe_change_config() {
		let config_change = ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed default timeout".to_string(),
			is_user_facing: true,
		};
		let change = SemanticChange::Config(config_change);
		let description = describe_change(&change);
		assert!(description.contains("config change"));
		assert!(description.contains("Changed default timeout"));
	}

	#[test]
	fn describe_change_unknown() {
		let change = SemanticChange::Unknown {
			path: PathBuf::from("src/unknown.rs"),
			description: "Some unknown change".to_string(),
		};
		let description = describe_change(&change);
		assert_eq!(description, "Some unknown change");
	}

	#[test]
	fn group_changes_with_breaking_change() {
		let api_change = ApiChange {
			kind: ApiChangeKind::FunctionRemoved,
			visibility: Visibility::Public,
			name: "old_fn".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: true,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(42),
		};
		let change = SemanticChange::Api(api_change);
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(vec![change], ArtifactType::Library, &thresholds).unwrap();

		assert_eq!(groups.len(), 1);
		assert!(
			groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.has_breaking
		);
		assert!(matches!(
			groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.suggested_bump,
			BumpSuggestion::Major
		));
	}

	#[test]
	fn group_changes_with_non_breaking_changes() {
		let api_change1 = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "new_fn1".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(42),
		};
		let api_change2 = ApiChange {
			kind: ApiChangeKind::TypeAdded,
			visibility: Visibility::Public,
			name: "NewType".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(50),
		};
		let changes = vec![
			SemanticChange::Api(api_change1),
			SemanticChange::Api(api_change2),
		];
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(changes, ArtifactType::Library, &thresholds).unwrap();

		assert_eq!(groups.len(), 2);
		assert!(
			!groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.has_breaking
		);
		assert!(
			!groups
				.get(1)
				.unwrap_or_else(|| panic!("expected at least 2 groups"))
				.has_breaking
		);
	}

	#[test]
	fn analyze_changes_placeholder() {
		let root = PathBuf::from(".");
		let frame = ChangeFrame::WorkingDirectory;
		let config = AnalysisConfig::default();
		let analysis = analyze_changes(&root, &frame, &config).unwrap();

		assert!(matches!(analysis.frame, ChangeFrame::WorkingDirectory));
		assert!(analysis.package_changes.is_empty());
		assert!(analysis.recommendations.is_empty());
	}

	#[test]
	fn semantic_change_variants_debug() {
		let api = SemanticChange::Api(ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: None,
		});
		assert!(format!("{api:?}").contains("Api"));

		let app = SemanticChange::App(AppChange {
			kind: AppChangeKind::ComponentAdded,
			route: None,
			component: Some("Button".to_string()),
			description: "Added button".to_string(),
			is_user_visible: true,
			file_path: PathBuf::from("src/components/Button.tsx"),
		});
		assert!(format!("{app:?}").contains("App"));

		let cli = SemanticChange::Cli(CliChange {
			kind: CliChangeKind::FlagAdded,
			command: Some("build".to_string()),
			flag: Some("verbose".to_string()),
			description: "Added verbose flag".to_string(),
			is_breaking: false,
			file_path: PathBuf::from("src/cli.rs"),
		});
		assert!(format!("{cli:?}").contains("Cli"));

		let config = SemanticChange::Config(ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		});
		assert!(format!("{config:?}").contains("Config"));

		let unknown = SemanticChange::Unknown {
			path: PathBuf::from("unknown.txt"),
			description: "Unknown change".to_string(),
		};
		assert!(format!("{unknown:?}").contains("Unknown"));
	}

	#[test]
	fn change_group_struct() {
		let group = ChangeGroup {
			package_id: "test-package".to_string(),
			artifact_type: ArtifactType::Library,
			suggested_summary: "Add new function".to_string(),
			suggested_details: Some("Details here".to_string()),
			suggested_bump: BumpSuggestion::Minor,
			changes: vec![],
			has_breaking: false,
			confidence: 0.9,
		};

		assert_eq!(group.package_id, "test-package");
		assert!(matches!(group.artifact_type, ArtifactType::Library));
		assert_eq!(group.suggested_summary, "Add new function");
		assert!(group.suggested_details.is_some());
		assert!(matches!(group.suggested_bump, BumpSuggestion::Minor));
		assert!(!group.has_breaking);
		assert!((group.confidence - 0.9).abs() < f32::EPSILON);
	}

	#[test]
	fn suggested_changeset_struct() {
		let changeset = SuggestedChangeset {
			summary: "Add feature".to_string(),
			details: Some("Details".to_string()),
			bump: BumpSuggestion::Minor,
			change_type: Some("feature".to_string()),
			confidence: 0.95,
			api_changes: vec![],
			grouped_count: 1,
			files_changed: vec![PathBuf::from("src/lib.rs")],
			has_breaking_changes: false,
			before_after_suggested: true,
		};

		assert_eq!(changeset.summary, "Add feature");
		assert_eq!(changeset.bump, BumpSuggestion::Minor);
		assert!(changeset.before_after_suggested);
	}

	#[test]
	fn package_change_analysis_struct() {
		let analysis = PackageChangeAnalysis {
			package_id: "my-package".to_string(),
			artifact_type: ArtifactType::Library,
			direct_change_count: 5,
			has_propagated_changes: true,
			suggested_changesets: vec![],
		};

		assert_eq!(analysis.package_id, "my-package");
		assert!(analysis.has_propagated_changes);
		assert_eq!(analysis.direct_change_count, 5);
	}

	#[test]
	fn change_analysis_struct() {
		let analysis = ChangeAnalysis {
			frame: ChangeFrame::WorkingDirectory,
			package_changes: BTreeMap::new(),
			recommendations: vec!["Recommendation 1".to_string()],
		};

		assert!(matches!(analysis.frame, ChangeFrame::WorkingDirectory));
		assert_eq!(analysis.recommendations.len(), 1);
	}

	#[test]
	fn test_detect_artifact_type_library() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("lib.rs"), "pub fn foo() {}").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Library));
	}

	#[test]
	fn test_detect_artifact_type_cli_from_clap() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("main.rs"), "use clap::Parser;").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::CliTool));
	}

	#[test]
	fn test_detect_artifact_type_application_from_axum() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("main.rs"), "use axum::Router;").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Application));
	}

	#[test]
	fn test_detect_artifact_type_application_from_actix() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("main.rs"), "use actix_web;").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Application));
	}

	#[test]
	fn test_detect_artifact_type_application_from_rocket() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("main.rs"), "use rocket;").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Application));
	}

	#[test]
	fn test_detect_artifact_type_application_from_warp() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("main.rs"), "use warp;").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Application));
	}

	#[test]
	fn test_detect_artifact_type_cli_default() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		// No clap, no web frameworks - should default to CLI
		fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::CliTool));
	}

	#[test]
	fn test_detect_artifact_type_mixed() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		fs::write(src_dir.join("lib.rs"), "").unwrap();
		fs::write(src_dir.join("main.rs"), "").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Mixed));
	}

	#[test]
	fn test_detect_artifact_type_from_cargo_toml_cdylib() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let src_dir = temp_dir.path().join("src");
		fs::create_dir(&src_dir).unwrap();
		// No lib.rs or main.rs, but has Cargo.toml with crate-type
		fs::write(
			temp_dir.path().join("Cargo.toml"),
			"crate-type = [\"cdylib\"]",
		)
		.unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Library));
	}

	#[test]
	fn test_detect_artifact_type_from_cargo_toml_staticlib() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		// No src directory, but has Cargo.toml with crate-type
		fs::write(
			temp_dir.path().join("Cargo.toml"),
			"crate-type = [\"staticlib\"]",
		)
		.unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::Library));
	}

	#[test]
	fn test_detect_artifact_type_from_cargo_toml_bin() {
		use std::fs;

		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		// No src directory, but has Cargo.toml with [[bin]]
		fs::write(temp_dir.path().join("Cargo.toml"), "[[bin]]").unwrap();

		let artifact = detect_artifact_type(temp_dir.path()).unwrap();
		assert!(matches!(artifact, ArtifactType::CliTool));
	}

	#[test]
	fn test_detect_artifact_type_error() {
		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		// Empty directory - should fail
		let result = detect_artifact_type(temp_dir.path());
		assert!(result.is_err());
	}

	#[test]
	fn test_api_change_kind_variants() {
		// Test all ApiChangeKind variants have distinct debug representations
		let kinds = vec![
			ApiChangeKind::FunctionAdded,
			ApiChangeKind::FunctionModified,
			ApiChangeKind::FunctionRemoved,
			ApiChangeKind::TypeAdded,
			ApiChangeKind::TypeModified,
			ApiChangeKind::TypeRemoved,
			ApiChangeKind::TraitAdded,
			ApiChangeKind::TraitModified,
			ApiChangeKind::TraitRemoved,
			ApiChangeKind::ConstantAdded,
			ApiChangeKind::ConstantModified,
			ApiChangeKind::ConstantRemoved,
			ApiChangeKind::ModuleAdded,
			ApiChangeKind::ModuleRemoved,
		];

		for kind in kinds {
			let debug = format!("{kind:?}");
			assert!(!debug.is_empty());
		}
	}

	#[test]
	fn test_app_change_kind_variants() {
		let kinds = vec![
			AppChangeKind::RouteAdded,
			AppChangeKind::RouteRemoved,
			AppChangeKind::RouteModified,
			AppChangeKind::ComponentAdded,
			AppChangeKind::ComponentModified,
			AppChangeKind::ComponentRemoved,
			AppChangeKind::ApiEndpointAdded,
			AppChangeKind::ApiEndpointModified,
			AppChangeKind::ApiEndpointRemoved,
			AppChangeKind::StateManagementChanged,
			AppChangeKind::FormValidationChanged,
			AppChangeKind::StyleChanged,
			AppChangeKind::NavigationChanged,
		];

		for kind in kinds {
			let debug = format!("{kind:?}");
			assert!(!debug.is_empty());
		}
	}

	#[test]
	fn test_cli_change_kind_variants() {
		let kinds = vec![
			CliChangeKind::CommandAdded,
			CliChangeKind::CommandRemoved,
			CliChangeKind::CommandModified,
			CliChangeKind::FlagAdded,
			CliChangeKind::FlagRemoved,
			CliChangeKind::FlagModified,
			CliChangeKind::OutputFormatChanged,
			CliChangeKind::ExitCodeChanged,
			CliChangeKind::ConfigFileChanged,
			CliChangeKind::PromptChanged,
		];

		for kind in kinds {
			let debug = format!("{kind:?}");
			assert!(!debug.is_empty());
		}
	}

	#[test]
	fn test_skip_reason_variants() {
		let reasons = vec![
			SkipReason::UnsupportedExtension,
			SkipReason::BinaryFile,
			SkipReason::TooLarge,
			SkipReason::ParseError("test".to_string()),
			SkipReason::NotRelevant,
		];

		for reason in reasons {
			let debug = format!("{reason:?}");
			assert!(!debug.is_empty());
		}
	}

	#[test]
	fn test_extraction_result_default() {
		let result = ExtractionResult {
			changes: vec![],
			files_analyzed: vec![],
			files_skipped: vec![],
		};
		assert!(result.changes.is_empty());
		assert!(result.files_analyzed.is_empty());
		assert!(result.files_skipped.is_empty());
	}

	#[test]
	fn test_pr_environment_struct() {
		let pr_env = PrEnvironment {
			source_branch: "feature".to_string(),
			target_branch: "main".to_string(),
			pr_number: Some("42".to_string()),
			provider: "github".to_string(),
		};
		assert_eq!(pr_env.source_branch, "feature");
		assert_eq!(pr_env.target_branch, "main");
		assert_eq!(pr_env.pr_number, Some("42".to_string()));
		assert_eq!(pr_env.provider, "github");
	}

	#[test]
	fn test_frame_error_variants() {
		let errors = vec![
			FrameError::Git("error".to_string()),
			FrameError::Environment("env error".to_string()),
			FrameError::InvalidFrame("invalid".to_string()),
		];

		for err in errors {
			assert!(!err.to_string().is_empty());
		}
	}

	#[test]
	fn test_api_change_with_signature() {
		let api = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test".to_string(),
			signature: Some("fn test(x: i32) -> bool".to_string()),
			doc_comment: Some("Test function".to_string()),
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(42),
		};
		assert!(api.signature.is_some());
		assert!(api.doc_comment.is_some());
		assert!(api.line_number.is_some());
	}

	#[test]
	fn test_analysis_config_clone() {
		let config = AnalysisConfig::default();
		let cloned = config.clone();
		assert_eq!(config.max_suggestions, cloned.max_suggestions);
	}

	#[test]
	fn test_grouping_thresholds_clone() {
		let thresholds = GroupingThresholds::default();
		let cloned = thresholds;
		assert_eq!(thresholds.group_public_api, cloned.group_public_api);
	}

	#[test]
	fn test_change_group_clone() {
		let group = ChangeGroup {
			package_id: "test".to_string(),
			artifact_type: ArtifactType::Library,
			suggested_summary: "Test".to_string(),
			suggested_details: None,
			suggested_bump: BumpSuggestion::Patch,
			changes: vec![],
			has_breaking: false,
			confidence: 0.9,
		};
		let cloned = group.clone();
		assert_eq!(cloned.package_id, "test");
	}

	#[test]
	fn test_suggested_changeset_clone() {
		let changeset = SuggestedChangeset {
			summary: "Test".to_string(),
			details: None,
			bump: BumpSuggestion::Minor,
			change_type: None,
			confidence: 0.95,
			api_changes: vec![],
			grouped_count: 1,
			files_changed: vec![],
			has_breaking_changes: false,
			before_after_suggested: false,
		};
		let cloned = changeset.clone();
		assert_eq!(cloned.summary, "Test");
	}

	#[test]
	fn test_package_change_analysis_clone() {
		let analysis = PackageChangeAnalysis {
			package_id: "test".to_string(),
			artifact_type: ArtifactType::Library,
			direct_change_count: 5,
			has_propagated_changes: false,
			suggested_changesets: vec![],
		};
		let cloned = analysis.clone();
		assert_eq!(cloned.direct_change_count, 5);
	}

	#[test]
	fn test_change_analysis_clone() {
		let analysis = ChangeAnalysis {
			frame: ChangeFrame::WorkingDirectory,
			package_changes: BTreeMap::new(),
			recommendations: vec![],
		};
		let cloned = analysis.clone();
		assert!(matches!(cloned.frame, ChangeFrame::WorkingDirectory));
	}

	#[test]
	fn test_semantic_change_clone() {
		let api = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: None,
		};
		let change = SemanticChange::Api(api);
		let cloned = change.clone();
		assert!(matches!(cloned, SemanticChange::Api(_)));
	}

	#[test]
	fn test_api_change_kind_clone() {
		let kind = ApiChangeKind::FunctionAdded;
		let cloned = kind;
		assert_eq!(kind, cloned);
	}

	#[test]
	fn test_app_change_kind_clone() {
		let kind = AppChangeKind::RouteAdded;
		let cloned = kind;
		assert_eq!(kind, cloned);
	}

	#[test]
	fn test_cli_change_kind_clone() {
		let kind = CliChangeKind::CommandAdded;
		let cloned = kind;
		assert_eq!(kind, cloned);
	}

	#[test]
	fn test_bump_suggestion_clone() {
		let bump = BumpSuggestion::Minor;
		let cloned = bump;
		assert_eq!(bump, cloned);
	}

	#[test]
	fn test_artifact_type_clone() {
		let artifact = ArtifactType::Library;
		let cloned = artifact;
		assert_eq!(artifact, cloned);
	}

	#[test]
	fn test_detection_level_clone() {
		let level = DetectionLevel::Signature;
		let cloned = level;
		assert_eq!(level, cloned);
	}

	#[test]
	fn test_visibility_clone() {
		let vis = Visibility::Public;
		let cloned = vis;
		assert_eq!(vis, cloned);
	}

	#[test]
	fn test_config_change_clone() {
		let config = ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		};
		let cloned = config.clone();
		assert_eq!(cloned.file_name, "config.toml");
	}

	#[test]
	fn test_app_change_clone() {
		let app = AppChange {
			kind: AppChangeKind::ComponentAdded,
			route: Some("/home".to_string()),
			component: Some("Home".to_string()),
			description: "Added home".to_string(),
			is_user_visible: true,
			file_path: PathBuf::from("src/pages/home.tsx"),
		};
		let cloned = app.clone();
		assert_eq!(cloned.description, "Added home");
	}

	#[test]
	fn test_cli_change_clone() {
		let cli = CliChange {
			kind: CliChangeKind::FlagAdded,
			command: Some("build".to_string()),
			flag: Some("verbose".to_string()),
			description: "Added verbose flag".to_string(),
			is_breaking: false,
			file_path: PathBuf::from("src/cli.rs"),
		};
		let cloned = cli.clone();
		assert_eq!(cloned.description, "Added verbose flag");
	}

	#[test]
	fn test_serialize_deserialize_semantic_change() {
		let api = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test_fn".to_string(),
			signature: Some("fn test()".to_string()),
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(10),
		};
		let change = SemanticChange::Api(api);
		let json =
			serde_json::to_string(&change).unwrap_or_else(|e| panic!("should serialize: {e}"));
		let deserialized: SemanticChange =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
		assert!(matches!(deserialized, SemanticChange::Api(_)));
	}

	#[test]
	fn test_serialize_deserialize_change_group() {
		let group = ChangeGroup {
			package_id: "test".to_string(),
			artifact_type: ArtifactType::Library,
			suggested_summary: "Test".to_string(),
			suggested_details: None,
			suggested_bump: BumpSuggestion::Patch,
			changes: vec![],
			has_breaking: false,
			confidence: 0.9,
		};
		let json =
			serde_json::to_string(&group).unwrap_or_else(|e| panic!("should serialize: {e}"));
		let deserialized: ChangeGroup =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
		assert_eq!(deserialized.package_id, "test");
	}

	#[test]
	fn test_suggested_changeset_with_change_type() {
		let changeset = SuggestedChangeset {
			summary: "Add feature".to_string(),
			details: None,
			bump: BumpSuggestion::Minor,
			change_type: Some("feature".to_string()),
			confidence: 0.95,
			api_changes: vec![ApiChange {
				kind: ApiChangeKind::FunctionAdded,
				visibility: Visibility::Public,
				name: "new_fn".to_string(),
				signature: Some("fn new_fn()".to_string()),
				doc_comment: Some("New function".to_string()),
				is_breaking: false,
				file_path: PathBuf::from("src/lib.rs"),
				line_number: Some(42),
			}],
			grouped_count: 1,
			files_changed: vec![PathBuf::from("src/lib.rs")],
			has_breaking_changes: false,
			before_after_suggested: true,
		};
		assert!(changeset.change_type.is_some());
		assert_eq!(changeset.api_changes.len(), 1);
	}

	#[test]
	fn test_group_changes_all_non_breaking_api() {
		let changes = vec![
			SemanticChange::Api(ApiChange {
				kind: ApiChangeKind::FunctionAdded,
				visibility: Visibility::Public,
				name: "fn1".to_string(),
				signature: None,
				doc_comment: None,
				is_breaking: false,
				file_path: PathBuf::from("src/lib.rs"),
				line_number: None,
			}),
			SemanticChange::Api(ApiChange {
				kind: ApiChangeKind::TypeAdded,
				visibility: Visibility::Public,
				name: "Type1".to_string(),
				signature: None,
				doc_comment: None,
				is_breaking: false,
				file_path: PathBuf::from("src/lib.rs"),
				line_number: None,
			}),
		];
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(changes, ArtifactType::Library, &thresholds).unwrap();
		assert_eq!(groups.len(), 2);
		assert!(
			!groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.has_breaking
		);
		assert!(
			!groups
				.get(1)
				.unwrap_or_else(|| panic!("expected at least 2 groups"))
				.has_breaking
		);
	}

	#[test]
	fn test_group_changes_only_breaking() {
		let changes = vec![SemanticChange::Api(ApiChange {
			kind: ApiChangeKind::FunctionRemoved,
			visibility: Visibility::Public,
			name: "old_fn1".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: true,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: None,
		})];
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(changes, ArtifactType::Library, &thresholds).unwrap();
		assert_eq!(groups.len(), 1);
		assert!(
			groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.has_breaking
		);
		assert!(matches!(
			groups
				.first()
				.unwrap_or_else(|| panic!("expected at least 1 group"))
				.suggested_bump,
			BumpSuggestion::Major
		));
	}

	#[test]
	fn test_describe_change_app_with_route() {
		let app = AppChange {
			kind: AppChangeKind::RouteAdded,
			route: Some("/home".to_string()),
			component: Some("HomePage".to_string()),
			description: "Added home page".to_string(),
			is_user_visible: true,
			file_path: PathBuf::from("src/pages/home.tsx"),
		};
		let desc = describe_change(&SemanticChange::App(app));
		assert!(desc.contains("RouteAdded"));
		assert!(desc.contains("Added home page"));
	}

	#[test]
	fn test_describe_change_cli_with_command() {
		let cli = CliChange {
			kind: CliChangeKind::CommandAdded,
			command: Some("build".to_string()),
			flag: None,
			description: "Added build command".to_string(),
			is_breaking: false,
			file_path: PathBuf::from("src/cli.rs"),
		};
		let desc = describe_change(&SemanticChange::Cli(cli));
		assert!(desc.contains("CommandAdded"));
		assert!(desc.contains("Added build command"));
	}

	#[test]
	fn test_serialize_deserialize_analysis_config() {
		let config = AnalysisConfig::default();
		let json =
			serde_json::to_string(&config).unwrap_or_else(|e| panic!("should serialize: {e}"));
		let deserialized: AnalysisConfig =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
		assert_eq!(config.max_suggestions, deserialized.max_suggestions);
	}

	#[test]
	fn test_serialize_deserialize_grouping_thresholds() {
		let thresholds = GroupingThresholds::default();
		let json =
			serde_json::to_string(&thresholds).unwrap_or_else(|e| panic!("should serialize: {e}"));
		let deserialized: GroupingThresholds =
			serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
		assert_eq!(thresholds.group_public_api, deserialized.group_public_api);
	}

	#[test]
	fn test_serialize_deserialize_bump_suggestion() {
		let bumps = vec![
			BumpSuggestion::None,
			BumpSuggestion::Patch,
			BumpSuggestion::Minor,
			BumpSuggestion::Major,
		];
		for bump in bumps {
			let json =
				serde_json::to_string(&bump).unwrap_or_else(|e| panic!("should serialize: {e}"));
			let deserialized: BumpSuggestion =
				serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
			assert_eq!(bump, deserialized);
		}
	}

	#[test]
	fn test_serialize_deserialize_detection_level() {
		let levels = vec![
			DetectionLevel::Basic,
			DetectionLevel::Signature,
			DetectionLevel::Semantic,
		];
		for level in levels {
			let json =
				serde_json::to_string(&level).unwrap_or_else(|e| panic!("should serialize: {e}"));
			let deserialized: DetectionLevel =
				serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
			assert_eq!(level, deserialized);
		}
	}

	#[test]
	fn test_serialize_deserialize_artifact_type() {
		let types = vec![
			ArtifactType::Library,
			ArtifactType::Application,
			ArtifactType::CliTool,
			ArtifactType::Mixed,
		];
		for artifact_type in types {
			let json = serde_json::to_string(&artifact_type)
				.unwrap_or_else(|e| panic!("should serialize: {e}"));
			let deserialized: ArtifactType =
				serde_json::from_str(&json).unwrap_or_else(|e| panic!("should deserialize: {e}"));
			assert_eq!(artifact_type, deserialized);
		}
	}

	#[test]
	fn test_api_change_kind_equality() {
		let k1 = ApiChangeKind::FunctionAdded;
		let k2 = ApiChangeKind::FunctionAdded;
		let k3 = ApiChangeKind::TypeAdded;
		assert_eq!(k1, k2);
		assert_ne!(k1, k3);
	}

	#[test]
	fn test_app_change_kind_equality() {
		let k1 = AppChangeKind::RouteAdded;
		let k2 = AppChangeKind::RouteAdded;
		let k3 = AppChangeKind::ComponentAdded;
		assert_eq!(k1, k2);
		assert_ne!(k1, k3);
	}

	#[test]
	fn test_cli_change_kind_equality() {
		let k1 = CliChangeKind::CommandAdded;
		let k2 = CliChangeKind::CommandAdded;
		let k3 = CliChangeKind::FlagAdded;
		assert_eq!(k1, k2);
		assert_ne!(k1, k3);
	}

	#[test]
	fn test_bump_suggestion_equality() {
		assert_eq!(BumpSuggestion::None, BumpSuggestion::None);
		assert_eq!(BumpSuggestion::Patch, BumpSuggestion::Patch);
		assert_eq!(BumpSuggestion::Minor, BumpSuggestion::Minor);
		assert_eq!(BumpSuggestion::Major, BumpSuggestion::Major);
		assert_ne!(BumpSuggestion::None, BumpSuggestion::Patch);
	}

	#[test]
	fn test_artifact_type_equality() {
		assert_eq!(ArtifactType::Library, ArtifactType::Library);
		assert_eq!(ArtifactType::Application, ArtifactType::Application);
		assert_eq!(ArtifactType::CliTool, ArtifactType::CliTool);
		assert_eq!(ArtifactType::Mixed, ArtifactType::Mixed);
		assert_ne!(ArtifactType::Library, ArtifactType::Application);
	}

	#[test]
	fn test_detection_level_equality() {
		assert_eq!(DetectionLevel::Basic, DetectionLevel::Basic);
		assert_eq!(DetectionLevel::Signature, DetectionLevel::Signature);
		assert_eq!(DetectionLevel::Semantic, DetectionLevel::Semantic);
		assert_ne!(DetectionLevel::Basic, DetectionLevel::Signature);
	}

	#[test]
	fn test_visibility_equality() {
		assert_eq!(Visibility::Public, Visibility::Public);
		assert_eq!(Visibility::Crate, Visibility::Crate);
		assert_eq!(Visibility::Super, Visibility::Super);
		assert_eq!(Visibility::Restricted, Visibility::Restricted);
		assert_eq!(Visibility::Private, Visibility::Private);
		assert_ne!(Visibility::Public, Visibility::Private);
	}

	#[test]
	fn test_config_change_equality() {
		let c1 = ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		};
		let c2 = ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		};
		let c3 = ConfigChange {
			file_name: "other.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		};
		assert_eq!(c1, c2);
		assert_ne!(c1, c3);
	}

	#[test]
	fn test_api_change_debug() {
		let api = ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "test".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(42),
		};
		let debug = format!("{api:?}");
		assert!(debug.contains("FunctionAdded"));
		assert!(debug.contains("test"));
	}

	#[test]
	fn test_app_change_debug() {
		let app = AppChange {
			kind: AppChangeKind::RouteAdded,
			route: Some("/home".to_string()),
			component: Some("Home".to_string()),
			description: "Added home".to_string(),
			is_user_visible: true,
			file_path: PathBuf::from("src/pages/home.tsx"),
		};
		let debug = format!("{app:?}");
		assert!(debug.contains("RouteAdded"));
		assert!(debug.contains("home"));
	}

	#[test]
	fn test_cli_change_debug() {
		let cli = CliChange {
			kind: CliChangeKind::CommandAdded,
			command: Some("build".to_string()),
			flag: None,
			description: "Added build command".to_string(),
			is_breaking: false,
			file_path: PathBuf::from("src/cli.rs"),
		};
		let debug = format!("{cli:?}");
		assert!(debug.contains("CommandAdded"));
		assert!(debug.contains("build"));
	}

	#[test]
	fn test_config_change_debug() {
		let config = ConfigChange {
			file_name: "config.toml".to_string(),
			description: "Changed port".to_string(),
			is_user_facing: true,
		};
		let debug = format!("{config:?}");
		assert!(debug.contains("config.toml"));
		assert!(debug.contains("Changed port"));
	}

	#[test]
	fn test_semantic_change_unknown_equality() {
		let s1 = SemanticChange::Unknown {
			path: PathBuf::from("test.txt"),
			description: "Test".to_string(),
		};
		let s2 = SemanticChange::Unknown {
			path: PathBuf::from("test.txt"),
			description: "Test".to_string(),
		};
		let s3 = SemanticChange::Unknown {
			path: PathBuf::from("other.txt"),
			description: "Test".to_string(),
		};
		assert_eq!(s1, s2);
		assert_ne!(s1, s3);
	}

	#[test]
	fn test_suggested_changeset_equality() {
		let s1 = SuggestedChangeset {
			summary: "Test".to_string(),
			details: None,
			bump: BumpSuggestion::Patch,
			change_type: None,
			confidence: 0.9,
			api_changes: vec![],
			grouped_count: 1,
			files_changed: vec![],
			has_breaking_changes: false,
			before_after_suggested: false,
		};
		let s2 = s1.clone();
		assert_eq!(s1.summary, s2.summary);
		assert_eq!(s1.bump, s2.bump);
	}

	#[test]
	fn test_change_group_equality() {
		let g1 = ChangeGroup {
			package_id: "pkg1".to_string(),
			artifact_type: ArtifactType::Library,
			suggested_summary: "Test".to_string(),
			suggested_details: None,
			suggested_bump: BumpSuggestion::Patch,
			changes: vec![],
			has_breaking: false,
			confidence: 0.9,
		};
		let g2 = g1.clone();
		assert_eq!(g1.package_id, g2.package_id);
		assert_eq!(g1.artifact_type, g2.artifact_type);
	}

	#[test]
	fn test_package_change_analysis_equality() {
		let p1 = PackageChangeAnalysis {
			package_id: "pkg1".to_string(),
			artifact_type: ArtifactType::Library,
			direct_change_count: 5,
			has_propagated_changes: false,
			suggested_changesets: vec![],
		};
		let p2 = p1.clone();
		assert_eq!(p1.package_id, p2.package_id);
		assert_eq!(p1.direct_change_count, p2.direct_change_count);
	}

	#[test]
	fn test_change_analysis_equality() {
		let c1 = ChangeAnalysis {
			frame: ChangeFrame::WorkingDirectory,
			package_changes: BTreeMap::new(),
			recommendations: vec!["Rec1".to_string()],
		};
		let c2 = c1.clone();
		assert!(matches!(c2.frame, ChangeFrame::WorkingDirectory));
		assert_eq!(c2.recommendations.len(), 1);
	}

	#[test]
	fn test_group_changes_with_many_non_breaking() {
		let changes: Vec<SemanticChange> = (0..10)
			.map(|i| {
				SemanticChange::Api(ApiChange {
					kind: ApiChangeKind::FunctionAdded,
					visibility: Visibility::Public,
					name: format!("fn{i}"),
					signature: None,
					doc_comment: None,
					is_breaking: false,
					file_path: PathBuf::from("src/lib.rs"),
					line_number: Some(i),
				})
			})
			.collect();
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(changes, ArtifactType::Library, &thresholds).unwrap();
		assert_eq!(groups.len(), 10); // Each change gets its own group
	}

	#[test]
	fn test_group_changes_with_separate_breaking_and_non_breaking() {
		let breaking = SemanticChange::Api(ApiChange {
			kind: ApiChangeKind::FunctionRemoved,
			visibility: Visibility::Public,
			name: "old_fn".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: true,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(10),
		});
		let non_breaking = SemanticChange::Api(ApiChange {
			kind: ApiChangeKind::FunctionAdded,
			visibility: Visibility::Public,
			name: "new_fn".to_string(),
			signature: None,
			doc_comment: None,
			is_breaking: false,
			file_path: PathBuf::from("src/lib.rs"),
			line_number: Some(20),
		});
		let changes = vec![breaking, non_breaking];
		let thresholds = GroupingThresholds::default();
		let groups = group_changes(changes, ArtifactType::Library, &thresholds).unwrap();
		assert_eq!(groups.len(), 2);
		let breaking_groups: Vec<_> = groups.iter().filter(|g| g.has_breaking).collect();
		let non_breaking_groups: Vec<_> = groups.iter().filter(|g| !g.has_breaking).collect();
		assert_eq!(breaking_groups.len(), 1);
		assert_eq!(non_breaking_groups.len(), 1);
	}
}
