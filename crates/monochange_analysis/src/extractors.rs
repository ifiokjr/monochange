//! Semantic change extractors for different artifact types.
//!
//! This module provides the logic to extract meaningful changes from git diffs
//! based on the artifact type (library, application, CLI tool).

use std::path::Path;
use std::path::PathBuf;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use crate::ApiChange;
use crate::ApiChangeKind;
use crate::AppChange;
use crate::AppChangeKind;
use crate::ArtifactType;
use crate::CliChange;
use crate::CliChangeKind;
use crate::DetectionLevel;
use crate::SemanticChange;
use crate::Visibility;

/// Result of extracting changes from a set of files.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
	/// The extracted semantic changes
	pub changes: Vec<SemanticChange>,
	/// Files that were analyzed
	pub files_analyzed: Vec<PathBuf>,
	/// Files that could not be analyzed
	pub files_skipped: Vec<(PathBuf, SkipReason)>,
}

/// Reasons why a file was skipped during extraction.
#[derive(Debug, Clone)]
pub enum SkipReason {
	/// File type not supported for this artifact type
	UnsupportedExtension,
	/// Binary file
	BinaryFile,
	/// Too large to analyze
	TooLarge,
	/// Parse error
	ParseError(String),
	/// Not relevant to this artifact
	NotRelevant,
}

/// Extract semantic changes from a set of changed files.
///
/// # Errors
///
/// Returns an error if extraction fails.
pub fn extract_changes(
	files: &[PathBuf],
	artifact_type: ArtifactType,
	detection_level: DetectionLevel,
	repo_root: &Path,
) -> MonochangeResult<ExtractionResult> {
	match artifact_type {
		ArtifactType::Library => extract_library_changes(files, detection_level, repo_root),
		ArtifactType::Application => extract_application_changes(files, detection_level, repo_root),
		ArtifactType::CliTool => extract_cli_changes(files, detection_level, repo_root),
		ArtifactType::Mixed => extract_mixed_changes(files, detection_level, repo_root),
	}
}

/// Extract changes for library artifacts.
fn extract_library_changes(
	files: &[PathBuf],
	detection_level: DetectionLevel,
	repo_root: &Path,
) -> MonochangeResult<ExtractionResult> {
	let mut changes = Vec::new();
	let mut files_analyzed = Vec::new();
	let mut files_skipped = Vec::new();

	for file in files {
		// Skip non-Rust files for libraries
		if file.extension().and_then(|e| e.to_str()) != Some("rs") {
			files_skipped.push((file.clone(), SkipReason::UnsupportedExtension));
			continue;
		}

		// Skip test files
		if is_test_file(file) {
			files_skipped.push((file.clone(), SkipReason::NotRelevant));
			continue;
		}

		let file_changes = match detection_level {
			DetectionLevel::Basic => extract_library_basic(file, repo_root),
			DetectionLevel::Signature => extract_library_signatures(file, repo_root)?,
			DetectionLevel::Semantic => extract_library_semantic(file, repo_root),
		};

		changes.extend(file_changes);
		files_analyzed.push(file.clone());
	}

	Ok(ExtractionResult {
		changes,
		files_analyzed,
		files_skipped,
	})
}

/// Basic level: Just detect file-level changes for libraries.
fn extract_library_basic(file: &Path, _repo_root: &Path) -> Vec<SemanticChange> {
	// At basic level, we just note that a file changed
	let description = format!("changes in {}", file.display());

	vec![SemanticChange::Unknown {
		path: file.to_path_buf(),
		description,
	}]
}

/// Signature level: Extract function/type signatures for libraries.
fn extract_library_signatures(
	file: &Path,
	repo_root: &Path,
) -> MonochangeResult<Vec<SemanticChange>> {
	let mut changes = Vec::new();

	// Get the diff for this file
	let diff_output = get_file_diff(repo_root, file)?;

	// Parse the diff for signature changes
	let parsed_changes = parse_rust_signatures(&diff_output, file);
	changes.extend(parsed_changes);

	Ok(changes)
}

/// Parse Rust signatures from diff output.
fn parse_rust_signatures(diff: &str, file_path: &Path) -> Vec<SemanticChange> {
	let mut changes = Vec::new();
	let mut line_number: Option<usize> = None;

	for line in diff.lines() {
		// Track line numbers from diff hunk headers
		if line.starts_with("@@") {
			line_number = parse_hunk_header(line);
			continue;
		}

		// Look for added public items
		if (line.starts_with("+pub ") || line.starts_with("+pub("))
			&& let Some(change) = parse_added_item(line, file_path, line_number)
		{
			changes.push(change);
		}

		// Look for removed public items (breaking changes)
		if (line.starts_with("-pub ") || line.starts_with("-pub("))
			&& let Some(change) = parse_removed_item(line, file_path, line_number)
		{
			changes.push(change);
		}

		// Increment line number for added lines
		if !line.starts_with('-')
			&& !line.starts_with("@@")
			&& let Some(ref mut num) = line_number
		{
			*num += 1;
		}
	}

	changes
}

