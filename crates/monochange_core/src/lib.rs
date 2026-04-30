#![forbid(clippy::indexing_slicing)]

//! # `monochange_core`
//!
//! <!-- {=monochangeCoreCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_core` is the shared vocabulary for the `monochange` workspace.
//!
//! Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.
//!
//! ## Why use it?
//!
//! - avoid redefining package and release domain models in each crate
//! - share one error and result surface across discovery, planning, and command layers
//! - pass normalized workspace data between adapters and planners without extra translation
//!
//! ## Best for
//!
//! - implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
//! - moving normalized package or release data between crates without custom conversion code
//! - depending on the workspace domain model without pulling in discovery or planning behavior
//!
//! ## What it provides
//!
//! - normalized package and dependency records
//! - version-group definitions and planned group outcomes
//! - change signals and compatibility assessments
//! - changelog formats, changelog targets, structured release-note types, release-manifest types, source-automation config types, and changeset-policy evaluation types
//! - shared error and result types
//!
//! ## Example
//!
//! ```rust
//! use monochange_core::render_release_notes;
//! use monochange_core::ChangelogFormat;
//! use monochange_core::ReleaseNotesDocument;
//! use monochange_core::ReleaseNotesSection;
//!
//! let notes = ReleaseNotesDocument {
//!     title: "1.2.3".to_string(),
//!     summary: vec!["Grouped release for `sdk`.".to_string()],
//!     sections: vec![ReleaseNotesSection {
//!         title: "Features".to_string(),
//!         entries: vec!["- add keep-a-changelog output".to_string()],
//!         collapsed: false,
//!     }],
//! };
//!
//! let rendered = render_release_notes(ChangelogFormat::KeepAChangelog, &notes);
//!
//! assert!(rendered.contains("## 1.2.3"));
//! assert!(rendered.contains("### Features"));
//! assert!(rendered.contains("- add keep-a-changelog output"));
//! ```
//! <!-- {/monochangeCoreCrateDocs} -->

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub mod analysis;
pub mod git;
pub mod lint;

pub use analysis::*;
use ignore::gitignore::Gitignore;
use ignore::gitignore::GitignoreBuilder;
use semver::Version;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

pub type MonochangeResult<T> = Result<T, MonochangeError>;

/// Default release title template for primary versioning: `1.2.3 (2026-04-06)`.
pub const DEFAULT_RELEASE_TITLE_PRIMARY: &str = "{{ version }} ({{ date }})";
/// Default release title template for namespaced versioning: `my-pkg 1.2.3 (2026-04-06)`.
pub const DEFAULT_RELEASE_TITLE_NAMESPACED: &str = "{{ id }} {{ version }} ({{ date }})";
/// Default changelog version title for primary versioning (markdown-linked when source configured).
pub const DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY: &str =
	"{% if tag_url %}[{{ version }}]({{ tag_url }}){% else %}{{ version }}{% endif %} ({{ date }})";
/// Default changelog version title for namespaced versioning (markdown-linked when source configured).
pub const DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED: &str = "{% if tag_url %}{{ id }} [{{ version }}]({{ tag_url }}){% else %}{{ id }} {{ version }}{% endif %} ({{ date }})";

/// Default initial changelog header for the `monochange` changelog format.
pub const DEFAULT_INITIAL_CHANGELOG_HEADER_MONOCHANGE: &str = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\nThis changelog is managed by [monochange](https://github.com/monochange/monochange).";
/// Default initial changelog header for the `keep_a_changelog` changelog format.
pub const DEFAULT_INITIAL_CHANGELOG_HEADER_KEEP_A_CHANGELOG: &str = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\nThe format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),\nand this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).";

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MonochangeError {
	#[error("io error: {0}")]
	Io(String),
	#[error("config error: {0}")]
	Config(String),
	#[error("discovery error: {0}")]
	Discovery(String),
	#[error("{0}")]
	Diagnostic(String),
	#[error("io error at {path:?}: {source}")]
	IoSource {
		path: PathBuf,
		source: std::io::Error,
	},
	#[error("parse error at {path:?}: {source}")]
	Parse {
		path: PathBuf,
		source: Box<dyn std::error::Error + Send + Sync>,
	},
	#[cfg(feature = "http")]
	#[error("http error {context}: {source}")]
	HttpRequest {
		context: String,
		source: reqwest::Error,
	},
	#[error("interactive error: {message}")]
	Interactive { message: String },
	#[error("cancelled")]
	Cancelled,
}

impl MonochangeError {
	/// Render a stable human-readable diagnostic string for the error.
	#[must_use]
	pub fn render(&self) -> String {
		match self {
			Self::Diagnostic(report) => report.clone(),
			Self::IoSource { path, source } => {
				format!("io error at {}: {source}", path.display())
			}
			Self::Parse { path, source } => {
				format!("parse error at {}: {source}", path.display())
			}
			#[cfg(feature = "http")]
			Self::HttpRequest { context, source } => {
				format!("http error {context}: {source}")
			}
			Self::Interactive { message } => message.clone(),
			Self::Cancelled => "cancelled".to_string(),
			_ => self.to_string(),
		}
	}
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BumpSeverity {
	None,
	#[default]
	Patch,
	Minor,
	Major,
}

impl BumpSeverity {
	/// Return `true` when this severity produces a release.
	#[must_use]
	pub fn is_release(self) -> bool {
		self != Self::None
	}

	/// Returns `true` when the version is below `1.0.0`.
	///
	/// Pre-1.0 packages use a shifted bump policy where major changes bump
	/// the minor component and minor changes bump the patch component.
	#[must_use]
	pub fn is_pre_stable(version: &Version) -> bool {
		version.major == 0
	}

	/// Apply the severity to `version`, including pre-1.0 bump shifting.
	#[must_use]
	pub fn apply_to_version(self, version: &Version) -> Version {
		let effective = if Self::is_pre_stable(version) {
			match self {
				Self::Major => Self::Minor,
				Self::Minor => Self::Patch,
				other => other,
			}
		} else {
			self
		};

		let mut next = version.clone();
		match effective {
			Self::None => next,
			Self::Patch => {
				next.patch += 1;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
			Self::Minor => {
				next.minor += 1;
				next.patch = 0;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
			Self::Major => {
				next.major += 1;
				next.minor = 0;
				next.patch = 0;
				next.pre = semver::Prerelease::EMPTY;
				next.build = semver::BuildMetadata::EMPTY;
				next
			}
		}
	}
}

impl fmt::Display for BumpSeverity {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::None => "none",
			Self::Patch => "patch",
			Self::Minor => "minor",
			Self::Major => "major",
		})
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Ecosystem {
	Cargo,
	Npm,
	Deno,
	Dart,
	Flutter,
	Python,
	Go,
}

impl Ecosystem {
	/// Return the canonical config and serialization string for the ecosystem.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Cargo => "cargo",
			Self::Npm => "npm",
			Self::Deno => "deno",
			Self::Dart => "dart",
			Self::Flutter => "flutter",
			Self::Python => "python",
			Self::Go => "go",
		}
	}
}

