#![forbid(clippy::indexing_slicing)]

//! # `monochange_analysis`
//!
//! Semantic change analysis for generating granular changesets.
//!
//! This crate provides the analysis pipeline for detecting user-facing changes
//! across libraries, applications, and CLI tools. It extracts semantic meaning
//! from git diffs to suggest appropriate changeset granularity.

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
	}
}