/// Parse a line with an added public item.
fn parse_added_item(
	line: &str,
	file_path: &Path,
	line_number: Option<usize>,
) -> Option<SemanticChange> {
	let content = line.trim_start_matches('+').trim();

	// Parse visibility
	let (visibility, rest) = parse_visibility(content)?;

	// Skip if not actually public
	if !matches!(visibility, Visibility::Public) {
		return None;
	}

	// Parse item kind
	let tokens: Vec<&str> = rest.split_whitespace().collect();
	let kind = tokens.first()?;
	let name = tokens.get(1)?.trim_end_matches('{').trim_end_matches(';');

	let api_kind = match *kind {
		"fn" => ApiChangeKind::FunctionAdded,
		"struct" | "enum" | "type" => ApiChangeKind::TypeAdded,
		"trait" => ApiChangeKind::TraitAdded,
		"const" | "static" => ApiChangeKind::ConstantAdded,
		_ => return None,
	};

	// Skip if name is empty
	if name.is_empty() {
		return None;
	}

	let signature = if *kind == "fn" {
		// Extract function signature
		let sig_start = rest.find("fn")?;
		Some(rest[sig_start..].to_string())
	} else {
		None
	};

	Some(SemanticChange::Api(ApiChange {
		kind: api_kind,
		visibility,
		name: name.to_string(),
		signature,
		doc_comment: None, // Could extract from context
		is_breaking: false,
		file_path: file_path.to_path_buf(),
		line_number,
	}))
}

/// Parse a line with a removed public item.
fn parse_removed_item(
	line: &str,
	file_path: &Path,
	line_number: Option<usize>,
) -> Option<SemanticChange> {
	let content = line.trim_start_matches('-').trim();

	// Parse visibility
	let (visibility, rest) = parse_visibility(content)?;

	// Only track public removals as breaking
	if !matches!(visibility, Visibility::Public) {
		return None;
	}

	// Parse item kind
	let tokens: Vec<&str> = rest.split_whitespace().collect();
	let kind = tokens.first()?;
	let name = tokens.get(1)?.trim_end_matches('{').trim_end_matches(';');

	let api_kind = match *kind {
		"fn" => ApiChangeKind::FunctionRemoved,
		"struct" | "enum" | "type" => ApiChangeKind::TypeRemoved,
		"trait" => ApiChangeKind::TraitRemoved,
		"const" | "static" => ApiChangeKind::ConstantRemoved,
		_ => return None,
	};

	Some(SemanticChange::Api(ApiChange {
		kind: api_kind,
		visibility,
		name: name.to_string(),
		signature: None,
		doc_comment: None,
		is_breaking: true, // Removing public items is breaking
		file_path: file_path.to_path_buf(),
		line_number,
	}))
}

/// Parse visibility modifier.
fn parse_visibility(content: &str) -> Option<(Visibility, &str)> {
	if let Some(stripped) = content.strip_prefix("pub(crate)") {
		Some((Visibility::Crate, stripped.trim_start()))
	} else if let Some(stripped) = content.strip_prefix("pub(super)") {
		Some((Visibility::Super, stripped.trim_start()))
	} else if content.starts_with("pub(in ") {
		// Find closing paren
		let end = content.find(')')?;
		Some((Visibility::Restricted, content.get(end + 1..)?.trim_start()))
	} else if let Some(stripped) = content.strip_prefix("pub ") {
		Some((Visibility::Public, stripped.trim_start()))
	} else {
		Some((Visibility::Private, content))
	}
}

/// Parse hunk header to get starting line number.
fn parse_hunk_header(line: &str) -> Option<usize> {
	// Format: @@ -old_start,old_count +new_start,new_count @@
	// or: @@ -old_start +new_start @@
	let parts: Vec<&str> = line.split_whitespace().collect();

	// Find the part starting with '+' (the new file position)
	let new_part = parts.iter().find(|p| p.starts_with('+'))?;

	// Extract the number after '+' and before optional comma
	let num_str = new_part.trim_start_matches('+').split(',').next()?;

	num_str.parse().ok()
}