impl fmt::Display for Ecosystem {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PublishState {
	Public,
	Private,
	Unpublished,
	Excluded,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DependencyKind {
	Runtime,
	Development,
	Build,
	Peer,
	Workspace,
	Unknown,
}

impl fmt::Display for DependencyKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(match self {
			Self::Runtime => "runtime",
			Self::Development => "development",
			Self::Build => "build",
			Self::Peer => "peer",
			Self::Workspace => "workspace",
			Self::Unknown => "unknown",
		})
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DependencySourceKind {
	Manifest,
	Workspace,
	Transitive,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageDependency {
	pub name: String,
	pub kind: DependencyKind,
	pub version_constraint: Option<String>,
	pub optional: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageRecord {
	pub id: String,
	pub name: String,
	pub ecosystem: Ecosystem,
	pub manifest_path: PathBuf,
	pub workspace_root: PathBuf,
	pub current_version: Option<Version>,
	pub publish_state: PublishState,
	pub version_group_id: Option<String>,
	pub metadata: BTreeMap<String, String>,
	pub declared_dependencies: Vec<PackageDependency>,
}

impl PackageRecord {
	#[allow(clippy::needless_pass_by_value)]
	/// Construct a normalized package record for a discovered package.
	#[must_use]
	pub fn new(
		ecosystem: Ecosystem,
		name: impl Into<String>,
		manifest_path: PathBuf,
		workspace_root: PathBuf,
		current_version: Option<Version>,
		publish_state: PublishState,
	) -> Self {
		let name = name.into();
		let normalized_workspace_root = normalize_path(&workspace_root);
		let normalized_manifest_path = normalize_path(&manifest_path);
		let id_path = relative_to_root(&normalized_workspace_root, &normalized_manifest_path)
			.unwrap_or_else(|| normalized_manifest_path.clone());
		let id = format!("{}:{}", ecosystem.as_str(), id_path.to_string_lossy());

		Self {
			id,
			name,
			ecosystem,
			manifest_path: normalized_manifest_path,
			workspace_root: normalized_workspace_root,
			current_version,
			publish_state,
			version_group_id: None,
			metadata: BTreeMap::new(),
			declared_dependencies: Vec::new(),
		}
	}

	/// Return the manifest path relative to `root` when possible.
	#[must_use]
	pub fn relative_manifest_path(&self, root: &Path) -> Option<PathBuf> {
		relative_to_root(root, &self.manifest_path)
	}
}

/// Normalize a path to an absolute, canonicalized path when possible.
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
	let absolute = if path.is_absolute() {
		path.to_path_buf()
	} else {
		env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
	};
	fs::canonicalize(&absolute).unwrap_or(absolute)
}

/// Return `path` relative to `root` after normalizing both paths.
#[must_use]
pub fn relative_to_root(root: &Path, path: &Path) -> Option<PathBuf> {
	let normalized_root = normalize_path(root);
	let normalized_path = normalize_path(path);
	normalized_path
		.strip_prefix(&normalized_root)
		.ok()
		.map(Path::to_path_buf)
}

#[derive(Clone, Debug)]
pub struct DiscoveryPathFilter {
	root: PathBuf,
	gitignore: Gitignore,
}

impl DiscoveryPathFilter {
	/// Build a discovery filter from repository gitignore rules.
	#[must_use]
	pub fn new(root: &Path) -> Self {
		let root = normalize_path(root);
		let mut builder = GitignoreBuilder::new(&root);
		for path in [root.join(".gitignore"), root.join(".git/info/exclude")] {
			if path.is_file() {
				let _ = builder.add(path);
			}
		}
		let gitignore = builder.build().unwrap_or_else(|_| Gitignore::empty());

		Self { root, gitignore }
	}

	/// Return `true` when `path` should be considered during discovery.
	#[must_use]
	pub fn allows(&self, path: &Path) -> bool {
		!self.is_ignored(path, path.is_dir())
	}

	/// Return `true` when directory traversal should continue into `path`.
	#[must_use]
	pub fn should_descend(&self, path: &Path) -> bool {
		!self.is_ignored(path, true)
	}

	fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
		if ignored_discovery_dir_name(path) || self.has_nested_git_worktree_ancestor(path, is_dir) {
			return true;
		}

		self.matches_gitignore(path, is_dir)
	}

	fn matches_gitignore(&self, path: &Path, is_dir: bool) -> bool {
		let normalized_path = normalize_path(path);
		normalized_path
			.strip_prefix(&self.root)
			.ok()
			.is_some_and(|relative| {
				self.gitignore
					.matched_path_or_any_parents(relative, is_dir)
					.is_ignore()
			})
	}

	fn has_nested_git_worktree_ancestor(&self, path: &Path, is_dir: bool) -> bool {
		let normalized_path = normalize_path(path);
		let mut current = if is_dir {
			normalized_path.clone()
		} else {
			normalized_path
				.parent()
				.unwrap_or(&normalized_path)
				.to_path_buf()
		};

		while current.starts_with(&self.root) && current != self.root {
			if current.join(".git").exists() {
				return true;
			}
			let Some(parent) = current.parent() else {
				break;
			};
			current = parent.to_path_buf();
		}

		false
	}
}

fn ignored_discovery_dir_name(path: &Path) -> bool {
	path.components().any(|component| {
		component.as_os_str().to_str().is_some_and(|name| {
			matches!(
				name,
				".git" | "target" | "node_modules" | ".devenv" | ".claude" | "book"
			)
		})
	})
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DependencyEdge {
	pub from_package_id: String,
	pub to_package_id: String,
	pub dependency_kind: DependencyKind,
	pub source_kind: DependencySourceKind,
	pub version_constraint: Option<String>,
	pub is_optional: bool,
	pub is_direct: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PackageType {
	Cargo,
	Npm,
	Deno,
	Dart,
	Flutter,
	Python,
	Go,
}

impl PackageType {
	/// Return the canonical config string for the package type.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Cargo => "cargo",
			Self::Npm => "npm",
			Self::Deno => "deno",
			Self::Dart => "dart",
			Self::Flutter => "flutter",
			Self::Python => "python",
			Self::Go => "go",
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum VersionFormat {
	#[default]
	Namespaced,
	Primary,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EcosystemType {
	Cargo,
	Npm,
	Deno,
	Dart,
	Python,
	Go,
}

impl EcosystemType {
	/// Return the default dependency-version prefix for this ecosystem.
	#[must_use]
	pub fn default_prefix(self) -> &'static str {
		match self {
			Self::Cargo | Self::Go => "",
			Self::Npm | Self::Deno | Self::Dart => "^",
			Self::Python => ">=",
		}
	}

	/// Return the manifest fields that usually contain dependency versions.
	#[must_use]
	pub fn default_fields(self) -> &'static [&'static str] {
		match self {
			Self::Cargo => &["dependencies", "dev-dependencies", "build-dependencies"],
			Self::Npm => &["dependencies", "devDependencies", "peerDependencies"],
			Self::Deno => &["imports"],
			Self::Dart => &["dependencies", "dev_dependencies"],
			Self::Python => &["dependencies"],
			Self::Go => &["require"],
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct JsonSpan {
	start: usize,
	end: usize,
}

/// Remove `//` and `/* ... */` comments from JSON-like text.
pub fn strip_json_comments(contents: &str) -> String {
	let bytes = contents.as_bytes();
	let mut output = String::with_capacity(contents.len());
	let mut cursor = 0usize;
	while let Some(&byte) = bytes.get(cursor) {
		if byte == b'"' {
			let start = cursor;
			cursor += 1;
			while let Some(&string_byte) = bytes.get(cursor) {
				cursor += 1;
				if string_byte == b'\\' {
					cursor += usize::from(bytes.get(cursor).is_some());
					continue;
				}
				if string_byte == b'"' {
					break;
				}
			}
			output.push_str(&contents[start..cursor]);
			continue;
		}
		if byte == b'/' && bytes.get(cursor + 1) == Some(&b'/') {
			cursor += 2;
			while let Some(&line_byte) = bytes.get(cursor) {
				if line_byte == b'\n' {
					break;
				}
				cursor += 1;
			}
			continue;
		}
		if byte == b'/' && bytes.get(cursor + 1) == Some(&b'*') {
			cursor += 2;
			while bytes.get(cursor).is_some() {
				if bytes.get(cursor) == Some(&b'*') && bytes.get(cursor + 1) == Some(&b'/') {
					cursor += 2;
					break;
				}
				cursor += 1;
			}
			continue;
		}
		output.push(char::from(byte));
		cursor += 1;
	}
	output
}

/// Update JSON manifest text while preserving most existing formatting.
#[must_use = "the manifest update result must be checked"]
pub fn update_json_manifest_text(
	contents: &str,
	owner_version: Option<&str>,
	fields: &[&str],
	versioned_deps: &BTreeMap<String, String>,
) -> MonochangeResult<String> {
	let root_start = json_root_object_start(contents)?;
	let mut replacements = Vec::<(JsonSpan, String)>::new();
	if let Some(owner_version) = owner_version
		&& let Some(span) = find_json_object_field_value_span(contents, root_start, "version")?
			.filter(|span| json_span_is_string(contents, *span))
	{
		replacements.push((span, render_json_string(owner_version)?));
	}
	for field in fields {
		let Some(field_span) = find_json_path_value_span(contents, root_start, field)? else {
			continue;
		};
		if json_span_is_object(contents, field_span) {
			for (dep_name, dep_version) in versioned_deps {
				let Some(dep_span) =
					find_json_object_field_value_span(contents, field_span.start, dep_name)?
						.filter(|span| json_span_is_string(contents, *span))
				else {
					continue;
				};
				replacements.push((dep_span, render_json_string(dep_version)?));
			}
			continue;
		}
		if let Some(owner_version) = owner_version
			&& json_span_is_string(contents, field_span)
		{
			replacements.push((field_span, render_json_string(owner_version)?));
		}
	}
	apply_json_replacements(contents, replacements)
}

fn render_json_string(value: &str) -> MonochangeResult<String> {
	serde_json::to_string(value).map_err(|error| MonochangeError::Config(error.to_string()))
}

fn apply_json_replacements(
	contents: &str,
	mut replacements: Vec<(JsonSpan, String)>,
) -> MonochangeResult<String> {
	replacements.sort_by_key(|right| std::cmp::Reverse(right.0.start));
	let mut updated = contents.to_string();
	for (span, replacement) in replacements {
		if span.start > span.end || span.end > updated.len() {
			return Err(MonochangeError::Config(
				"json edit range was out of bounds".to_string(),
			));
		}
		updated.replace_range(span.start..span.end, &replacement);
	}
	Ok(updated)
}

fn json_root_object_start(contents: &str) -> MonochangeResult<usize> {
	let start = skip_json_ws_and_comments(contents, 0);
	if contents.as_bytes().get(start) == Some(&b'{') {
		Ok(start)
	} else {
		Err(MonochangeError::Config(
			"expected JSON object at document root".to_string(),
		))
	}
}

fn find_json_path_value_span(
	contents: &str,
	root_start: usize,
	path: &str,
) -> MonochangeResult<Option<JsonSpan>> {
	let mut segments = path.split('.').filter(|segment| !segment.is_empty());
	let Some(first) = segments.next() else {
		return Ok(None);
	};
	let Some(mut span) = find_json_object_field_value_span(contents, root_start, first)? else {
		return Ok(None);
	};
	for segment in segments {
		if !json_span_is_object(contents, span) {
			return Ok(None);
		}
		let Some(next_span) = find_json_object_field_value_span(contents, span.start, segment)?
		else {
			return Ok(None);
		};
		span = next_span;
	}
	Ok(Some(span))
}

fn find_json_object_field_value_span(
	contents: &str,
	object_start: usize,
	key: &str,
) -> MonochangeResult<Option<JsonSpan>> {
	let bytes = contents.as_bytes();
	if bytes.get(object_start) != Some(&b'{') {
		return Err(MonochangeError::Config(
			"expected JSON object when locating field".to_string(),
		));
	}
	let mut cursor = object_start + 1;
	loop {
		cursor = skip_json_ws_and_comments(contents, cursor);
		match bytes.get(cursor) {
			Some(b'}') => return Ok(None),
			Some(b'"') => {}
			Some(_) => {
				return Err(MonochangeError::Config(
					"expected JSON object key".to_string(),
				));
			}
			None => {
				return Err(MonochangeError::Config(
					"unterminated JSON object".to_string(),
				));
			}
		}
		let (key_span, next) = parse_json_string_span(contents, cursor)?;
		let key_text = &contents[key_span.start..key_span.end];
		cursor = skip_json_ws_and_comments(contents, next);
		if bytes.get(cursor) != Some(&b':') {
			return Err(MonochangeError::Config(
				"expected `:` after JSON object key".to_string(),
			));
		}
		cursor = skip_json_ws_and_comments(contents, cursor + 1);
		let value_start = cursor;
		let value_end = skip_json_value(contents, value_start)?;
		if key_text == key {
			return Ok(Some(JsonSpan {
				start: value_start,
				end: value_end,
			}));
		}
		cursor = skip_json_ws_and_comments(contents, value_end);
		match bytes.get(cursor) {
			Some(b',') => {
				cursor += 1;
			}
			Some(b'}') => return Ok(None),
			Some(_) => {
				return Err(MonochangeError::Config(
					"expected `,` or `}` after JSON object value".to_string(),
				));
			}
			None => {
				return Err(MonochangeError::Config(
					"unterminated JSON object".to_string(),
				));
			}
		}
	}
}

fn skip_json_value(contents: &str, start: usize) -> MonochangeResult<usize> {
	let bytes = contents.as_bytes();
	let cursor = skip_json_ws_and_comments(contents, start);
	match bytes.get(cursor) {
		Some(b'"') => parse_json_string_span(contents, cursor).map(|(_, next)| next),
		Some(b'{') => skip_json_object(contents, cursor),
		Some(b'[') => skip_json_array(contents, cursor),
		Some(_) => Ok(skip_json_primitive(contents, cursor)),
		None => {
			Err(MonochangeError::Config(
				"unexpected end of JSON input".to_string(),
			))
		}
	}
}

fn skip_json_object(contents: &str, object_start: usize) -> MonochangeResult<usize> {
	let bytes = contents.as_bytes();
	let mut cursor = object_start + 1;
	loop {
		cursor = skip_json_ws_and_comments(contents, cursor);
		match bytes.get(cursor) {
			Some(b'}') => return Ok(cursor + 1),
			Some(b'"') => {}
			Some(_) => {
				return Err(MonochangeError::Config(
					"expected JSON object key".to_string(),
				));
			}
			None => {
				return Err(MonochangeError::Config(
					"unterminated JSON object".to_string(),
				));
			}
		}
		let (_, next) = parse_json_string_span(contents, cursor)?;
		cursor = skip_json_ws_and_comments(contents, next);
		if bytes.get(cursor) != Some(&b':') {
			return Err(MonochangeError::Config(
				"expected `:` after JSON object key".to_string(),
			));
		}
		cursor = skip_json_value(contents, cursor + 1)?;
		cursor = skip_json_ws_and_comments(contents, cursor);
		match bytes.get(cursor) {
			Some(b',') => {
				cursor += 1;
			}
			Some(b'}') => return Ok(cursor + 1),
			Some(_) => {
				return Err(MonochangeError::Config(
					"expected `,` or `}` after JSON object value".to_string(),
				));
			}
			None => {
				return Err(MonochangeError::Config(
					"unterminated JSON object".to_string(),
				));
			}
		}
	}
}

fn skip_json_array(contents: &str, array_start: usize) -> MonochangeResult<usize> {
	let bytes = contents.as_bytes();
	let mut cursor = array_start + 1;
	loop {
		cursor = skip_json_ws_and_comments(contents, cursor);
		match bytes.get(cursor) {
			Some(b']') => return Ok(cursor + 1),
			Some(_) => {
				cursor = skip_json_value(contents, cursor)?;
				cursor = skip_json_ws_and_comments(contents, cursor);
				match bytes.get(cursor) {
					Some(b',') => {
						cursor += 1;
					}
					Some(b']') => return Ok(cursor + 1),
					Some(_) => {
						return Err(MonochangeError::Config(
							"expected `,` or `]` after JSON array value".to_string(),
						));
					}
					None => {
						return Err(MonochangeError::Config(
							"unterminated JSON array".to_string(),
						));
					}
				}
			}
			None => {
				return Err(MonochangeError::Config(
					"unterminated JSON array".to_string(),
				));
			}
		}
	}
}

fn skip_json_primitive(contents: &str, start: usize) -> usize {
	let bytes = contents.as_bytes();
	let mut cursor = start;
	while let Some(&byte) = bytes.get(cursor) {
		if matches!(byte, b',' | b'}' | b']') || byte.is_ascii_whitespace() {
			break;
		}
		if byte == b'/' && matches!(bytes.get(cursor + 1), Some(b'/' | b'*')) {
			break;
		}
		cursor += 1;
	}
	cursor
}

fn parse_json_string_span(contents: &str, start: usize) -> MonochangeResult<(JsonSpan, usize)> {
	let bytes = contents.as_bytes();
	if bytes.get(start) != Some(&b'"') {
		return Err(MonochangeError::Config("expected JSON string".to_string()));
	}
	let mut cursor = start + 1;
	while let Some(&byte) = bytes.get(cursor) {
		if byte == b'\\' {
			// Escape sequence: verify there is a character after the backslash.
			let Some(&escape_char) = bytes.get(cursor + 1) else {
				return Err(MonochangeError::Config(
					"unterminated escape sequence in JSON string".to_string(),
				));
			};
			if escape_char == b'u' {
				// Unicode escape \uXXXX requires exactly 4 hex digits.
				for offset in 2..6 {
					match bytes.get(cursor + offset) {
						Some(b) if b.is_ascii_hexdigit() => {}
						Some(_) => {
							return Err(MonochangeError::Config(format!(
								"invalid unicode escape sequence in JSON string: expected hex digit at position {}",
								cursor + offset
							)));
						}
						None => {
							return Err(MonochangeError::Config(
								"incomplete unicode escape sequence in JSON string".to_string(),
							));
						}
					}
				}
				cursor += 6;
			} else {
				cursor += 2;
			}
			continue;
		}
		if byte == b'"' {
			return Ok((
				JsonSpan {
					start: start + 1,
					end: cursor,
				},
				cursor + 1,
			));
		}
		cursor += 1;
	}
	Err(MonochangeError::Config(
		"unterminated JSON string".to_string(),
	))
}

fn skip_json_ws_and_comments(contents: &str, start: usize) -> usize {
	let bytes = contents.as_bytes();
	let mut cursor = start;
	loop {
		while let Some(&byte) = bytes.get(cursor) {
			if !byte.is_ascii_whitespace() {
				break;
			}
			cursor += 1;
		}
		if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'/') {
			cursor += 2;
			while let Some(&byte) = bytes.get(cursor) {
				if byte == b'\n' {
					break;
				}
				cursor += 1;
			}
			continue;
		}
		if bytes.get(cursor) == Some(&b'/') && bytes.get(cursor + 1) == Some(&b'*') {
			cursor += 2;
			while bytes.get(cursor).is_some() {
				if bytes.get(cursor) == Some(&b'*') && bytes.get(cursor + 1) == Some(&b'/') {
					cursor += 2;
					break;
				}
				cursor += 1;
			}
			continue;
		}
		break;
	}
	cursor
}

fn json_span_is_string(contents: &str, span: JsonSpan) -> bool {
	contents.as_bytes().get(span.start) == Some(&b'"')
		&& span.end > span.start
		&& contents.as_bytes().get(span.end - 1) == Some(&b'"')
}

fn json_span_is_object(contents: &str, span: JsonSpan) -> bool {
	contents.as_bytes().get(span.start) == Some(&b'{')
		&& span.end > span.start
		&& contents.as_bytes().get(span.end - 1) == Some(&b'}')
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct VersionedFileDefinition {
	pub path: String,
	#[serde(rename = "type", default)]
	pub ecosystem_type: Option<EcosystemType>,
	#[serde(default)]
	pub prefix: Option<String>,
	#[serde(default)]
	pub fields: Option<Vec<String>>,
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default)]
	pub regex: Option<String>,
}

impl VersionedFileDefinition {
	/// Return `true` when the definition uses raw regex replacement mode.
	#[must_use]
	pub fn uses_regex(&self) -> bool {
		self.regex.is_some()
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChangelogDefinition {
	Disabled,
	PackageDefault,
	PathPattern(String),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ChangelogFormat {
	#[default]
	Monochange,
	KeepAChangelog,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogTarget {
	pub path: PathBuf,
	#[serde(default)]
	pub format: ChangelogFormat,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub initial_header: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseNotesSection {
	pub title: String,
	#[serde(default)]
	pub collapsed: bool,
	pub entries: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseNotesDocument {
	pub title: String,
	pub summary: Vec<String>,
	pub sections: Vec<ReleaseNotesSection>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogSectionDef {
	/// Display heading for the section in rendered changelogs.
	pub heading: String,
	/// Description of when this section should appear.
	#[serde(default)]
	pub description: Option<String>,
	/// Ordering priority for changelog rendering. Lower values appear first.
	#[serde(default = "default_changelog_section_priority")]
	pub priority: i8,
}

fn default_changelog_section_priority() -> i8 {
	100
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogSectionThresholds {
	/// Collapse sections whose priority is greater than or equal to this value.
	#[serde(default = "default_changelog_collapse_threshold")]
	pub collapse: i8,
	/// Omit sections whose priority is strictly greater than this value.
	#[serde(default = "default_changelog_ignored_threshold")]
	pub ignored: i8,
}

fn default_changelog_collapse_threshold() -> i8 {
	i8::MAX
}

fn default_changelog_ignored_threshold() -> i8 {
	i8::MAX
}

impl Default for ChangelogSectionThresholds {
	fn default() -> Self {
		Self {
			collapse: default_changelog_collapse_threshold(),
			ignored: default_changelog_ignored_threshold(),
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogType {
	/// Semver bump severity implied by this changeset type.
	#[serde(default = "default_changelog_type_bump")]
	pub bump: BumpSeverity,
	/// Section key this type routes to (references a `[changelog.sections]` key).
	pub section: String,
	/// Human-readable description of when to use this type.
	#[serde(default)]
	pub description: Option<String>,
}

fn default_changelog_type_bump() -> BumpSeverity {
	BumpSeverity::None
}

/// Top-level `[changelog]` configuration combining templates, sections, and types.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangelogSettings {
	#[serde(default)]
	pub templates: Vec<String>,
	#[serde(default)]
	pub sections: BTreeMap<String, ChangelogSectionDef>,
	#[serde(default)]
	pub section_thresholds: ChangelogSectionThresholds,
	#[serde(default)]
	pub types: BTreeMap<String, ChangelogType>,
}

impl Default for ChangelogSettings {
	fn default() -> Self {
		Self::defaults()
	}
}

impl ChangelogSettings {
	/// Return the built-in changelog configuration with default sections and types.
	#[must_use]
	pub fn defaults() -> Self {
		let mut sections = BTreeMap::new();
		sections.insert(
			"major".to_string(),
			ChangelogSectionDef {
				heading: "Major".to_string(),
				description: Some("Major version bumps".to_string()),
				priority: 5,
			},
		);
		sections.insert(
			"breaking".to_string(),
			ChangelogSectionDef {
				heading: "Breaking Change".to_string(),
				description: Some("API changes requiring migration".to_string()),
				priority: 10,
			},
		);
		sections.insert(
			"minor".to_string(),
			ChangelogSectionDef {
				heading: "Minor".to_string(),
				description: Some("Minor version bumps".to_string()),
				priority: 15,
			},
		);
		sections.insert(
			"feat".to_string(),
			ChangelogSectionDef {
				heading: "Added".to_string(),
				description: Some("New features added".to_string()),
				priority: 20,
			},
		);
		sections.insert(
			"change".to_string(),
			ChangelogSectionDef {
				heading: "Changed".to_string(),
				description: Some("Changes to existing functionality".to_string()),
				priority: 25,
			},
		);
		sections.insert(
			"fix".to_string(),
			ChangelogSectionDef {
				heading: "Fixed".to_string(),
				description: Some("Bug fixes".to_string()),
				priority: 30,
			},
		);
		sections.insert(
			"patch".to_string(),
			ChangelogSectionDef {
				heading: "Patch".to_string(),
				description: Some("Patch version bumps".to_string()),
				priority: 35,
			},
		);
		sections.insert(
			"test".to_string(),
			ChangelogSectionDef {
				heading: "Testing".to_string(),
				description: Some("Changes that only modify tests".to_string()),
				priority: 40,
			},
		);
		sections.insert(
			"refactor".to_string(),
			ChangelogSectionDef {
				heading: "Refactor".to_string(),
				description: Some("Code refactoring without functional changes".to_string()),
				priority: 40,
			},
		);
		sections.insert(
			"docs".to_string(),
			ChangelogSectionDef {
				heading: "Documentation".to_string(),
				description: Some("Changes that only modify documentation".to_string()),
				priority: 40,
			},
		);
		sections.insert(
			"security".to_string(),
			ChangelogSectionDef {
				heading: "Security".to_string(),
				description: Some("Security-related changes".to_string()),
				priority: 40,
			},
		);
		sections.insert(
			"perf".to_string(),
			ChangelogSectionDef {
				heading: "Performance".to_string(),
				description: Some("Performance improvements".to_string()),
				priority: 40,
			},
		);
		sections.insert(
			"none".to_string(),
			ChangelogSectionDef {
				heading: "None".to_string(),
				description: Some("No version bump".to_string()),
				priority: 50,
			},
		);

		let mut types = BTreeMap::new();
		types.insert(
			"breaking".to_string(),
			ChangelogType {
				bump: BumpSeverity::Major,
				section: "breaking".to_string(),
				description: Some("Breaking change with major bump".to_string()),
			},
		);
		types.insert(
			"major".to_string(),
			ChangelogType {
				bump: BumpSeverity::Major,
				section: "major".to_string(),
				description: Some("Major version bump".to_string()),
			},
		);
		types.insert(
			"feat".to_string(),
			ChangelogType {
				bump: BumpSeverity::Minor,
				section: "feat".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"minor".to_string(),
			ChangelogType {
				bump: BumpSeverity::Minor,
				section: "minor".to_string(),
				description: Some("Minor version bump".to_string()),
			},
		);
		types.insert(
			"change".to_string(),
			ChangelogType {
				bump: BumpSeverity::Minor,
				section: "change".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"fix".to_string(),
			ChangelogType {
				bump: BumpSeverity::Patch,
				section: "fix".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"patch".to_string(),
			ChangelogType {
				bump: BumpSeverity::Patch,
				section: "patch".to_string(),
				description: Some("Patch version bump".to_string()),
			},
		);
		types.insert(
			"refactor".to_string(),
			ChangelogType {
				bump: BumpSeverity::Patch,
				section: "refactor".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"test".to_string(),
			ChangelogType {
				bump: BumpSeverity::None,
				section: "test".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"none".to_string(),
			ChangelogType {
				bump: BumpSeverity::None,
				section: "none".to_string(),
				description: Some("No version bump".to_string()),
			},
		);
		types.insert(
			"docs".to_string(),
			ChangelogType {
				bump: BumpSeverity::None,
				section: "docs".to_string(),
				description: Some(String::new()),
			},
		);
		types.insert(
			"security".to_string(),
			ChangelogType {
				bump: BumpSeverity::None,
				section: "security".to_string(),
				description: Some(String::new()),
			},
		);

		Self {
			templates: vec![
				"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}".to_string(),
				"#### {{ summary }}\n\n{{ context }}".to_string(),
				"#### {{ summary }}\n\n{{ details }}".to_string(),
				"- {{ summary }}".to_string(),
			],
			sections,
			section_thresholds: ChangelogSectionThresholds::default(),
			types,
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PublishMode {
	#[default]
	Builtin,
	External,
}

impl PublishMode {
	/// Return the canonical serialized name for the publish mode.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Builtin => "builtin",
			Self::External => "external",
		}
	}
}

impl fmt::Display for PublishMode {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RegistryKind {
	CratesIo,
	Npm,
	Jsr,
	PubDev,
	Pypi,
	GoProxy,
}

impl RegistryKind {
	/// Return the canonical serialized name for the registry.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::CratesIo => "crates_io",
			Self::Npm => "npm",
			Self::Jsr => "jsr",
			Self::PubDev => "pub_dev",
			Self::Pypi => "pypi",
			Self::GoProxy => "go_proxy",
		}
	}
}

impl fmt::Display for RegistryKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PublishRegistry {
	Builtin(RegistryKind),
	Custom(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PlaceholderSettings {
	#[serde(default)]
	pub readme: Option<String>,
	#[serde(default)]
	pub readme_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PublishRateLimitSettings {
	#[serde(default)]
	pub enforce: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrustedPublishingSettings {
	#[serde(default = "default_true")]
	pub enabled: bool,
	#[serde(default)]
	pub repository: Option<String>,
	#[serde(default)]
	pub workflow: Option<String>,
	#[serde(default)]
	pub environment: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PublishAttestationSettings {
	#[serde(default)]
	pub require_registry_provenance: bool,
}

impl PublishAttestationSettings {
	#[must_use]
	pub fn is_default(&self) -> bool {
		self == &Self::default()
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct ReleaseAttestationSettings {
	#[serde(default)]
	pub require_github_artifact_attestations: bool,
}

impl ReleaseAttestationSettings {
	#[must_use]
	pub fn is_default(&self) -> bool {
		self == &Self::default()
	}
}

impl Default for TrustedPublishingSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			repository: None,
			workflow: None,
			environment: None,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PublishSettings {
	#[serde(default = "default_true")]
	pub enabled: bool,
	#[serde(default)]
	pub mode: PublishMode,
	#[serde(default)]
	pub registry: Option<PublishRegistry>,
	#[serde(default)]
	pub trusted_publishing: TrustedPublishingSettings,
	#[serde(
		default,
		skip_serializing_if = "PublishAttestationSettings::is_default"
	)]
	pub attestations: PublishAttestationSettings,
	#[serde(default)]
	pub rate_limits: PublishRateLimitSettings,
	#[serde(default)]
	pub placeholder: PlaceholderSettings,
}

impl Default for PublishSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			mode: PublishMode::default(),
			registry: None,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
			rate_limits: PublishRateLimitSettings::default(),
			placeholder: PlaceholderSettings::default(),
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageDefinition {
	pub id: String,
	pub path: PathBuf,
	pub package_type: PackageType,
	pub changelog: Option<ChangelogTarget>,
	pub excluded_changelog_types: Vec<String>,
	pub empty_update_message: Option<String>,
	#[serde(default)]
	pub release_title: Option<String>,
	#[serde(default)]
	pub changelog_version_title: Option<String>,
	pub versioned_files: Vec<VersionedFileDefinition>,
	#[serde(default)]
	pub ignore_ecosystem_versioned_files: bool,
	#[serde(default)]
	pub ignored_paths: Vec<String>,
	#[serde(default)]
	pub additional_paths: Vec<String>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	#[serde(default)]
	pub publish: PublishSettings,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub enum GroupChangelogInclude {
	#[default]
	All,
	GroupOnly,
	Selected(BTreeSet<String>),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupDefinition {
	pub id: String,
	pub packages: Vec<String>,
	pub changelog: Option<ChangelogTarget>,
	#[serde(default)]
	pub changelog_include: GroupChangelogInclude,
	pub excluded_changelog_types: Vec<String>,
	pub empty_update_message: Option<String>,
	#[serde(default)]
	pub release_title: Option<String>,
	#[serde(default)]
	pub changelog_version_title: Option<String>,
	pub versioned_files: Vec<VersionedFileDefinition>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceDefaults {
	pub parent_bump: BumpSeverity,
	pub include_private: bool,
	pub warn_on_group_mismatch: bool,
	pub strict_version_conflicts: bool,
	pub package_type: Option<PackageType>,
	pub changelog: Option<ChangelogDefinition>,
	pub changelog_format: ChangelogFormat,
	pub empty_update_message: Option<String>,
	pub release_title: Option<String>,
	pub changelog_version_title: Option<String>,
}

impl Default for WorkspaceDefaults {
	fn default() -> Self {
		Self {
			parent_bump: BumpSeverity::Patch,
			include_private: false,
			warn_on_group_mismatch: true,
			strict_version_conflicts: false,
			package_type: None,
			changelog: None,
			changelog_format: ChangelogFormat::Monochange,
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct EcosystemSettings {
	#[serde(default)]
	pub enabled: Option<bool>,
	#[serde(default)]
	pub roots: Vec<String>,
	#[serde(default)]
	pub exclude: Vec<String>,
	#[serde(default)]
	pub dependency_version_prefix: Option<String>,
	#[serde(default)]
	pub versioned_files: Vec<VersionedFileDefinition>,
	#[serde(default)]
	pub lockfile_commands: Vec<LockfileCommandDefinition>,
	#[serde(default)]
	pub publish: PublishSettings,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LockfileCommandDefinition {
	pub command: String,
	#[serde(default)]
	pub cwd: Option<PathBuf>,
	#[serde(default)]
	pub shell: ShellConfig,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LockfileCommandExecution {
	pub command: String,
	pub cwd: PathBuf,
	pub shell: ShellConfig,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CliInputKind {
	String,
	StringList,
	Path,
	Choice,
	Boolean,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliInputDefinition {
	pub name: String,
	#[serde(rename = "type")]
	pub kind: CliInputKind,
	#[serde(default)]
	pub help_text: Option<String>,
	#[serde(default)]
	pub required: bool,
	#[serde(default, deserialize_with = "deserialize_cli_input_default")]
	pub default: Option<String>,
	#[serde(default)]
	pub choices: Vec<String>,
	#[serde(default)]
	pub short: Option<char>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum CliInputDefault {
	String(String),
	Boolean(bool),
}

fn deserialize_cli_input_default<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let value = Option::<CliInputDefault>::deserialize(deserializer)?;
	Ok(value.map(|value| {
		match value {
			CliInputDefault::String(value) => value,
			CliInputDefault::Boolean(value) => value.to_string(),
		}
	}))
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CliStepInputValue {
	String(String),
	Boolean(bool),
	List(Vec<String>),
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandVariable {
	Version,
	GroupVersion,
	ReleasedPackages,
	ChangedFiles,
	Changesets,
}

/// Shell configuration for `Command` steps.
#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub enum ShellConfig {
	#[default]
	None,
	Default,
	Custom(String),
}

impl ShellConfig {
	/// Return the shell binary used to execute a `Command` step, if any.
	#[must_use]
	pub fn shell_binary(&self) -> Option<&str> {
		match self {
			Self::None => None,
			Self::Default => Some("sh"),
			Self::Custom(shell) => Some(shell),
		}
	}
}

impl Serialize for ShellConfig {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			Self::None => serializer.serialize_bool(false),
			Self::Default => serializer.serialize_bool(true),
			Self::Custom(shell) => serializer.serialize_str(shell),
		}
	}
}

impl<'de> Deserialize<'de> for ShellConfig {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		use serde::de;

		struct ShellConfigVisitor;

		impl de::Visitor<'_> for ShellConfigVisitor {
			type Value = ShellConfig;

			fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
				formatter.write_str("a boolean or a shell name string")
			}

			fn visit_bool<E: de::Error>(self, value: bool) -> Result<ShellConfig, E> {
				Ok(if value {
					ShellConfig::Default
				} else {
					ShellConfig::None
				})
			}

			fn visit_str<E: de::Error>(self, value: &str) -> Result<ShellConfig, E> {
				if value.is_empty() {
					return Err(de::Error::invalid_value(
						de::Unexpected::Str(value),
						&"a non-empty shell name",
					));
				}
				Ok(ShellConfig::Custom(value.to_string()))
			}

			fn visit_string<E: de::Error>(self, value: String) -> Result<ShellConfig, E> {
				self.visit_str(&value)
			}
		}

		deserializer.deserialize_any(ShellConfigVisitor)
	}
}

/// Built-in execution units for `[[cli.<command>.steps]]`.
///
/// `monochange` runs steps in order and lets later steps consume state created by
/// earlier ones. Use standalone steps such as `Validate`, `Discover`,
/// `AffectedPackages`, `DiagnoseChangesets`, and `RetargetRelease` when you want
/// inspection or repair. Use `PrepareRelease` when later steps need structured
/// release state.
///
/// See the CLI step reference in the book for full workflow guidance.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
#[non_exhaustive]
pub enum CliStepDefinition {
	/// Expose the resolved `monochange` configuration and workspace root.
	Config {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Validate `monochange` configuration and changesets, and run lint rules
	/// on package manifests.
	Validate {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Discover packages across supported ecosystems and render the result.
	Discover {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Display planned package and group versions without mutating release files.
	DisplayVersions {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Create a `.changeset/*.md` file from typed CLI inputs or interactive
	/// prompts.
	CreateChangeFile {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		show_progress: Option<bool>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Prepare a release and expose structured `release.*` context to later
	/// steps.
	PrepareRelease {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
		/// When true, do not error when there are no pending changesets.
		/// Instead, succeed with zero changesets, allowing downstream steps
		/// to check `number_of_changesets` in their `when` conditions.
		#[serde(default)]
		allow_empty_changesets: bool,
	},
	/// Create a local release commit with an embedded durable `ReleaseRecord`.
	///
	/// Requires a previous `PrepareRelease` step.
	CommitRelease {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		no_verify: bool,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Verify a commit is reachable from one of the configured release branches.
	VerifyReleaseBranch {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Publish hosted releases from a prepared `monochange` release.
	///
	/// Requires a previous `PrepareRelease` step and `[source]`
	/// configuration.
	PublishRelease {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Publish placeholder package versions for missing registry packages.
	PlaceholderPublish {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Publish package versions from durable monochange release state.
	PublishPackages {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Plan package-registry rate-limit windows for publish operations.
	PlanPublishRateLimits {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Open or update a hosted release request from prepared release state.
	///
	/// Requires a previous `PrepareRelease` step and `[source]`
	/// configuration.
	OpenReleaseRequest {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		no_verify: bool,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Comment on linked released issues after a prepared release.
	///
	/// Requires a previous `PrepareRelease` step and currently expects
	/// `[source].provider = "github"`.
	CommentReleasedIssues {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Evaluate affected packages and changeset coverage for changed files.
	///
	/// Standalone CI-oriented step.
	AffectedPackages {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Inspect parsed changeset data, provenance, and linked metadata.
	DiagnoseChangesets {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Repair a recent release by retargeting its stored release tag set.
	///
	/// This step is independent from `PrepareRelease` and exposes structured
	/// `retarget.*` state to later commands.
	RetargetRelease {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
	/// Run an arbitrary command with `monochange` template context.
	///
	/// Use this to bridge built-in `monochange` state into external tooling.
	Command {
		#[serde(default)]
		name: Option<String>,
		#[serde(default)]
		when: Option<String>,
		#[serde(default)]
		show_progress: Option<bool>,
		command: String,
		#[serde(default)]
		dry_run_command: Option<String>,
		#[serde(default)]
		shell: ShellConfig,
		#[serde(default)]
		id: Option<String>,
		#[serde(default)]
		variables: Option<BTreeMap<String, CommandVariable>>,
		#[serde(default)]
		inputs: BTreeMap<String, CliStepInputValue>,
	},
}

impl CliStepDefinition {
	/// Return the step-local input overrides configured for this step.
	#[must_use]
	pub fn inputs(&self) -> &BTreeMap<String, CliStepInputValue> {
		match self {
			Self::Config { inputs, .. }
			| Self::Validate { inputs, .. }
			| Self::Discover { inputs, .. }
			| Self::DisplayVersions { inputs, .. }
			| Self::CreateChangeFile { inputs, .. }
			| Self::PrepareRelease { inputs, .. }
			| Self::CommitRelease { inputs, .. }
			| Self::VerifyReleaseBranch { inputs, .. }
			| Self::PublishRelease { inputs, .. }
			| Self::PlaceholderPublish { inputs, .. }
			| Self::PublishPackages { inputs, .. }
			| Self::PlanPublishRateLimits { inputs, .. }
			| Self::OpenReleaseRequest { inputs, .. }
			| Self::CommentReleasedIssues { inputs, .. }
			| Self::AffectedPackages { inputs, .. }
			| Self::DiagnoseChangesets { inputs, .. }
			| Self::RetargetRelease { inputs, .. }
			| Self::Command { inputs, .. } => inputs,
		}
	}

	/// Return the optional configured display name for this step.
	#[must_use]
	pub fn name(&self) -> Option<&str> {
		match self {
			Self::Config { name, .. }
			| Self::Validate { name, .. }
			| Self::Discover { name, .. }
			| Self::DisplayVersions { name, .. }
			| Self::CreateChangeFile { name, .. }
			| Self::PrepareRelease { name, .. }
			| Self::CommitRelease { name, .. }
			| Self::VerifyReleaseBranch { name, .. }
			| Self::PublishRelease { name, .. }
			| Self::PlaceholderPublish { name, .. }
			| Self::PublishPackages { name, .. }
			| Self::PlanPublishRateLimits { name, .. }
			| Self::OpenReleaseRequest { name, .. }
			| Self::CommentReleasedIssues { name, .. }
			| Self::AffectedPackages { name, .. }
			| Self::DiagnoseChangesets { name, .. }
			| Self::RetargetRelease { name, .. }
			| Self::Command { name, .. } => name.as_deref(),
		}
	}

	/// Return the label shown in human-readable progress output.
	#[must_use]
	pub fn display_name(&self) -> &str {
		self.name().unwrap_or(self.kind_name())
	}

	/// Return the optional `when` condition for this step.
	#[must_use]
	pub fn when(&self) -> Option<&str> {
		match self {
			Self::Config { when, .. }
			| Self::Validate { when, .. }
			| Self::Discover { when, .. }
			| Self::DisplayVersions { when, .. }
			| Self::CreateChangeFile { when, .. }
			| Self::PrepareRelease { when, .. }
			| Self::CommitRelease { when, .. }
			| Self::VerifyReleaseBranch { when, .. }
			| Self::PublishRelease { when, .. }
			| Self::PlaceholderPublish { when, .. }
			| Self::PublishPackages { when, .. }
			| Self::PlanPublishRateLimits { when, .. }
			| Self::OpenReleaseRequest { when, .. }
			| Self::CommentReleasedIssues { when, .. }
			| Self::AffectedPackages { when, .. }
			| Self::DiagnoseChangesets { when, .. }
			| Self::RetargetRelease { when, .. }
			| Self::Command { when, .. } => when.as_deref(),
		}
	}

	/// Return whether progress output is explicitly enabled or disabled.
	#[must_use]
	pub fn show_progress(&self) -> Option<bool> {
		match self {
			Self::CreateChangeFile { show_progress, .. } | Self::Command { show_progress, .. } => {
				*show_progress
			}
			_ => None,
		}
	}

	/// Return the built-in step kind name.
	#[must_use]
	pub fn kind_name(&self) -> &'static str {
		match self {
			Self::Config { .. } => "Config",
			Self::Validate { .. } => "Validate",
			Self::Discover { .. } => "Discover",
			Self::DisplayVersions { .. } => "DisplayVersions",
			Self::CreateChangeFile { .. } => "CreateChangeFile",
			Self::PrepareRelease { .. } => "PrepareRelease",
			Self::CommitRelease { .. } => "CommitRelease",
			Self::VerifyReleaseBranch { .. } => "VerifyReleaseBranch",
			Self::PublishRelease { .. } => "PublishRelease",
			Self::PlaceholderPublish { .. } => "PlaceholderPublish",
			Self::PublishPackages { .. } => "PublishPackages",
			Self::PlanPublishRateLimits { .. } => "PlanPublishRateLimits",
			Self::OpenReleaseRequest { .. } => "OpenReleaseRequest",
			Self::CommentReleasedIssues { .. } => "CommentReleasedIssues",
			Self::AffectedPackages { .. } => "AffectedPackages",
			Self::DiagnoseChangesets { .. } => "DiagnoseChangesets",
			Self::RetargetRelease { .. } => "RetargetRelease",
			Self::Command { .. } => "Command",
		}
	}

	/// Returns the set of input names that this step kind recognises.
	///
	/// `Command` steps accept any input (returns `None`).
	/// All built-in step kinds return `Some(…)` with the exhaustive set of
	/// input names they consume at runtime.
	#[must_use]
	pub fn valid_input_names(&self) -> Option<&'static [&'static str]> {
		match self {
			Self::Config { .. } => Some(&[]),
			Self::Validate { .. } => Some(&["fix"]),
			Self::CommitRelease { .. } => Some(&["no_verify"]),
			Self::VerifyReleaseBranch { .. } => Some(&["from"]),
			Self::Discover { .. } | Self::DisplayVersions { .. } | Self::PrepareRelease { .. } => {
				Some(&["format"])
			}
			Self::CommentReleasedIssues { .. } => {
				Some(&["format", "from-ref", "auto-close-issues"])
			}
			Self::PublishRelease { .. } => Some(&["format", "from-ref", "draft"]),
			Self::OpenReleaseRequest { .. } => Some(&["format", "no_verify"]),
			Self::PlaceholderPublish { .. } => Some(&["format", "package"]),
			Self::PublishPackages { .. } => {
				Some(&["format", "output", "package", "readiness", "resume"])
			}
			Self::PlanPublishRateLimits { .. } => {
				Some(&["format", "mode", "package", "ci", "readiness"])
			}
			Self::CreateChangeFile { .. } => {
				Some(&[
					"interactive",
					"package",
					"bump",
					"version",
					"reason",
					"type",
					"details",
					"output",
				])
			}
			Self::AffectedPackages { .. } => {
				Some(&["format", "changed_paths", "from", "verify", "label"])
			}
			Self::DiagnoseChangesets { .. } => Some(&["format", "changeset"]),
			Self::RetargetRelease { .. } => Some(&["from", "target", "force", "sync_provider"]),
			Self::Command { .. } => None,
		}
	}

	/// Returns the valid choice values for a named input on this step, if any.
	#[must_use]
	pub fn valid_input_choices(&self, name: &str) -> Option<&'static [&'static str]> {
		match self {
			Self::Discover { .. }
			| Self::DisplayVersions { .. }
			| Self::PrepareRelease { .. }
			| Self::PublishRelease { .. }
			| Self::CommentReleasedIssues { .. }
			| Self::OpenReleaseRequest { .. }
			| Self::AffectedPackages { .. }
			| Self::DiagnoseChangesets { .. }
			| Self::PlaceholderPublish { .. }
			| Self::PublishPackages { .. } => {
				match name {
					"format" => Some(&["text", "json", "md"]),
					_ => None,
				}
			}
			Self::PlanPublishRateLimits { .. } => {
				match name {
					"format" => Some(&["text", "json", "md"]),
					"mode" => Some(&["local", "ci"]),
					"ci" => Some(&["github", "gitlab", "generic"]),
					_ => None,
				}
			}
			Self::CreateChangeFile { .. } => {
				match name {
					"bump" => Some(&["major", "minor", "patch", "none"]),
					_ => None,
				}
			}
			Self::RetargetRelease { .. } | _ => None,
		}
	}

	/// Returns the expected [`CliInputKind`] for a named input on this step,
	/// or `None` when the step is a `Command` (accepts anything) or the name
	/// is unrecognised.
	#[must_use]
	pub fn expected_input_kind(&self, name: &str) -> Option<CliInputKind> {
		match self {
			Self::Validate { .. } => {
				match name {
					"fix" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::CommitRelease { .. } => {
				match name {
					"no_verify" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::VerifyReleaseBranch { .. } => {
				match name {
					"from" => Some(CliInputKind::String),
					_ => None,
				}
			}
			Self::Config { .. } | Self::Command { .. } => None,
			Self::Discover { .. } | Self::DisplayVersions { .. } | Self::PrepareRelease { .. } => {
				matches!(name, "format").then_some(CliInputKind::Choice)
			}
			Self::CommentReleasedIssues { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"from-ref" => Some(CliInputKind::String),
					"auto-close-issues" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::PublishRelease { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"from-ref" => Some(CliInputKind::String),
					"draft" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::OpenReleaseRequest { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"no_verify" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::PlaceholderPublish { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"package" => Some(CliInputKind::StringList),
					_ => None,
				}
			}
			Self::PublishPackages { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"package" => Some(CliInputKind::StringList),
					"output" | "readiness" | "resume" => Some(CliInputKind::Path),
					_ => None,
				}
			}
			Self::PlanPublishRateLimits { .. } => {
				match name {
					"package" => Some(CliInputKind::StringList),
					"readiness" => Some(CliInputKind::Path),
					"format" | "mode" | "ci" => Some(CliInputKind::Choice),
					_ => None,
				}
			}
			Self::CreateChangeFile { .. } => {
				match name {
					"interactive" => Some(CliInputKind::Boolean),
					"package" => Some(CliInputKind::StringList),
					"bump" => Some(CliInputKind::Choice),
					"version" | "reason" | "type" | "details" => Some(CliInputKind::String),
					"output" => Some(CliInputKind::Path),
					_ => None,
				}
			}
			Self::AffectedPackages { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"changed_paths" | "label" => Some(CliInputKind::StringList),
					"from" => Some(CliInputKind::String),
					"verify" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
			Self::DiagnoseChangesets { .. } => {
				match name {
					"format" => Some(CliInputKind::Choice),
					"changeset" => Some(CliInputKind::StringList),
					_ => None,
				}
			}
			Self::RetargetRelease { .. } => {
				match name {
					"from" | "target" => Some(CliInputKind::String),
					"force" | "sync_provider" => Some(CliInputKind::Boolean),
					_ => None,
				}
			}
		}
	}

	pub fn step_kebab_name(&self) -> String {
		let name = self.kind_name();
		let mut result = String::new();
		let mut prev_upper = false;
		for ch in name.chars() {
			if ch.is_uppercase() {
				if !result.is_empty() && !prev_upper {
					result.push('-');
				}
				result.push(ch.to_ascii_lowercase());
				prev_upper = true;
			} else {
				result.push(ch);
				prev_upper = false;
			}
		}
		result
	}

	/// Return the set of input definitions for this step kind.
	#[must_use]
	pub fn step_inputs_schema(&self) -> Vec<CliInputDefinition> {
		let Some(names) = self.valid_input_names() else {
			return Vec::new();
		};
		names
			.iter()
			.map(|name| {
				let kind = self
					.expected_input_kind(name)
					.unwrap_or(CliInputKind::String);
				let choices = self
					.valid_input_choices(name)
					.map(|c| {
						#[allow(clippy::redundant_closure_for_method_calls)]
						c.iter().map(|s| s.to_string()).collect::<Vec<_>>()
					})
					.unwrap_or_default();
				let default = if *name == "sync_provider" {
					Some("true".to_string())
				} else {
					None
				};
				CliInputDefinition {
					name: name.to_string(),
					kind,
					help_text: None,
					required: false,
					default,
					choices,
					short: None,
				}
			})
			.collect()
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CliCommandDefinition {
	pub name: String,
	#[serde(default)]
	pub help_text: Option<String>,
	#[serde(default)]
	pub inputs: Vec<CliInputDefinition>,
	#[serde(default)]
	pub steps: Vec<CliStepDefinition>,
}

/// Render release notes in the selected changelog format.
#[must_use]
pub fn render_release_notes(format: ChangelogFormat, document: &ReleaseNotesDocument) -> String {
	match format {
		ChangelogFormat::Monochange => render_monochange_release_notes(document),
		ChangelogFormat::KeepAChangelog => render_keep_a_changelog_release_notes(document),
	}
}

fn render_monochange_release_notes(document: &ReleaseNotesDocument) -> String {
	let mut lines = vec![format!("## {}", document.title), String::new()];
	for (index, paragraph) in document.summary.iter().enumerate() {
		if index > 0 {
			lines.push(String::new());
		}
		lines.push(paragraph.clone());
	}
	let include_section_headings = document.sections.len() > 1
		|| document
			.sections
			.iter()
			.any(|section| section.title != "Changed" || section.collapsed);
	for section in &document.sections {
		if section.entries.is_empty() {
			continue;
		}
		if !lines.last().is_some_and(String::is_empty) {
			lines.push(String::new());
		}
		if section.collapsed {
			push_collapsed_release_note_section(&mut lines, section);
			continue;
		}
		if include_section_headings {
			lines.push(format!("### {}", section.title));
			lines.push(String::new());
		}
		push_release_note_entries(&mut lines, &section.entries);
	}
	lines.join("\n")
}

fn render_keep_a_changelog_release_notes(document: &ReleaseNotesDocument) -> String {
	let mut lines = vec![format!("## {}", document.title), String::new()];
	for (index, paragraph) in document.summary.iter().enumerate() {
		if index > 0 {
			lines.push(String::new());
		}
		lines.push(paragraph.clone());
	}
	for section in &document.sections {
		if section.entries.is_empty() {
			continue;
		}
		if !lines.last().is_some_and(String::is_empty) {
			lines.push(String::new());
		}
		if section.collapsed {
			push_collapsed_release_note_section(&mut lines, section);
			continue;
		}
		lines.push(format!("### {}", section.title));
		lines.push(String::new());
		push_release_note_entries(&mut lines, &section.entries);
	}
	lines.join("\n")
}

fn push_collapsed_release_note_section(lines: &mut Vec<String>, section: &ReleaseNotesSection) {
	lines.push("<details>".to_string());
	lines.push(format!(
		"<summary><strong>{}</strong></summary>",
		section.title
	));
	lines.push(String::new());
	push_release_note_entries(lines, &section.entries);
	lines.push("</details>".to_string());
}

fn push_release_note_entries(lines: &mut Vec<String>, entries: &[String]) {
	for (index, entry) in entries.iter().enumerate() {
		let trimmed = entry.trim();
		if trimmed.contains('\n') {
			lines.extend(trimmed.lines().map(ToString::to_string));
			if index + 1 < entries.len() {
				lines.push(String::new());
			}
			continue;
		}
		if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with('#') {
			lines.push(trimmed.to_string());
		} else {
			lines.push(format!("- {trimmed}"));
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseOwnerKind {
	Package,
	Group,
}

impl ReleaseOwnerKind {
	/// Return the canonical serialized name for the release-owner kind.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Package => "package",
			Self::Group => "group",
		}
	}
}

impl fmt::Display for ReleaseOwnerKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub tag_name: String,
	pub members: Vec<String>,
	#[serde(default)]
	pub rendered_title: String,
	#[serde(default)]
	pub rendered_changelog_title: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestChangelog {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub path: PathBuf,
	pub format: ChangelogFormat,
	pub notes: ReleaseNotesDocument,
	pub rendered: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePublicationTarget {
	pub package: String,
	pub ecosystem: Ecosystem,
	#[serde(default)]
	pub registry: Option<PublishRegistry>,
	pub version: String,
	#[serde(default)]
	pub mode: PublishMode,
	#[serde(default)]
	pub trusted_publishing: TrustedPublishingSettings,
	#[serde(
		default,
		skip_serializing_if = "PublishAttestationSettings::is_default"
	)]
	pub attestations: PublishAttestationSettings,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitOperation {
	PlaceholderPublish,
	Publish,
	Update,
}

impl RateLimitOperation {
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::PlaceholderPublish => "placeholder_publish",
			Self::Publish => "publish",
			Self::Update => "update",
		}
	}
}

impl fmt::Display for RateLimitOperation {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitEvidenceKind {
	Official,
	SourceCode,
	Secondary,
	ConservativeDefault,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitConfidence {
	High,
	Medium,
	Low,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitEvidence {
	pub title: String,
	pub url: String,
	pub kind: RateLimitEvidenceKind,
	pub notes: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRateLimitPolicy {
	pub registry: RegistryKind,
	pub operation: RateLimitOperation,
	pub limit: Option<u32>,
	pub window_seconds: Option<u64>,
	pub confidence: RateLimitConfidence,
	pub notes: String,
	pub evidence: Vec<RateLimitEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryRateLimitWindowPlan {
	pub registry: RegistryKind,
	pub operation: RateLimitOperation,
	pub limit: Option<u32>,
	pub window_seconds: Option<u64>,
	pub pending: usize,
	pub batches_required: usize,
	pub fits_single_window: bool,
	pub confidence: RateLimitConfidence,
	pub notes: String,
	pub evidence: Vec<RateLimitEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRateLimitBatch {
	pub registry: RegistryKind,
	pub operation: RateLimitOperation,
	pub batch_index: usize,
	pub total_batches: usize,
	pub packages: Vec<String>,
	pub recommended_wait_seconds: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub batches: Vec<PublishRateLimitBatch>,
	pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostingProviderKind {
	#[default]
	#[serde(rename = "generic_git")]
	GenericGit,
	#[serde(rename = "github")]
	GitHub,
	#[serde(rename = "gitlab")]
	GitLab,
	#[serde(rename = "gitea")]
	Gitea,
	#[serde(rename = "bitbucket")]
	Bitbucket,
}

impl HostingProviderKind {
	/// Return the canonical serialized name for the hosting provider.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::GenericGit => "generic_git",
			Self::GitHub => "github",
			Self::GitLab => "gitlab",
			Self::Gitea => "gitea",
			Self::Bitbucket => "bitbucket",
		}
	}
}

impl fmt::Display for HostingProviderKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostingCapabilities {
	pub commit_web_urls: bool,
	pub actor_profiles: bool,
	pub review_request_lookup: bool,
	pub related_issues: bool,
	pub issue_comments: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostedActorSourceKind {
	#[default]
	CommitAuthor,
	CommitCommitter,
	ReviewRequestAuthor,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostedActorRef {
	pub provider: HostingProviderKind,
	#[serde(default)]
	pub host: Option<String>,
	#[serde(default)]
	pub id: Option<String>,
	#[serde(default)]
	pub login: Option<String>,
	#[serde(default)]
	pub display_name: Option<String>,
	#[serde(default)]
	pub url: Option<String>,
	pub source: HostedActorSourceKind,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostedCommitRef {
	pub provider: HostingProviderKind,
	#[serde(default)]
	pub host: Option<String>,
	pub sha: String,
	pub short_sha: String,
	#[serde(default)]
	pub url: Option<String>,
	#[serde(default)]
	pub authored_at: Option<String>,
	#[serde(default)]
	pub committed_at: Option<String>,
	#[serde(default)]
	pub author_name: Option<String>,
	#[serde(default)]
	pub author_email: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostedReviewRequestKind {
	#[default]
	PullRequest,
	MergeRequest,
}

impl HostedReviewRequestKind {
	/// Return the canonical serialized name for the review-request kind.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::PullRequest => "pull_request",
			Self::MergeRequest => "merge_request",
		}
	}
}

impl fmt::Display for HostedReviewRequestKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostedReviewRequestRef {
	pub provider: HostingProviderKind,
	#[serde(default)]
	pub host: Option<String>,
	pub kind: HostedReviewRequestKind,
	pub id: String,
	#[serde(default)]
	pub title: Option<String>,
	#[serde(default)]
	pub url: Option<String>,
	#[serde(default)]
	pub author: Option<HostedActorRef>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HostedIssueRelationshipKind {
	#[default]
	ClosedByReviewRequest,
	ReferencedByReviewRequest,
	Mentioned,
	Manual,
}

impl HostedIssueRelationshipKind {
	/// Return the canonical serialized name for the issue relationship kind.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::ClosedByReviewRequest => "closed_by_review_request",
			Self::ReferencedByReviewRequest => "referenced_by_review_request",
			Self::Mentioned => "mentioned",
			Self::Manual => "manual",
		}
	}
}

impl fmt::Display for HostedIssueRelationshipKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostedIssueRef {
	pub provider: HostingProviderKind,
	#[serde(default)]
	pub host: Option<String>,
	pub id: String,
	#[serde(default)]
	pub title: Option<String>,
	#[serde(default)]
	pub url: Option<String>,
	pub relationship: HostedIssueRelationshipKind,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChangesetRevision {
	#[serde(default)]
	pub actor: Option<HostedActorRef>,
	#[serde(default)]
	pub commit: Option<HostedCommitRef>,
	#[serde(default)]
	pub review_request: Option<HostedReviewRequestRef>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChangesetContext {
	pub provider: HostingProviderKind,
	#[serde(default)]
	pub host: Option<String>,
	#[serde(default)]
	pub capabilities: HostingCapabilities,
	#[serde(default)]
	pub introduced: Option<ChangesetRevision>,
	#[serde(default)]
	pub last_updated: Option<ChangesetRevision>,
	#[serde(default)]
	pub related_issues: Vec<HostedIssueRef>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesetTargetKind {
	Package,
	Group,
}

impl ChangesetTargetKind {
	/// Return the canonical serialized name for the changeset target kind.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Package => "package",
			Self::Group => "group",
		}
	}
}

impl fmt::Display for ChangesetTargetKind {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedChangesetTarget {
	pub id: String,
	pub kind: ChangesetTargetKind,
	#[serde(default)]
	pub bump: Option<BumpSeverity>,
	pub origin: String,
	#[serde(default)]
	pub evidence_refs: Vec<String>,
	#[serde(default)]
	pub change_type: Option<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedChangeset {
	pub path: PathBuf,
	#[serde(default)]
	pub summary: Option<String>,
	#[serde(default)]
	pub details: Option<String>,
	pub targets: Vec<PreparedChangesetTarget>,
	#[serde(default, alias = "context")]
	pub context: Option<ChangesetContext>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlanDecision {
	pub package: String,
	pub bump: BumpSeverity,
	pub trigger: String,
	pub planned_version: Option<String>,
	pub reasons: Vec<String>,
	pub upstream_sources: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlanGroup {
	pub id: String,
	pub planned_version: Option<String>,
	pub members: Vec<String>,
	pub bump: BumpSeverity,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestCompatibilityEvidence {
	pub package: String,
	pub provider: String,
	pub severity: BumpSeverity,
	pub summary: String,
	pub confidence: String,
	pub evidence_location: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifestPlan {
	pub workspace_root: PathBuf,
	pub decisions: Vec<ReleaseManifestPlanDecision>,
	pub groups: Vec<ReleaseManifestPlanGroup>,
	pub warnings: Vec<String>,
	pub unresolved_items: Vec<String>,
	pub compatibility_evidence: Vec<ReleaseManifestCompatibilityEvidence>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseManifest {
	pub command: String,
	pub dry_run: bool,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseManifestTarget>,
	pub released_packages: Vec<String>,
	pub changed_files: Vec<PathBuf>,
	pub changelogs: Vec<ReleaseManifestChangelog>,
	#[serde(default)]
	pub package_publications: Vec<PackagePublicationTarget>,
	#[serde(default)]
	pub changesets: Vec<PreparedChangeset>,
	#[serde(default)]
	pub deleted_changesets: Vec<PathBuf>,
	pub plan: ReleaseManifestPlan,
}

/// Current supported `ReleaseRecord` schema version.
pub const RELEASE_RECORD_SCHEMA_VERSION: u64 = 1;
/// Required `ReleaseRecord.kind` discriminator.
pub const RELEASE_RECORD_KIND: &str = "monochange.releaseRecord";
/// Human-readable heading used for commit-embedded release records.
pub const RELEASE_RECORD_HEADING: &str = "## monochange Release Record";
/// Opening marker for a commit-embedded release record block.
pub const RELEASE_RECORD_START_MARKER: &str = "<!-- monochange:release-record:start -->";
/// Closing marker for a commit-embedded release record block.
pub const RELEASE_RECORD_END_MARKER: &str = "<!-- monochange:release-record:end -->";

const fn release_record_schema_version() -> u64 {
	RELEASE_RECORD_SCHEMA_VERSION
}

fn default_release_record_kind() -> String {
	RELEASE_RECORD_KIND.to_string()
}

fn default_true() -> bool {
	true
}

fn default_pull_request_branch_prefix() -> String {
	"monochange/release".to_string()
}

fn default_pull_request_base() -> String {
	"main".to_string()
}

fn default_pull_request_title() -> String {
	"chore(release): prepare release".to_string()
}

fn default_pull_request_labels() -> Vec<String> {
	vec!["release".to_string(), "automated".to_string()]
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRecordTarget {
	pub id: String,
	pub kind: ReleaseOwnerKind,
	pub version: String,
	pub version_format: VersionFormat,
	pub tag: bool,
	pub release: bool,
	pub tag_name: String,
	#[serde(default)]
	pub members: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRecordProvider {
	pub kind: SourceProvider,
	pub owner: String,
	pub repo: String,
	#[serde(default)]
	pub host: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRecord {
	#[serde(default = "release_record_schema_version")]
	pub schema_version: u64,
	#[serde(default = "default_release_record_kind")]
	pub kind: String,
	pub created_at: String,
	pub command: String,
	#[serde(default)]
	pub version: Option<String>,
	#[serde(default)]
	pub group_version: Option<String>,
	pub release_targets: Vec<ReleaseRecordTarget>,
	pub released_packages: Vec<String>,
	pub changed_files: Vec<PathBuf>,
	#[serde(default)]
	pub package_publications: Vec<PackagePublicationTarget>,
	#[serde(default)]
	pub updated_changelogs: Vec<PathBuf>,
	#[serde(default)]
	pub deleted_changesets: Vec<PathBuf>,
	#[serde(default)]
	pub changesets: Vec<PreparedChangeset>,
	#[serde(default)]
	pub provider: Option<ReleaseRecordProvider>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRecordDiscovery {
	pub input_ref: String,
	pub resolved_commit: String,
	pub record_commit: String,
	pub distance: usize,
	pub record: ReleaseRecord,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetargetOperation {
	Planned,
	Moved,
	AlreadyUpToDate,
	Skipped,
	Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetargetTagResult {
	pub tag_name: String,
	pub from_commit: String,
	pub to_commit: String,
	pub operation: RetargetOperation,
	#[serde(default)]
	pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetargetProviderOperation {
	Planned,
	Synced,
	AlreadyAligned,
	Unsupported,
	Skipped,
	Failed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetargetProviderResult {
	pub provider: SourceProvider,
	pub tag_name: String,
	pub target_commit: String,
	pub operation: RetargetProviderOperation,
	#[serde(default)]
	pub url: Option<String>,
	#[serde(default)]
	pub message: Option<String>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetargetPlan {
	pub record_commit: String,
	pub target_commit: String,
	pub is_descendant: bool,
	pub force: bool,
	pub git_tag_updates: Vec<RetargetTagResult>,
	pub provider_updates: Vec<RetargetProviderResult>,
	pub sync_provider: bool,
	pub dry_run: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetargetResult {
	pub record_commit: String,
	pub target_commit: String,
	pub force: bool,
	pub git_tag_results: Vec<RetargetTagResult>,
	pub provider_results: Vec<RetargetProviderResult>,
	pub sync_provider: bool,
	pub dry_run: bool,
}

/// Return all tag names owned by the release record, deduplicated and sorted.
#[must_use]
pub fn release_record_tag_names(record: &ReleaseRecord) -> Vec<String> {
	record
		.release_targets
		.iter()
		.filter(|target| target.tag)
		.map(|target| target.tag_name.clone())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

/// Return tag names that correspond to outward hosted releases.
#[must_use]
pub fn release_record_release_tag_names(record: &ReleaseRecord) -> Vec<String> {
	record
		.release_targets
		.iter()
		.filter(|target| target.release)
		.map(|target| target.tag_name.clone())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

#[derive(Debug, Error)]
pub enum ReleaseRecordError {
	#[error("no monochange release record block found")]
	NotFound,
	#[error("found multiple monochange release record blocks")]
	MultipleBlocks,
	#[error("found a release record start marker without a matching end marker")]
	MissingEndMarker,
	#[error("found a malformed release record block without a fenced json payload")]
	MissingJsonBlock,
	#[error("release record is missing required `kind`")]
	MissingKind,
	#[error("release record is missing required `schemaVersion`")]
	MissingSchemaVersion,
	#[error("release record uses unsupported kind `{0}`")]
	UnsupportedKind(String),
	#[error("release record uses unsupported schemaVersion {0}")]
	UnsupportedSchemaVersion(u64),
	#[error("release record json error: {0}")]
	InvalidJson(#[from] serde_json::Error),
}

/// Result type used by release-record parsing and rendering helpers.
pub type ReleaseRecordResult<T> = Result<T, ReleaseRecordError>;

/// Render a `ReleaseRecord` into the reserved commit-message block format.
#[must_use = "the rendered record result must be checked"]
pub fn render_release_record_block(record: &ReleaseRecord) -> ReleaseRecordResult<String> {
	if record.kind != RELEASE_RECORD_KIND {
		return Err(ReleaseRecordError::UnsupportedKind(record.kind.clone()));
	}
	if record.schema_version != RELEASE_RECORD_SCHEMA_VERSION {
		return Err(ReleaseRecordError::UnsupportedSchemaVersion(
			record.schema_version,
		));
	}
	let json = serde_json::to_string_pretty(record)?;
	Ok(format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n{json}\n```\n{RELEASE_RECORD_END_MARKER}"
	))
}

/// Parse a `ReleaseRecord` from a full commit message body.
#[must_use = "the parsed record result must be checked"]
pub fn parse_release_record_block(commit_message: &str) -> ReleaseRecordResult<ReleaseRecord> {
	let start_matches = commit_message
		.match_indices(RELEASE_RECORD_START_MARKER)
		.collect::<Vec<_>>();
	if start_matches.is_empty() {
		return Err(ReleaseRecordError::NotFound);
	}
	let end_matches = commit_message
		.match_indices(RELEASE_RECORD_END_MARKER)
		.collect::<Vec<_>>();
	if end_matches.is_empty() {
		return Err(ReleaseRecordError::MissingEndMarker);
	}
	if start_matches.len() > 1 || end_matches.len() > 1 {
		return Err(ReleaseRecordError::MultipleBlocks);
	}
	let (start_index, _) = start_matches
		.first()
		.copied()
		.unwrap_or_else(|| unreachable!("start marker count was validated"));
	let (end_index, _) = end_matches
		.first()
		.copied()
		.unwrap_or_else(|| unreachable!("end marker count was validated"));
	if end_index <= start_index {
		return Err(ReleaseRecordError::MissingEndMarker);
	}
	let block_contents =
		&commit_message[start_index + RELEASE_RECORD_START_MARKER.len()..end_index];
	let json_text = extract_release_record_json(block_contents)?;
	let raw = serde_json::from_str::<serde_json::Value>(&json_text)?;
	let kind = raw
		.get("kind")
		.and_then(serde_json::Value::as_str)
		.ok_or(ReleaseRecordError::MissingKind)?;
	if kind != RELEASE_RECORD_KIND {
		return Err(ReleaseRecordError::UnsupportedKind(kind.to_string()));
	}
	let schema_version = raw
		.get("schemaVersion")
		.and_then(serde_json::Value::as_u64)
		.ok_or(ReleaseRecordError::MissingSchemaVersion)?;
	if schema_version != RELEASE_RECORD_SCHEMA_VERSION {
		return Err(ReleaseRecordError::UnsupportedSchemaVersion(schema_version));
	}
	serde_json::from_value(raw).map_err(ReleaseRecordError::InvalidJson)
}

fn extract_release_record_json(block_contents: &str) -> ReleaseRecordResult<String> {
	let lines = block_contents.trim().lines().collect::<Vec<_>>();
	if lines.first().map(|line| line.trim_end()) != Some("```json") {
		return Err(ReleaseRecordError::MissingJsonBlock);
	}
	let Some(closing_index) = lines
		.iter()
		.enumerate()
		.skip(1)
		.find_map(|(index, line)| (line.trim_end() == "```").then_some(index))
	else {
		return Err(ReleaseRecordError::MissingJsonBlock);
	};
	if lines
		.iter()
		.skip(closing_index + 1)
		.any(|line| !line.trim().is_empty())
	{
		return Err(ReleaseRecordError::MissingJsonBlock);
	}
	let json = lines
		.iter()
		.skip(1)
		.take(closing_index.saturating_sub(1))
		.copied()
		.collect::<Vec<_>>()
		.join("\n");
	if json.trim().is_empty() {
		return Err(ReleaseRecordError::MissingJsonBlock);
	}
	Ok(json)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReleaseNotesSource {
	#[default]
	Monochange,
	#[serde(rename = "github_generated")]
	GitHubGenerated,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderReleaseSettings {
	#[serde(default = "default_true")]
	pub enabled: bool,
	#[serde(default)]
	pub draft: bool,
	#[serde(default)]
	pub prerelease: bool,
	#[serde(default)]
	pub generate_notes: bool,
	#[serde(default)]
	pub source: ProviderReleaseNotesSource,
	#[serde(default = "default_release_branch_patterns")]
	pub branches: Vec<String>,
	#[serde(default = "default_true")]
	pub enforce_for_tags: bool,
	#[serde(default = "default_true")]
	pub enforce_for_publish: bool,
	#[serde(default)]
	pub enforce_for_commit: bool,
	#[serde(
		default,
		skip_serializing_if = "ReleaseAttestationSettings::is_default"
	)]
	pub attestations: ReleaseAttestationSettings,
}

impl Default for ProviderReleaseSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::default(),
			branches: default_release_branch_patterns(),
			enforce_for_tags: true,
			enforce_for_publish: true,
			enforce_for_commit: false,
			attestations: ReleaseAttestationSettings::default(),
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderMergeRequestSettings {
	#[serde(default = "default_true")]
	pub enabled: bool,
	#[serde(default = "default_pull_request_branch_prefix")]
	pub branch_prefix: String,
	#[serde(default = "default_pull_request_base")]
	pub base: String,
	#[serde(default = "default_pull_request_title")]
	pub title: String,
	#[serde(default = "default_pull_request_labels")]
	pub labels: Vec<String>,
	#[serde(default)]
	pub auto_merge: bool,
	#[serde(default)]
	pub verified_commits: bool,
}

impl Default for ProviderMergeRequestSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			branch_prefix: default_pull_request_branch_prefix(),
			base: default_pull_request_base(),
			title: default_pull_request_title(),
			labels: default_pull_request_labels(),
			auto_merge: false,
			verified_commits: false,
		}
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangesetAffectedSettings {
	#[serde(default = "default_true")]
	pub enabled: bool,
	#[serde(default = "default_true")]
	pub required: bool,
	#[serde(default)]
	pub skip_labels: Vec<String>,
	#[serde(default = "default_true")]
	pub comment_on_failure: bool,
	#[serde(default)]
	pub changed_paths: Vec<String>,
	#[serde(default)]
	pub ignored_paths: Vec<String>,
}

impl Default for ChangesetAffectedSettings {
	fn default() -> Self {
		Self {
			enabled: true,
			required: true,
			skip_labels: Vec::new(),
			comment_on_failure: true,
			changed_paths: Vec::new(),
			ignored_paths: Vec::new(),
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct ChangesetSettings {
	#[serde(default)]
	pub affected: ChangesetAffectedSettings,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangesetPolicyStatus {
	Passed,
	Failed,
	Skipped,
	NotRequired,
}

impl ChangesetPolicyStatus {
	/// Return the canonical serialized name for the policy status.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Passed => "passed",
			Self::Failed => "failed",
			Self::Skipped => "skipped",
			Self::NotRequired => "not_required",
		}
	}
}

impl fmt::Display for ChangesetPolicyStatus {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

fn default_release_branch_patterns() -> Vec<String> {
	vec!["main".to_string()]
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesetPolicyEvaluation {
	pub status: ChangesetPolicyStatus,
	pub required: bool,
	#[serde(default)]
	pub enforce: bool,
	pub summary: String,
	#[serde(default)]
	pub comment: Option<String>,
	#[serde(default)]
	pub labels: Vec<String>,
	#[serde(default)]
	pub matched_skip_labels: Vec<String>,
	#[serde(default)]
	pub changed_paths: Vec<String>,
	#[serde(default)]
	pub matched_paths: Vec<String>,
	#[serde(default)]
	pub ignored_paths: Vec<String>,
	#[serde(default)]
	pub changeset_paths: Vec<String>,
	#[serde(default)]
	pub affected_package_ids: Vec<String>,
	#[serde(default)]
	pub covered_package_ids: Vec<String>,
	#[serde(default)]
	pub uncovered_package_ids: Vec<String>,
	#[serde(default)]
	pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
pub enum SourceProvider {
	#[default]
	#[serde(rename = "github")]
	GitHub,
	#[serde(rename = "gitlab")]
	GitLab,
	#[serde(rename = "gitea")]
	Gitea,
}

impl SourceProvider {
	/// Return the canonical serialized name for the source provider.
	#[must_use]
	pub fn as_str(self) -> &'static str {
		match self {
			Self::GitHub => "github",
			Self::GitLab => "gitlab",
			Self::Gitea => "gitea",
		}
	}
}

impl fmt::Display for SourceProvider {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str(self.as_str())
	}
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct SourceCapabilities {
	pub draft_releases: bool,
	pub prereleases: bool,
	pub generated_release_notes: bool,
	pub auto_merge_change_requests: bool,
	pub released_issue_comments: bool,
	pub requires_host: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceConfiguration {
	#[serde(default)]
	pub provider: SourceProvider,
	pub owner: String,
	pub repo: String,
	#[serde(default)]
	pub host: Option<String>,
	#[serde(default)]
	pub api_url: Option<String>,
	#[serde(default)]
	pub releases: ProviderReleaseSettings,
	#[serde(default)]
	pub pull_requests: ProviderMergeRequestSettings,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HostedSourceFeatures {
	pub batched_changeset_context_lookup: bool,
	pub released_issue_comments: bool,
	pub release_retarget_sync: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedIssueCommentPlan {
	pub repository: String,
	pub issue_id: String,
	pub issue_url: Option<String>,
	pub body: String,
	#[serde(default)]
	pub close: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedIssueCommentOperation {
	Created,
	SkippedExisting,
	Closed,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedIssueCommentOutcome {
	pub repository: String,
	pub issue_id: String,
	pub operation: HostedIssueCommentOperation,
	pub url: Option<String>,
}

pub trait HostedSourceAdapter: Sync {
	fn provider(&self) -> SourceProvider;

	fn features(&self) -> HostedSourceFeatures {
		HostedSourceFeatures::default()
	}

	fn annotate_changeset_context(
		&self,
		source: &SourceConfiguration,
		changesets: &mut [PreparedChangeset],
	);

	fn enrich_changeset_context(
		&self,
		source: &SourceConfiguration,
		changesets: &mut [PreparedChangeset],
	) {
		self.annotate_changeset_context(source, changesets);
	}

	fn plan_released_issue_comments(
		&self,
		_source: &SourceConfiguration,
		_manifest: &ReleaseManifest,
	) -> Vec<HostedIssueCommentPlan> {
		Vec::new()
	}

	fn comment_released_issues(
		&self,
		source: &SourceConfiguration,
		manifest: &ReleaseManifest,
	) -> MonochangeResult<Vec<HostedIssueCommentOutcome>> {
		let plans = self.plan_released_issue_comments(source, manifest);
		if plans.is_empty() {
			return Ok(Vec::new());
		}
		Err(MonochangeError::Config(format!(
			"released issue comments are not yet supported for {}",
			self.provider()
		)))
	}

	fn plan_retargeted_releases(
		&self,
		tag_results: &[RetargetTagResult],
	) -> Vec<RetargetProviderResult> {
		let provider = self.provider();
		let supports_sync = self.features().release_retarget_sync;
		tag_results
			.iter()
			.map(|update| {
				RetargetProviderResult {
					provider,
					tag_name: update.tag_name.clone(),
					target_commit: update.to_commit.clone(),
					operation: if supports_sync {
						RetargetProviderOperation::Planned
					} else {
						RetargetProviderOperation::Unsupported
					},
					url: None,
					message: (!supports_sync).then_some(format!(
						"provider sync is not yet supported for {provider} release retargeting"
					)),
				}
			})
			.collect()
	}

	fn sync_retargeted_releases(
		&self,
		source: &SourceConfiguration,
		tag_results: &[RetargetTagResult],
		dry_run: bool,
	) -> MonochangeResult<Vec<RetargetProviderResult>> {
		if dry_run {
			return Ok(self.plan_retargeted_releases(tag_results));
		}
		Err(MonochangeError::Config(format!(
			"provider sync is not yet supported for {} release retargeting",
			source.provider
		)))
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReleaseRequest {
	pub provider: SourceProvider,
	pub repository: String,
	pub owner: String,
	pub repo: String,
	pub target_id: String,
	pub target_kind: ReleaseOwnerKind,
	pub tag_name: String,
	pub name: String,
	pub body: Option<String>,
	pub draft: bool,
	pub prerelease: bool,
	pub generate_release_notes: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceReleaseOperation {
	Created,
	Updated,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReleaseOutcome {
	pub provider: SourceProvider,
	pub repository: String,
	pub tag_name: String,
	pub operation: SourceReleaseOperation,
	pub url: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitMessage {
	pub subject: String,
	#[serde(default)]
	pub body: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceChangeRequest {
	pub provider: SourceProvider,
	pub repository: String,
	pub owner: String,
	pub repo: String,
	pub base_branch: String,
	pub head_branch: String,
	pub title: String,
	pub body: String,
	pub labels: Vec<String>,
	pub auto_merge: bool,
	pub commit_message: CommitMessage,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChangeRequestOperation {
	Created,
	Updated,
	Skipped,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceChangeRequestOutcome {
	pub provider: SourceProvider,
	pub repository: String,
	pub number: u64,
	pub head_branch: String,
	pub operation: SourceChangeRequestOperation,
	pub url: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct EffectiveReleaseIdentity {
	pub owner_id: String,
	pub owner_kind: ReleaseOwnerKind,
	pub group_id: Option<String>,
	pub tag: bool,
	pub release: bool,
	pub version_format: VersionFormat,
	pub members: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceConfiguration {
	pub root_path: PathBuf,
	pub defaults: WorkspaceDefaults,
	pub changelog: ChangelogSettings,
	pub packages: Vec<PackageDefinition>,
	pub groups: Vec<GroupDefinition>,
	pub cli: Vec<CliCommandDefinition>,
	pub changesets: ChangesetSettings,
	pub source: Option<SourceConfiguration>,
	pub lints: lint::WorkspaceLintSettings,
	pub cargo: EcosystemSettings,
	pub npm: EcosystemSettings,
	pub deno: EcosystemSettings,
	pub dart: EcosystemSettings,
	pub python: EcosystemSettings,
	pub go: EcosystemSettings,
}

impl WorkspaceConfiguration {
	/// Look up a configured package by its package id.
	#[must_use]
	pub fn package_by_id(&self, package_id: &str) -> Option<&PackageDefinition> {
		self.packages
			.iter()
			.find(|package| package.id == package_id)
	}

	/// Look up a configured group by its group id.
	#[must_use]
	pub fn group_by_id(&self, group_id: &str) -> Option<&GroupDefinition> {
		self.groups.iter().find(|group| group.id == group_id)
	}

	/// Return the configured group that directly owns `package_id`, if any.
	#[must_use]
	pub fn group_for_package(&self, package_id: &str) -> Option<&GroupDefinition> {
		self.groups
			.iter()
			.find(|group| group.packages.iter().any(|member| member == package_id))
	}

	/// Resolve the effective outward release identity for a package.
	#[must_use]
	pub fn effective_release_identity(&self, package_id: &str) -> Option<EffectiveReleaseIdentity> {
		let package = self.package_by_id(package_id)?;
		if let Some(group) = self.group_for_package(package_id) {
			return Some(EffectiveReleaseIdentity {
				owner_id: group.id.clone(),
				owner_kind: ReleaseOwnerKind::Group,
				group_id: Some(group.id.clone()),
				tag: group.tag,
				release: group.release,
				version_format: group.version_format,
				members: group.packages.clone(),
			});
		}

		Some(EffectiveReleaseIdentity {
			owner_id: package.id.clone(),
			owner_kind: ReleaseOwnerKind::Package,
			group_id: None,
			tag: package.tag,
			release: package.release,
			version_format: package.version_format,
			members: vec![package.id.clone()],
		})
	}
}

/// Return the built-in CLI command definitions used when config omits them.
#[must_use]
pub fn default_cli_commands() -> Vec<CliCommandDefinition> {
	vec![]
}

/// Return all built-in step variants except `Command`.
#[must_use]
pub fn all_step_variants() -> Vec<CliStepDefinition> {
	vec![
		CliStepDefinition::Config {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::Validate {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::Discover {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::DisplayVersions {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::CreateChangeFile {
			name: None,
			when: None,
			show_progress: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PrepareRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
			allow_empty_changesets: false,
		},
		CliStepDefinition::CommitRelease {
			name: None,
			when: None,
			no_verify: false,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::VerifyReleaseBranch {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PublishRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PlaceholderPublish {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PublishPackages {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PlanPublishRateLimits {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::OpenReleaseRequest {
			name: None,
			when: None,
			no_verify: false,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::CommentReleasedIssues {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::AffectedPackages {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::DiagnoseChangesets {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::RetargetRelease {
			name: None,
			when: None,
			inputs: BTreeMap::new(),
		},
	]
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionGroup {
	pub group_id: String,
	pub display_name: String,
	pub members: Vec<String>,
	pub mismatch_detected: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannedVersionGroup {
	pub group_id: String,
	pub display_name: String,
	pub members: Vec<String>,
	pub mismatch_detected: bool,
	pub planned_version: Option<Version>,
	pub recommended_bump: BumpSeverity,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangeSignal {
	pub package_id: String,
	pub requested_bump: Option<BumpSeverity>,
	pub explicit_version: Option<Version>,
	pub change_origin: String,
	pub evidence_refs: Vec<String>,
	pub notes: Option<String>,
	pub details: Option<String>,
	pub change_type: Option<String>,
	pub caused_by: Vec<String>,
	pub source_path: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompatibilityAssessment {
	pub package_id: String,
	pub provider_id: String,
	pub severity: BumpSeverity,
	pub confidence: String,
	pub summary: String,
	pub evidence_location: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleaseDecision {
	pub package_id: String,
	pub trigger_type: String,
	pub recommended_bump: BumpSeverity,
	pub planned_version: Option<Version>,
	pub group_id: Option<String>,
	pub reasons: Vec<String>,
	pub upstream_sources: Vec<String>,
	pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReleasePlan {
	pub workspace_root: PathBuf,
	pub decisions: Vec<ReleaseDecision>,
	pub groups: Vec<PlannedVersionGroup>,
	pub warnings: Vec<String>,
	pub unresolved_items: Vec<String>,
	pub compatibility_evidence: Vec<CompatibilityAssessment>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryReport {
	pub workspace_root: PathBuf,
	pub packages: Vec<PackageRecord>,
	pub dependencies: Vec<DependencyEdge>,
	pub version_groups: Vec<VersionGroup>,
	pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterDiscovery {
	pub packages: Vec<PackageRecord>,
	pub warnings: Vec<String>,
}

pub trait EcosystemAdapter {
	fn ecosystem(&self) -> Ecosystem;

	fn discover(&self, root: &Path) -> MonochangeResult<AdapterDiscovery>;
}

/// Build dependency edges by matching declared dependency names to known packages.
#[must_use]
pub fn materialize_dependency_edges(packages: &[PackageRecord]) -> Vec<DependencyEdge> {
	let mut package_ids_by_name = BTreeMap::<String, Vec<String>>::new();
	for package in packages {
		package_ids_by_name
			.entry(package.name.clone())
			.or_default()
			.push(package.id.clone());
	}

	let mut edges = Vec::new();
	for package in packages {
		for dependency in &package.declared_dependencies {
			if let Some(target_package_ids) = package_ids_by_name.get(&dependency.name) {
				for target_package_id in target_package_ids {
					edges.push(DependencyEdge {
						from_package_id: package.id.clone(),
						to_package_id: target_package_id.clone(),
						dependency_kind: dependency.kind,
						source_kind: DependencySourceKind::Manifest,
						version_constraint: dependency.version_constraint.clone(),
						is_optional: dependency.optional,
						is_direct: true,
					});
				}
			}
		}
	}

	edges
}

#[cfg(test)]
mod proptest_bump_severity;

#[cfg(test)]
mod __tests;
