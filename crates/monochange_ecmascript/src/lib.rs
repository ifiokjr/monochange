#![forbid(clippy::indexing_slicing)]

//! Shared ECMAScript semantic-analysis helpers for `monochange` adapters.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::AnalyzedFileChange;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use oxc_allocator::Allocator;
use oxc_ast::ast::Declaration;
use oxc_ast::ast::ExportDefaultDeclarationKind;
use oxc_ast::ast::ImportOrExportKind;
use oxc_ast::ast::ModuleDeclaration;
use oxc_ast::ast::ModuleExportName;
use oxc_ast::ast::TSModuleDeclarationName;
use oxc_parser::Parser;
use oxc_span::SourceType;

#[derive(Debug, Clone, Copy)]
pub struct EcmascriptExportConfig {
	pub source_extensions: &'static [&'static str],
	pub module_roots_to_strip: &'static [&'static str],
	pub ignored_module_stems: &'static [&'static str],
	pub strip_declaration_stem_suffix: bool,
	pub legacy_supports_declare_prefix: bool,
	pub legacy_supports_namespace_exports: bool,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExportedSymbol {
	pub item_kind: String,
	pub item_path: String,
	pub signature: String,
	pub file_path: PathBuf,
}

pub fn snapshot_exported_symbols(
	snapshot: Option<&PackageSnapshot>,
	changed_files: &[AnalyzedFileChange],
	config: &EcmascriptExportConfig,
) -> BTreeMap<(String, String), ExportedSymbol> {
	let mut symbols = BTreeMap::new();

	if let Some(snapshot) = snapshot {
		for file in &snapshot.files {
			if !is_source_file(&file.path, config) {
				continue;
			}

			for symbol in collect_public_symbols(file, config) {
				symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
			}
		}

		return symbols;
	}

	for change in changed_files {
		let Some(contents) = change
			.after_contents
			.as_deref()
			.or(change.before_contents.as_deref())
		else {
			continue;
		};

		if !is_source_file(&change.package_path, config) {
			continue;
		}

		let file = PackageSnapshotFile {
			path: change.package_path.clone(),
			contents: contents.to_string(),
		};
		for symbol in collect_public_symbols(&file, config) {
			symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
		}
	}

	symbols
}

pub fn diff_public_symbols(
	before: &BTreeMap<(String, String), ExportedSymbol>,
	after: &BTreeMap<(String, String), ExportedSymbol>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	for (key, after_symbol) in after {
		match before.get(key) {
			None => {
				changes.push(build_symbol_change(
					SemanticChangeKind::Added,
					after_symbol,
					None,
					Some(after_symbol.signature.clone()),
				));
			}
			Some(before_symbol) if before_symbol.signature != after_symbol.signature => {
				changes.push(build_symbol_change(
					SemanticChangeKind::Modified,
					after_symbol,
					Some(before_symbol.signature.clone()),
					Some(after_symbol.signature.clone()),
				));
			}
			Some(_) => {}
		}
	}

	for (key, before_symbol) in before {
		if after.contains_key(key) {
			continue;
		}

		changes.push(build_symbol_change(
			SemanticChangeKind::Removed,
			before_symbol,
			Some(before_symbol.signature.clone()),
			None,
		));
	}

	changes
}

fn is_source_file(path: &Path, config: &EcmascriptExportConfig) -> bool {
	let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
		return false;
	};

	if !config.source_extensions.contains(&extension) {
		return false;
	}

	!path.starts_with("dist") && !path.starts_with("build") && !path.starts_with("node_modules")
}

fn collect_public_symbols(
	file: &PackageSnapshotFile,
	config: &EcmascriptExportConfig,
) -> Vec<ExportedSymbol> {
	parse_public_symbols(file, config)
		.unwrap_or_else(|| collect_public_symbols_with_legacy_scanner(file, config))
}