/// Semantic level: Full AST parsing (placeholder for future implementation).
fn extract_library_semantic(file: &Path, _repo_root: &Path) -> Vec<SemanticChange> {
	// Full AST parsing would require syn crate
	// For now, fall back to signature level
	vec![SemanticChange::Unknown {
		path: file.to_path_buf(),
		description: format!(
			"semantic analysis for {} (not yet implemented)",
			file.display()
		),
	}]
}

/// Extract changes for application artifacts.
fn extract_application_changes(
	files: &[PathBuf],
	detection_level: DetectionLevel,
	repo_root: &Path,
) -> MonochangeResult<ExtractionResult> {
	let mut changes = Vec::new();
	let mut files_analyzed = Vec::new();
	let mut files_skipped = Vec::new();

	for file in files {
		// Detect file category from path
		let category = categorize_app_file(file);

		if category == AppFileCategory::Ignored {
			files_skipped.push((file.clone(), SkipReason::NotRelevant));
			continue;
		}

		let file_changes = match detection_level {
			DetectionLevel::Basic => extract_app_basic(file, category),
			DetectionLevel::Signature => extract_app_signatures(file, category, repo_root)?,
			DetectionLevel::Semantic => extract_app_semantic(file, category, repo_root),
		};

		changes.extend(file_changes);
		files_analyzed.push(file.clone());
	}

	Ok(ExtractionResult {
		changes,
		files_analyzed,
		files_skipped,
	})
}

/// Categories of application files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppFileCategory {
	Route,
	Component,
	State,
	Api,
	Style,
	Config,
	Ignored,
}

/// Categorize an application file by its path.
fn categorize_app_file(file: &Path) -> AppFileCategory {
	let path_str = file.to_string_lossy();

	// Check common patterns
	if path_str.contains("/routes/") || path_str.contains("/pages/") {
		return AppFileCategory::Route;
	}
	if path_str.contains("/components/") {
		return AppFileCategory::Component;
	}
	if path_str.contains("/state/") || path_str.contains("/store/") || path_str.contains("/redux/")
	{
		return AppFileCategory::State;
	}
	if path_str.contains("/api/") || path_str.contains("/endpoints/") {
		return AppFileCategory::Api;
	}
	if path_str.contains("/styles/") || path_str.ends_with(".css") || path_str.ends_with(".scss") {
		return AppFileCategory::Style;
	}
	if path_str.contains("config") && path_str.ends_with(".ts") || path_str.ends_with(".js") {
		return AppFileCategory::Config;
	}

	// Check file content patterns
	AppFileCategory::Ignored
}

/// Basic extraction for applications.
fn extract_app_basic(file: &Path, category: AppFileCategory) -> Vec<SemanticChange> {
	let (kind, description) = match category {
		AppFileCategory::Route => (AppChangeKind::RouteModified, "route modified"),
		AppFileCategory::Component => (AppChangeKind::ComponentModified, "component modified"),
		AppFileCategory::State => {
			(
				AppChangeKind::StateManagementChanged,
				"state management changed",
			)
		}
		AppFileCategory::Api => (AppChangeKind::ApiEndpointModified, "API endpoint modified"),
		AppFileCategory::Style => (AppChangeKind::StyleChanged, "styles modified"),
		AppFileCategory::Config => {
			(
				AppChangeKind::StateManagementChanged,
				"configuration modified",
			)
		}
		AppFileCategory::Ignored => return Vec::new(),
	};

	vec![SemanticChange::App(AppChange {
		kind,
		route: extract_route_from_path(file),
		component: extract_component_name(file),
		description: description.to_string(),
		is_user_visible: category == AppFileCategory::Route
			|| category == AppFileCategory::Component
			|| category == AppFileCategory::Style,
		file_path: file.to_path_buf(),
	})]
}

/// Extract signatures for applications.
fn extract_app_signatures(
	file: &Path,
	category: AppFileCategory,
	repo_root: &Path,
) -> MonochangeResult<Vec<SemanticChange>> {
	// Get diff and look for patterns
	let diff_output = get_file_diff(repo_root, file)?;
	let mut changes = Vec::new();

	// Look for component/route additions
	if diff_output.contains("export default") || diff_output.contains("export function") {
		changes.push(SemanticChange::App(AppChange {
			kind: match category {
				AppFileCategory::Route => AppChangeKind::RouteAdded,
				_ => AppChangeKind::ComponentAdded,
			},
			route: extract_route_from_path(file),
			component: extract_component_name(file),
			description: format!("new {} added", category_name(category)),
			is_user_visible: true,
			file_path: file.to_path_buf(),
		}));
	}

	// Look for prop/parameter changes
	if diff_output.contains("interface ") || diff_output.contains("type ") {
		changes.push(SemanticChange::App(AppChange {
			kind: AppChangeKind::ComponentModified,
			route: None,
			component: extract_component_name(file),
			description: "type definitions modified".to_string(),
			is_user_visible: category == AppFileCategory::Route,
			file_path: file.to_path_buf(),
		}));
	}

	if changes.is_empty() {
		changes = extract_app_basic(file, category);
	}

	Ok(changes)
}