fn parse_public_symbols(
	file: &PackageSnapshotFile,
	config: &EcmascriptExportConfig,
) -> Option<Vec<ExportedSymbol>> {
	let source_type = SourceType::from_path(&file.path).ok()?;
	let allocator = Allocator::default();
	let parser_return = Parser::new(&allocator, &file.contents, source_type).parse();
	let module_prefix = module_prefix_for_file(&file.path, config);
	let mut symbols = Vec::new();

	for statement in &parser_return.program.body {
		let Some(declaration) = statement.as_module_declaration() else {
			continue;
		};
		collect_public_symbols_from_module_declaration(
			declaration,
			&module_prefix,
			&file.path,
			&file.contents,
			&mut symbols,
		);
	}

	Some(symbols)
}

fn collect_public_symbols_from_module_declaration(
	declaration: &ModuleDeclaration<'_>,
	module_prefix: &[String],
	file_path: &Path,
	source_text: &str,
	output: &mut Vec<ExportedSymbol>,
) {
	match declaration {
		ModuleDeclaration::ExportNamedDeclaration(export) => {
			let signature = normalize_signature(export.span.source_text(source_text));
			if let Some(declaration) = &export.declaration {
				collect_public_symbols_from_declaration(
					declaration,
					module_prefix,
					file_path,
					&signature,
					output,
				);
				return;
			}

			for specifier in &export.specifiers {
				let item_kind = if matches!(export.export_kind, ImportOrExportKind::Type)
					|| matches!(specifier.export_kind, ImportOrExportKind::Type)
				{
					"type_reexport"
				} else {
					"reexport"
				};
				push_symbol(
					output,
					item_kind,
					module_prefix,
					module_export_name(&specifier.exported),
					&signature,
					file_path,
				);
			}
		}
		ModuleDeclaration::ExportAllDeclaration(export) => {
			let signature = normalize_signature(export.span.source_text(source_text));
			if let Some(exported) = &export.exported {
				push_symbol(
					output,
					"namespace_reexport",
					module_prefix,
					module_export_name(exported),
					&signature,
					file_path,
				);
				return;
			}

			push_symbol(
				output,
				"wildcard_reexport",
				module_prefix,
				export.source.value.to_string(),
				&signature,
				file_path,
			);
		}
		ModuleDeclaration::ExportDefaultDeclaration(export) => {
			let signature = normalize_signature(export.span.source_text(source_text));
			collect_public_symbols_from_default_export(
				&export.declaration,
				module_prefix,
				file_path,
				&signature,
				output,
			);
		}
		_ => {}
	}
}

fn collect_public_symbols_from_declaration(
	declaration: &Declaration<'_>,
	module_prefix: &[String],
	file_path: &Path,
	signature: &str,
	output: &mut Vec<ExportedSymbol>,
) {
	match declaration {
		Declaration::FunctionDeclaration(function) => {
			if let Some(identifier) = &function.id {
				push_symbol(
					output,
					"function",
					module_prefix,
					identifier.name.to_string(),
					signature,
					file_path,
				);
			}
		}
		Declaration::ClassDeclaration(class) => {
			if let Some(identifier) = &class.id {
				push_symbol(
					output,
					"class",
					module_prefix,
					identifier.name.to_string(),
					signature,
					file_path,
				);
			}
		}
		Declaration::VariableDeclaration(variable) => {
			let item_kind = variable_item_kind(variable.kind);
			for declarator in &variable.declarations {
				for identifier in declarator.id.get_binding_identifiers() {
					push_symbol(
						output,
						item_kind,
						module_prefix,
						identifier.name.to_string(),
						signature,
						file_path,
					);
				}
			}
		}
		Declaration::TSInterfaceDeclaration(interface) => {
			push_symbol(
				output,
				"interface",
				module_prefix,
				interface.id.name.to_string(),
				signature,
				file_path,
			);
		}
		Declaration::TSTypeAliasDeclaration(alias) => {
			push_symbol(
				output,
				"type_alias",
				module_prefix,
				alias.id.name.to_string(),
				signature,
				file_path,
			);
		}
		Declaration::TSEnumDeclaration(declaration) => {
			push_symbol(
				output,
				"enum",
				module_prefix,
				declaration.id.name.to_string(),
				signature,
				file_path,
			);
		}
		Declaration::TSModuleDeclaration(namespace) => {
			push_symbol(
				output,
				"namespace",
				module_prefix,
				ts_module_name(&namespace.id),
				signature,
				file_path,
			);
		}
		Declaration::TSGlobalDeclaration(_) | Declaration::TSImportEqualsDeclaration(_) => {}
	}
}