/// Semantic extraction for applications.
fn extract_app_semantic(
	file: &Path,
	_category: AppFileCategory,
	_repo_root: &Path,
) -> Vec<SemanticChange> {
	// Full parsing would require TypeScript/JavaScript AST
	// For now, use signature level
	vec![SemanticChange::Unknown {
		path: file.to_path_buf(),
		description: format!(
			"semantic analysis for {} (not yet implemented)",
			file.display()
		),
	}]
}

/// Helper to extract route from file path.
fn extract_route_from_path(file: &Path) -> Option<String> {
	let path_str = file.to_string_lossy();

	// Extract route from /routes/... or /pages/...
	if let Some(routes_idx) = path_str.find("/routes/") {
		let route_part = &path_str[routes_idx + 8..];
		let route = route_part
			.trim_end_matches(".tsx")
			.trim_end_matches(".jsx")
			.trim_end_matches(".ts")
			.trim_end_matches(".js");
		return Some(format!("/{}", route.replace("/index", "")));
	}

	if let Some(pages_idx) = path_str.find("/pages/") {
		let route_part = &path_str[pages_idx + 7..];
		let route = route_part
			.trim_end_matches(".tsx")
			.trim_end_matches(".jsx")
			.trim_end_matches(".ts")
			.trim_end_matches(".js");
		return Some(format!("/{}", route.replace("/index", "")));
	}

	None
}

/// Helper to extract component name from file path.
fn extract_component_name(file: &Path) -> Option<String> {
	file.file_stem()
		.and_then(|s| s.to_str())
		.map(ToString::to_string)
}

/// Get category name.
fn category_name(category: AppFileCategory) -> &'static str {
	match category {
		AppFileCategory::Route => "route",
		AppFileCategory::Component => "component",
		AppFileCategory::State => "state management",
		AppFileCategory::Api => "API endpoint",
		AppFileCategory::Style => "style",
		AppFileCategory::Config => "configuration",
		AppFileCategory::Ignored => "file",
	}
}

/// Extract changes for CLI artifacts.
fn extract_cli_changes(
	files: &[PathBuf],
	detection_level: DetectionLevel,
	repo_root: &Path,
) -> MonochangeResult<ExtractionResult> {
	let mut changes = Vec::new();
	let mut files_analyzed = Vec::new();
	let mut files_skipped = Vec::new();

	for file in files {
		// For CLI tools, focus on Rust source files
		if file.extension().and_then(|e| e.to_str()) != Some("rs") {
			files_skipped.push((file.clone(), SkipReason::UnsupportedExtension));
			continue;
		}

		let file_changes = match detection_level {
			DetectionLevel::Basic => extract_cli_basic(file),
			DetectionLevel::Signature => extract_cli_signatures(file, repo_root)?,
			DetectionLevel::Semantic => extract_cli_semantic(file, repo_root),
		};

		changes.extend(file_changes);
		files_analyzed.push(file.clone());
	}

	Ok(ExtractionResult {
		changes,
		files_analyzed,
		files_skipped,
	})
}

/// Basic extraction for CLI tools.
fn extract_cli_basic(file: &Path) -> Vec<SemanticChange> {
	vec![SemanticChange::Cli(CliChange {
		kind: CliChangeKind::CommandModified,
		command: None,
		flag: None,
		description: format!("changes in {}", file.display()),
		is_breaking: false,
		file_path: file.to_path_buf(),
	})]
}

/// Extract signatures for CLI tools.
fn extract_cli_signatures(file: &Path, repo_root: &Path) -> MonochangeResult<Vec<SemanticChange>> {
	let mut changes = Vec::new();
	let diff_output = get_file_diff(repo_root, file)?;

	// Look for clap derive patterns
	if diff_output.contains("#[derive(Parser)]") || diff_output.contains("#[command(") {
		// New command structure
		changes.push(SemanticChange::Cli(CliChange {
			kind: CliChangeKind::CommandAdded,
			command: extract_command_name(&diff_output),
			flag: None,
			description: "new CLI command added".to_string(),
			is_breaking: false,
			file_path: file.to_path_buf(),
		}));
	}

	// Look for flag changes
	if diff_output.contains("#[arg(") || diff_output.contains(".arg(") {
		changes.push(SemanticChange::Cli(CliChange {
			kind: CliChangeKind::FlagModified,
			command: None,
			flag: extract_flag_name(&diff_output),
			description: "CLI flag modified".to_string(),
			is_breaking: false,
			file_path: file.to_path_buf(),
		}));
	}

	// Look for output format changes
	if diff_output.contains("println!") || diff_output.contains("eprintln!") {
		changes.push(SemanticChange::Cli(CliChange {
			kind: CliChangeKind::OutputFormatChanged,
			command: None,
			flag: None,
			description: "CLI output modified".to_string(),
			is_breaking: false,
			file_path: file.to_path_buf(),
		}));
	}

	if changes.is_empty() {
		changes = extract_cli_basic(file);
	}

	Ok(changes)
}

/// Extract command name from diff.
fn extract_command_name(diff: &str) -> Option<String> {
	// Look for name = "..." or name = '...'
	if let Some(idx) = diff.find("name = ") {
		let start = idx + 7;
		let rest = &diff[start..];
		let quote_char = rest.chars().next()?;
		let end = rest[1..].find(quote_char).map(|i| i + 1)?;
		return Some(rest[1..end].to_string());
	}
	None
}

/// Extract flag name from diff.
fn extract_flag_name(diff: &str) -> Option<String> {
	// Look for long = "..." or short = '...'
	if let Some(idx) = diff.find("long = ") {
		let start = idx + 7;
		let rest = &diff[start..];
		let quote_char = rest.chars().next()?;
		let end = rest[1..].find(quote_char).map(|i| i + 1)?;
		return Some(rest[1..end].to_string());
	}
	None
}

/// Semantic extraction for CLI tools.
fn extract_cli_semantic(file: &Path, _repo_root: &Path) -> Vec<SemanticChange> {
	vec![SemanticChange::Unknown {
		path: file.to_path_buf(),
		description: format!(
			"semantic analysis for {} (not yet implemented)",
			file.display()
		),
	}]
}

/// Extract changes for mixed artifacts.
fn extract_mixed_changes(
	files: &[PathBuf],
	detection_level: DetectionLevel,
	repo_root: &Path,
) -> MonochangeResult<ExtractionResult> {
	// For mixed artifacts, analyze based on file location
	let lib_files: Vec<_> = files
		.iter()
		.filter(|f| {
			let p = f.to_string_lossy();
			p.contains("/lib.rs") || p.contains("/src/lib/")
		})
		.cloned()
		.collect();

	let bin_files: Vec<_> = files
		.iter()
		.filter(|f| {
			let p = f.to_string_lossy();
			p.contains("/main.rs") || p.contains("/bin/") || p.contains("/src/bin/")
		})
		.cloned()
		.collect();

	let mut all_changes = Vec::new();
	let mut all_analyzed = Vec::new();
	let mut all_skipped = Vec::new();

	// Analyze library files
	if !lib_files.is_empty() {
		let lib_result = extract_library_changes(&lib_files, detection_level, repo_root)?;
		all_changes.extend(lib_result.changes);
		all_analyzed.extend(lib_result.files_analyzed);
		all_skipped.extend(lib_result.files_skipped);
	}

	// Analyze binary files
	if !bin_files.is_empty() {
		let bin_result = extract_cli_changes(&bin_files, detection_level, repo_root)?;
		all_changes.extend(bin_result.changes);
		all_analyzed.extend(bin_result.files_analyzed);
		all_skipped.extend(bin_result.files_skipped);
	}

	// Handle remaining files
	let other_files: Vec<_> = files
		.iter()
		.filter(|f| {
			let p = f.to_string_lossy();
			!p.contains("/lib.rs")
				&& !p.contains("/src/lib/")
				&& !p.contains("/main.rs")
				&& !p.contains("/bin/")
				&& !p.contains("/src/bin/")
		})
		.cloned()
		.collect();

	if !other_files.is_empty() {
		let cli_result = extract_cli_changes(&other_files, detection_level, repo_root)?;
		all_changes.extend(cli_result.changes);
		all_analyzed.extend(cli_result.files_analyzed);
		all_skipped.extend(cli_result.files_skipped);
	}

	Ok(ExtractionResult {
		changes: all_changes,
		files_analyzed: all_analyzed,
		files_skipped: all_skipped,
	})
}