fn collect_public_symbols_from_default_export(
	declaration: &ExportDefaultDeclarationKind<'_>,
	module_prefix: &[String],
	file_path: &Path,
	signature: &str,
	output: &mut Vec<ExportedSymbol>,
) {
	match declaration {
		ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
			if let Some(identifier) = &function.id {
				push_symbol(
					output,
					"function",
					module_prefix,
					identifier.name.to_string(),
					signature,
					file_path,
				);
			} else {
				push_symbol(
					output,
					"default_export",
					module_prefix,
					"default",
					signature,
					file_path,
				);
			}
		}
		ExportDefaultDeclarationKind::ClassDeclaration(class) => {
			if let Some(identifier) = &class.id {
				push_symbol(
					output,
					"class",
					module_prefix,
					identifier.name.to_string(),
					signature,
					file_path,
				);
			} else {
				push_symbol(
					output,
					"default_export",
					module_prefix,
					"default",
					signature,
					file_path,
				);
			}
		}
		ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
			push_symbol(
				output,
				"interface",
				module_prefix,
				interface.id.name.to_string(),
				signature,
				file_path,
			);
		}
		_ => {
			push_symbol(
				output,
				"default_export",
				module_prefix,
				"default",
				signature,
				file_path,
			);
		}
	}
}

fn module_export_name(name: &ModuleExportName<'_>) -> String {
	name.to_string()
}

fn ts_module_name(name: &TSModuleDeclarationName<'_>) -> String {
	name.to_string()
}

fn variable_item_kind(kind: oxc_ast::ast::VariableDeclarationKind) -> &'static str {
	if matches!(kind, oxc_ast::ast::VariableDeclarationKind::Const) {
		"constant"
	} else {
		"variable"
	}
}

fn collect_public_symbols_with_legacy_scanner(
	file: &PackageSnapshotFile,
	config: &EcmascriptExportConfig,
) -> Vec<ExportedSymbol> {
	let module_prefix = module_prefix_for_file(&file.path, config);
	let mut symbols = Vec::new();

	for raw_line in file.contents.lines() {
		let line = normalize_signature(raw_line);
		if !line.starts_with("export ") || line == "export {}" || line == "export {};" {
			continue;
		}

		if let Some(target) = parse_wildcard_reexport(&line) {
			push_symbol(
				&mut symbols,
				"wildcard_reexport",
				&module_prefix,
				target,
				&line,
				&file.path,
			);
			continue;
		}

		let named_exports = parse_named_exports(&line);
		if !named_exports.is_empty() {
			for (item_kind, item_name) in named_exports {
				push_symbol(
					&mut symbols,
					&item_kind,
					&module_prefix,
					item_name,
					&line,
					&file.path,
				);
			}
			continue;
		}

		if let Some((item_kind, item_name)) = parse_declaration_export(&line, config) {
			push_symbol(
				&mut symbols,
				item_kind,
				&module_prefix,
				item_name,
				&line,
				&file.path,
			);
		}
	}

	symbols
}