/// Get git diff for a specific file.
fn get_file_diff(repo_root: &Path, file: &Path) -> MonochangeResult<String> {
	use std::process::Command;

	let output = Command::new("git")
		.current_dir(repo_root)
		.args(["diff", "HEAD", "--", file.to_string_lossy().as_ref()])
		.output()
		.map_err(|e| MonochangeError::Io(format!("failed to run git diff: {e}")))?;

	if !output.status.success() {
		// File might be new (not in HEAD)
		let output = Command::new("git")
			.current_dir(repo_root)
			.args(["diff", "--cached", "--", file.to_string_lossy().as_ref()])
			.output()
			.map_err(|e| MonochangeError::Io(format!("failed to run git diff: {e}")))?;

		if !output.status.success() {
			return Ok(String::new());
		}

		return String::from_utf8(output.stdout)
			.map_err(|e| MonochangeError::Io(format!("invalid utf-8: {e}")));
	}

	String::from_utf8(output.stdout).map_err(|e| MonochangeError::Io(format!("invalid utf-8: {e}")))
}

/// Check if a file is a test file.
fn is_test_file(file: &Path) -> bool {
	let path_str = file.to_string_lossy();

	path_str.contains("/tests/")
		|| path_str.contains("/__tests__/")
		|| path_str.contains(".test.")
		|| path_str.contains("_test.rs")
		|| path_str.ends_with("_tests.rs")
		|| path_str.starts_with("tests/")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn visibility_parsing() {
		assert_eq!(
			parse_visibility("pub fn foo()"),
			Some((Visibility::Public, "fn foo()"))
		);
		assert_eq!(
			parse_visibility("pub(crate) fn bar()"),
			Some((Visibility::Crate, "fn bar()"))
		);
		assert_eq!(
			parse_visibility("pub(super) struct Baz"),
			Some((Visibility::Super, "struct Baz"))
		);
		assert_eq!(
			parse_visibility("fn private()"),
			Some((Visibility::Private, "fn private()"))
		);
	}

	#[test]
	fn test_file_detection() {
		assert!(is_test_file(Path::new("src/tests/foo.rs")));
		assert!(is_test_file(Path::new("src/lib_test.rs")));
		assert!(is_test_file(Path::new("tests/integration.rs")));
		assert!(!is_test_file(Path::new("src/lib.rs")));
	}

	#[test]
	fn hunk_header_parsing() {
		assert_eq!(parse_hunk_header("@@ -10,5 +20,7 @@"), Some(20));
		assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some(1));
		assert_eq!(parse_hunk_header("invalid"), None);
	}

	#[test]
	fn app_file_categorization() {
		assert_eq!(
			categorize_app_file(Path::new("src/routes/dashboard.tsx")),
			AppFileCategory::Route
		);
		assert_eq!(
			categorize_app_file(Path::new("src/routes/dashboard/index.tsx")),
			AppFileCategory::Route
		);
		assert_eq!(
			categorize_app_file(Path::new("src/pages/home.tsx")),
			AppFileCategory::Route
		);
		assert_eq!(
			categorize_app_file(Path::new("src/components/Button.tsx")),
			AppFileCategory::Component
		);
		assert_eq!(
			categorize_app_file(Path::new("src/store/auth.ts")),
			AppFileCategory::State
		);
		assert_eq!(
			categorize_app_file(Path::new("src/redux/store.ts")),
			AppFileCategory::State
		);
		assert_eq!(
			categorize_app_file(Path::new("src/api/users.ts")),
			AppFileCategory::Api
		);
		assert_eq!(
			categorize_app_file(Path::new("src/endpoints/api.ts")),
			AppFileCategory::Api
		);
		assert_eq!(
			categorize_app_file(Path::new("src/styles/main.css")),
			AppFileCategory::Style
		);
		assert_eq!(
			categorize_app_file(Path::new("src/styles/theme.scss")),
			AppFileCategory::Style
		);
		assert_eq!(
			categorize_app_file(Path::new("config.ts")),
			AppFileCategory::Config
		);
		assert_eq!(
			categorize_app_file(Path::new("src/utils/helpers.ts")),
			AppFileCategory::Ignored
		);
	}

	#[test]
	fn test_parse_visibility_restricted() {
		assert_eq!(
			parse_visibility("pub(in crate::foo) fn bar()"),
			Some((Visibility::Restricted, "fn bar()"))
		);
	}

	#[test]
	fn test_parse_added_item_private_visibility() {
		// Private items should be skipped
		assert!(parse_added_item("+fn private()", Path::new("src/lib.rs"), Some(1)).is_none());
		assert!(parse_added_item("+struct Private {}", Path::new("src/lib.rs"), Some(1)).is_none());
	}

	#[test]
	fn test_parse_added_item_crate_visibility() {
		// Crate visibility items should be skipped (only Public is tracked for added items)
		assert!(
			parse_added_item(
				"+pub(crate) fn internal()",
				Path::new("src/lib.rs"),
				Some(1)
			)
			.is_none()
		);
	}

	#[test]
	fn test_parse_added_item_unknown_kind() {
		// Unknown item kinds should be skipped
		assert!(parse_added_item("+pub unknown_item", Path::new("src/lib.rs"), Some(1)).is_none());
	}

	#[test]
	fn test_parse_removed_item_crate_visibility() {
		// Crate visibility should be skipped (only Public removals are breaking)
		assert!(
			parse_removed_item(
				"-pub(crate) fn internal()",
				Path::new("src/lib.rs"),
				Some(1)
			)
			.is_none()
		);
	}

	#[test]
	fn test_parse_removed_item_private_visibility() {
		// Private items should be skipped
		assert!(parse_removed_item("-fn private()", Path::new("src/lib.rs"), Some(1)).is_none());
	}

	#[test]
	fn test_parse_rust_signatures_empty_diff() {
		let changes = parse_rust_signatures("", Path::new("src/lib.rs"));
		assert!(changes.is_empty());
	}

	#[test]
	fn test_parse_rust_signatures_no_public_items() {
		let diff = r#"@@ -10,5 +10,7 @@
 fn private() {}
- fn removed() {}
+ fn added() {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert!(changes.is_empty());
	}

	#[test]
	fn test_parse_rust_signatures_with_public_function_added() {
		let diff = r#"@@ -10,5 +10,7 @@
+pub fn new_function {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::FunctionAdded);
			assert_eq!(api.name, "new_function");
			assert!(!api.is_breaking);
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_public_function_removed() {
		let diff = r#"@@ -10,5 +10,7 @@
-pub fn old_function {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::FunctionRemoved);
			assert_eq!(api.name, "old_function");
			assert!(api.is_breaking);
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_struct_added() {
		let diff = r#"@@ -10,5 +10,7 @@
+pub struct NewStruct;
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::TypeAdded);
			assert_eq!(api.name, "NewStruct");
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_enum_removed() {
		let diff = r#"@@ -10,5 +10,7 @@
-pub enum OldEnum;
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::TypeRemoved);
			assert_eq!(api.name, "OldEnum");
			assert!(api.is_breaking);
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_trait_added() {
		let diff = r#"@@ -10,5 +10,7 @@
+pub trait NewTrait {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::TraitAdded);
			assert_eq!(api.name, "NewTrait");
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_const_added() {
		let diff = r#"@@ -10,5 +10,7 @@
+pub const NEW_CONST {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::ConstantAdded);
			assert_eq!(api.name, "NEW_CONST");
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_parse_rust_signatures_with_static_removed() {
		let diff = r#"@@ -10,5 +10,7 @@
-pub static OLD_STATIC {}
"#;
		let changes = parse_rust_signatures(diff, Path::new("src/lib.rs"));
		assert_eq!(changes.len(), 1);
		if let SemanticChange::Api(api) = &changes[0] {
			assert_eq!(api.kind, ApiChangeKind::ConstantRemoved);
			assert_eq!(api.name, "OLD_STATIC");
			assert!(api.is_breaking);
		} else {
			panic!("Expected API change");
		}
	}

	#[test]
	fn test_extract_route_from_path_pages() {
		assert_eq!(
			extract_route_from_path(Path::new("src/pages/index.tsx")),
			Some("/index".to_string()) // "index" becomes "/index", not just "/"
		);
		assert_eq!(
			extract_route_from_path(Path::new("src/pages/about.tsx")),
			Some("/about".to_string())
		);
		assert_eq!(
			extract_route_from_path(Path::new("src/pages/blog/index.tsx")),
			Some("/blog".to_string()) // "/blog/index" -> "/blog"
		);
	}

	#[test]
	fn test_extract_route_from_path_jsx() {
		assert_eq!(
			extract_route_from_path(Path::new("src/routes/home.jsx")),
			Some("/home".to_string())
		);
	}

	#[test]
	fn test_extract_route_from_path_js() {
		assert_eq!(
			extract_route_from_path(Path::new("src/routes/api.js")),
			Some("/api".to_string())
		);
	}

	#[test]
	fn test_extract_route_from_path_no_match() {
		assert_eq!(
			extract_route_from_path(Path::new("src/utils/helpers.ts")),
			None
		);
	}

	#[test]
	fn test_extract_component_name() {
		assert_eq!(
			extract_component_name(Path::new("src/components/Button.tsx")),
			Some("Button".to_string())
		);
		assert_eq!(
			extract_component_name(Path::new("src/utils/helpers.ts")),
			Some("helpers".to_string())
		);
	}

	#[test]
	fn test_extract_command_name_found() {
		let diff = r#"name = "my-command""#;
		assert_eq!(extract_command_name(diff), Some("my-command".to_string()));
	}

	#[test]
	fn test_extract_command_name_not_found() {
		let diff = r#"some other content"#;
		assert_eq!(extract_command_name(diff), None);
	}

	#[test]
	fn test_extract_flag_name_found() {
		let diff = r#"long = "verbose""#;
		assert_eq!(extract_flag_name(diff), Some("verbose".to_string()));
	}

	#[test]
	fn test_extract_flag_name_not_found() {
		let diff = r#"some other content"#;
		assert_eq!(extract_flag_name(diff), None);
	}

	#[test]
	fn test_is_test_file_patterns() {
		assert!(is_test_file(Path::new("tests/integration_test.rs")));
		assert!(is_test_file(Path::new("src/__tests__/unit.test.ts")));
	}

	#[test]
	fn test_extract_changes_library() {
		let files = vec![PathBuf::from("src/lib.rs"), PathBuf::from("readme.md")];
		let result = extract_changes(
			&files,
			ArtifactType::Library,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.files_analyzed.len(), 1); // Only Rust files analyzed
		assert_eq!(extraction.files_skipped.len(), 1); // readme.md skipped
	}

	#[test]
	fn test_extract_changes_application() {
		let files = vec![
			PathBuf::from("src/routes/home.tsx"),
			PathBuf::from("src/components/Button.tsx"),
		];
		let result = extract_changes(
			&files,
			ArtifactType::Application,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.files_analyzed.len(), 2);
	}

	#[test]
	fn test_extract_changes_cli() {
		let files = vec![PathBuf::from("src/main.rs")];
		let result = extract_changes(
			&files,
			ArtifactType::CliTool,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.files_analyzed.len(), 1);
	}

	#[test]
	fn test_extract_changes_mixed() {
		let files = vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")];
		let result = extract_changes(
			&files,
			ArtifactType::Mixed,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
	}

	#[test]
	fn test_extract_library_changes_skips_test_files() {
		let files = vec![
			PathBuf::from("src/lib.rs"),
			PathBuf::from("tests/integration.rs"),
		];
		let result = extract_changes(
			&files,
			ArtifactType::Library,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.files_analyzed.len(), 1);
		assert_eq!(extraction.files_skipped.len(), 1);
		assert!(matches!(
			extraction.files_skipped[0].1,
			SkipReason::NotRelevant
		));
	}

	#[test]
	fn test_extract_application_changes_basic() {
		let files = vec![PathBuf::from("src/routes/home.tsx")];
		let result = extract_changes(
			&files,
			ArtifactType::Application,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.changes.len(), 1);
	}

	#[test]
	fn test_extract_application_changes_signature() {
		let files = vec![PathBuf::from("src/routes/home.tsx")];
		let result = extract_changes(
			&files,
			ArtifactType::Application,
			DetectionLevel::Signature,
			Path::new("."),
		);
		assert!(result.is_ok());
	}

	#[test]
	fn test_extract_application_changes_semantic() {
		let files = vec![PathBuf::from("src/routes/home.tsx")];
		let result = extract_changes(
			&files,
			ArtifactType::Application,
			DetectionLevel::Semantic,
			Path::new("."),
		);
		assert!(result.is_ok());
	}

	#[test]
	fn test_extract_cli_changes_basic() {
		let files = vec![PathBuf::from("src/main.rs")];
		let result = extract_changes(
			&files,
			ArtifactType::CliTool,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.changes.len(), 1);
	}

	#[test]
	fn test_extract_cli_changes_skips_non_rust() {
		let files = vec![PathBuf::from("readme.md")];
		let result = extract_changes(
			&files,
			ArtifactType::CliTool,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
		let extraction = result.unwrap();
		assert_eq!(extraction.files_analyzed.len(), 0);
		assert_eq!(extraction.files_skipped.len(), 1);
		assert!(matches!(
			extraction.files_skipped[0].1,
			SkipReason::UnsupportedExtension
		));
	}

	#[test]
	fn test_extract_mixed_changes() {
		let files = vec![
			PathBuf::from("src/lib.rs"),
			PathBuf::from("src/main.rs"),
			PathBuf::from("other/file.rs"),
		];
		let result = extract_changes(
			&files,
			ArtifactType::Mixed,
			DetectionLevel::Basic,
			Path::new("."),
		);
		assert!(result.is_ok());
	}
}