fn normalize_signature(line: &str) -> String {
	line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_wildcard_reexport(line: &str) -> Option<String> {
	let rest = line.strip_prefix("export ")?;
	let target = rest.strip_prefix("* from ")?;
	extract_quoted_text(target)
}

fn parse_named_exports(line: &str) -> Vec<(String, String)> {
	let Some(mut rest) = line.strip_prefix("export ") else {
		return Vec::new();
	};
	let mut item_kind = "reexport";
	if let Some(stripped) = rest.strip_prefix("type ") {
		rest = stripped;
		item_kind = "type_reexport";
	}
	if !rest.starts_with('{') {
		return Vec::new();
	}
	let brace_start = rest
		.find('{')
		.expect("named exports should include an opening brace after the prefix check");
	let Some(brace_end) = rest[brace_start + 1..].find('}') else {
		return Vec::new();
	};
	let inside = &rest[brace_start + 1..brace_start + 1 + brace_end];

	inside
		.split(',')
		.filter_map(|entry| {
			let entry = entry.trim();
			if entry.is_empty() {
				return None;
			}
			let exported_name = entry.split(" as ").last().unwrap_or(entry).trim();
			(!exported_name.is_empty()).then(|| (item_kind.to_string(), exported_name.to_string()))
		})
		.collect()
}

fn parse_declaration_export(
	line: &str,
	config: &EcmascriptExportConfig,
) -> Option<(&'static str, String)> {
	let mut rest = line.strip_prefix("export ")?;
	if config.legacy_supports_declare_prefix
		&& let Some(stripped) = rest.strip_prefix("declare ")
	{
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("default ") {
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("async ") {
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("abstract ") {
		rest = stripped;
	}

	for (prefix, item_kind) in [
		("function ", "function"),
		("class ", "class"),
		("const ", "constant"),
		("let ", "variable"),
		("var ", "variable"),
		("interface ", "interface"),
		("type ", "type_alias"),
		("enum ", "enum"),
	] {
		if let Some(stripped) = rest.strip_prefix(prefix)
			&& let Some(name) = take_identifier(stripped)
		{
			return Some((item_kind, name));
		}
	}

	if config.legacy_supports_namespace_exports
		&& let Some(stripped) = rest.strip_prefix("namespace ")
		&& let Some(name) = take_identifier(stripped)
	{
		return Some(("namespace", name));
	}

	Some(("default_export", "default".to_string())).filter(|_| line.starts_with("export default "))
}

fn take_identifier(text: &str) -> Option<String> {
	let identifier = text
		.chars()
		.take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
		.collect::<String>();
	(!identifier.is_empty()).then_some(identifier)
}

fn extract_quoted_text(text: &str) -> Option<String> {
	let quote = text
		.chars()
		.find(|character| matches!(character, '\'' | '"'))?;
	let start = text.find(quote)? + 1;
	let end = text[start..].find(quote)? + start;
	Some(text[start..end].to_string())
}

fn module_prefix_for_file(path: &Path, config: &EcmascriptExportConfig) -> Vec<String> {
	let mut components = path
		.parent()
		.map(|parent| {
			parent
				.components()
				.filter_map(|component| component.as_os_str().to_str())
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();

	if components
		.first()
		.is_some_and(|component| config.module_roots_to_strip.contains(&component.as_str()))
	{
		components.remove(0);
	}

	let stem = path
		.file_stem()
		.and_then(|stem| stem.to_str())
		.map_or_else(String::new, |stem| {
			if config.strip_declaration_stem_suffix {
				stem.strip_suffix(".d").unwrap_or(stem).to_string()
			} else {
				stem.to_string()
			}
		});
	if !stem.is_empty() && !config.ignored_module_stems.contains(&stem.as_str()) {
		components.push(stem);
	}

	components
}

fn push_symbol<S: Into<String>>(
	output: &mut Vec<ExportedSymbol>,
	item_kind: &str,
	module_prefix: &[String],
	item_name: S,
	signature: &str,
	file_path: &Path,
) {
	let item_name = item_name.into();
	let item_path = if module_prefix.is_empty() {
		item_name.clone()
	} else {
		format!("{}::{item_name}", module_prefix.join("::"))
	};

	output.push(ExportedSymbol {
		item_kind: item_kind.to_string(),
		item_path,
		signature: signature.to_string(),
		file_path: file_path.to_path_buf(),
	});
}

fn build_symbol_change(
	kind: SemanticChangeKind,
	symbol: &ExportedSymbol,
	before_signature: Option<String>,
	after_signature: Option<String>,
) -> SemanticChange {
	let verb = if kind == SemanticChangeKind::Added {
		"added"
	} else if kind == SemanticChangeKind::Removed {
		"removed"
	} else {
		"modified"
	};

	SemanticChange {
		category: SemanticChangeCategory::PublicApi,
		kind,
		item_kind: symbol.item_kind.clone(),
		item_path: symbol.item_path.clone(),
		summary: format!("{} `{}` {verb}", symbol.item_kind, symbol.item_path),
		file_path: symbol.file_path.clone(),
		before_signature,
		after_signature,
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
