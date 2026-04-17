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
mod tests {
	use monochange_core::FileChangeKind;

	use super::*;

	const NPM_CONFIG: EcmascriptExportConfig = EcmascriptExportConfig {
		source_extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"],
		module_roots_to_strip: &["src", "lib"],
		ignored_module_stems: &["index"],
		strip_declaration_stem_suffix: true,
		legacy_supports_declare_prefix: true,
		legacy_supports_namespace_exports: true,
	};

	const DENO_CONFIG: EcmascriptExportConfig = EcmascriptExportConfig {
		source_extensions: &["js", "jsx", "ts", "tsx", "mjs", "mts"],
		module_roots_to_strip: &["src"],
		ignored_module_stems: &["index", "mod"],
		strip_declaration_stem_suffix: false,
		legacy_supports_declare_prefix: false,
		legacy_supports_namespace_exports: false,
	};

	#[test]
	fn module_prefix_for_nested_source_file_tracks_directory_components() {
		assert_eq!(
			module_prefix_for_file(Path::new("src/api/index.ts"), &NPM_CONFIG),
			vec!["api".to_string()]
		);
		assert_eq!(
			module_prefix_for_file(Path::new("src/utils/format.ts"), &NPM_CONFIG),
			vec!["utils".to_string(), "format".to_string()]
		);
		assert_eq!(
			module_prefix_for_file(Path::new("src/tools/index.ts"), &DENO_CONFIG),
			vec!["tools".to_string()]
		);
	}

	#[test]
	fn collect_public_symbols_finds_exported_declarations_and_reexports() {
		let npm_file = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: concat!(
				"export function greet(name: string): string { return name; }\n",
				"export const version = '1.0.0';\n",
				"export { formatGreeting as format } from './format';\n",
				"export * from './types';\n",
			)
			.to_string(),
		};
		let deno_file = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: concat!(
				"export function run() {}\n",
				"export { build } from './build.ts';\n",
				"export * from './types.ts';\n",
			)
			.to_string(),
		};

		let npm_symbols = collect_public_symbols(&npm_file, &NPM_CONFIG);
		let deno_symbols = collect_public_symbols(&deno_file, &DENO_CONFIG);

		assert!(npm_symbols.iter().any(|symbol| symbol.item_path == "greet"));
		assert!(
			npm_symbols
				.iter()
				.any(|symbol| symbol.item_path == "version")
		);
		assert!(
			npm_symbols
				.iter()
				.any(|symbol| symbol.item_path == "format")
		);
		assert!(npm_symbols.iter().any(|symbol| {
			symbol.item_kind == "wildcard_reexport" && symbol.item_path == "./types"
		}));
		assert!(deno_symbols.iter().any(|symbol| symbol.item_path == "run"));
		assert!(
			deno_symbols
				.iter()
				.any(|symbol| symbol.item_path == "build")
		);
		assert!(deno_symbols.iter().any(|symbol| {
			symbol.item_kind == "wildcard_reexport" && symbol.item_path == "./types.ts"
		}));
	}

	#[test]
	fn parser_backed_symbol_collection_handles_multiline_and_namespace_exports() {
		let npm_file = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: concat!(
				"export {\n",
				"  greet as renamedGreet,\n",
				"  version,\n",
				"} from './shared';\n",
				"export * as toolkit from './toolkit';\n",
				"export default function () { return 'ok'; }\n",
			)
			.to_string(),
		};
		let deno_file = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: concat!(
				"export {\n",
				"  run as renamedRun,\n",
				"  config,\n",
				"} from './shared.ts';\n",
				"export * as toolkit from './toolkit.ts';\n",
				"export default async function () { return 'ok'; }\n",
			)
			.to_string(),
		};

		let npm_symbols = collect_public_symbols(&npm_file, &NPM_CONFIG);
		let deno_symbols = collect_public_symbols(&deno_file, &DENO_CONFIG);

		assert!(
			npm_symbols
				.iter()
				.any(|symbol| symbol.item_path == "renamedGreet")
		);
		assert!(
			npm_symbols
				.iter()
				.any(|symbol| symbol.item_path == "version")
		);
		assert!(npm_symbols.iter().any(|symbol| {
			symbol.item_kind == "namespace_reexport" && symbol.item_path == "toolkit"
		}));
		assert!(npm_symbols.iter().any(|symbol| {
			symbol.item_kind == "default_export" && symbol.item_path == "default"
		}));
		assert!(
			deno_symbols
				.iter()
				.any(|symbol| symbol.item_path == "renamedRun")
		);
		assert!(
			deno_symbols
				.iter()
				.any(|symbol| symbol.item_path == "config")
		);
	}

	#[test]
	fn parser_backed_symbol_collection_handles_type_reexports_and_ts_declarations() {
		let npm_file = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: concat!(
				"const hidden = true;\n",
				"import { hiddenImport } from './hidden';\n",
				"export { type Foo as Bar } from './types';\n",
				"export interface Greeting {}\n",
				"export type GreetingValue = string;\n",
				"export enum Mode { Light }\n",
				"export namespace Toolkit {}\n",
				"export import Legacy = Toolkit;\n",
				"export default {};\n",
			)
			.to_string(),
		};
		let deno_file = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: concat!(
				"const hidden = true;\n",
				"import { hiddenImport } from './hidden.ts';\n",
				"export { type Foo as Bar } from './types.ts';\n",
				"export interface Greeting {}\n",
				"export type GreetingValue = string;\n",
				"export enum Mode { Light }\n",
				"export namespace Toolkit {}\n",
				"export import Legacy = Toolkit;\n",
				"export default {};\n",
			)
			.to_string(),
		};

		let npm_symbols = collect_public_symbols(&npm_file, &NPM_CONFIG);
		let deno_symbols = collect_public_symbols(&deno_file, &DENO_CONFIG);

		for symbols in [&npm_symbols, &deno_symbols] {
			assert!(symbols.iter().any(|symbol| {
				symbol.item_kind == "type_reexport" && symbol.item_path == "Bar"
			}));
			assert!(symbols.iter().any(|symbol| symbol.item_path == "Greeting"));
			assert!(
				symbols
					.iter()
					.any(|symbol| symbol.item_path == "GreetingValue")
			);
			assert!(symbols.iter().any(|symbol| symbol.item_path == "Mode"));
			assert!(symbols.iter().any(|symbol| symbol.item_path == "Toolkit"));
			assert!(symbols.iter().any(|symbol| {
				symbol.item_kind == "default_export" && symbol.item_path == "default"
			}));
			assert!(!symbols.iter().any(|symbol| symbol.item_path == "hidden"));
			assert!(!symbols.iter().any(|symbol| symbol.item_path == "Legacy"));
		}
	}

	#[test]
	fn parser_backed_symbol_collection_handles_named_default_exports() {
		let default_function = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: "export default function namedDefault() { return 'ok'; }\n".to_string(),
		};
		let default_class = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: "export default class NamedGreeter {}\n".to_string(),
		};
		let anonymous_default_class = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: "export default class {}\n".to_string(),
		};
		let default_interface = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: "export default interface RunnerContract {}\n".to_string(),
		};

		let function_symbols = collect_public_symbols(&default_function, &NPM_CONFIG);
		let class_symbols = collect_public_symbols(&default_class, &NPM_CONFIG);
		let anonymous_class_symbols =
			collect_public_symbols(&anonymous_default_class, &DENO_CONFIG);
		let interface_symbols = collect_public_symbols(&default_interface, &DENO_CONFIG);

		assert!(function_symbols.iter().any(|symbol| {
			symbol.item_kind == "function" && symbol.item_path == "namedDefault"
		}));
		assert!(
			class_symbols.iter().any(|symbol| {
				symbol.item_kind == "class" && symbol.item_path == "NamedGreeter"
			})
		);
		assert!(anonymous_class_symbols.iter().any(|symbol| {
			symbol.item_kind == "default_export" && symbol.item_path == "default"
		}));
		assert!(interface_symbols.iter().any(|symbol| {
			symbol.item_kind == "interface" && symbol.item_path == "RunnerContract"
		}));
	}

	#[test]
	fn variable_item_kind_maps_non_const_bindings_to_variable() {
		assert_eq!(
			variable_item_kind(oxc_ast::ast::VariableDeclarationKind::Let),
			"variable"
		);
	}

	#[test]
	fn legacy_scanner_collects_single_line_exports_and_fallback_uses_it() {
		let npm_file = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: "export const version = '1.0.0';\n".to_string(),
		};
		let fallback_file = PackageSnapshotFile {
			path: PathBuf::from("exports.unknown"),
			contents: "export const fallback = true;\n".to_string(),
		};

		let legacy_symbols = collect_public_symbols_with_legacy_scanner(&npm_file, &NPM_CONFIG);
		let fallback_symbols = collect_public_symbols(&fallback_file, &NPM_CONFIG);

		assert!(
			legacy_symbols
				.iter()
				.any(|symbol| symbol.item_path == "version")
		);
		assert!(
			fallback_symbols
				.iter()
				.any(|symbol| symbol.item_path == "exports::fallback")
		);
	}

	#[test]
	fn snapshot_and_symbol_helpers_cover_additional_ecmascript_branches() {
		let npm_changed_files = vec![
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/src/index.ts"),
				package_path: PathBuf::from("src/index.ts"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some(
					concat!(
						"export declare async function greet(name: string): Promise<string> { return name; }\n",
						"export default class Greeter {}\n",
						"export abstract class BaseGreeter {}\n",
						"export namespace Tools {}\n",
						"export type { Foo as Bar } from './types';\n",
					)
					.to_string(),
				),
			},
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/README.md"),
				package_path: PathBuf::from("README.md"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some("ignored".to_string()),
			},
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/src/legacy.ts"),
				package_path: PathBuf::from("src/legacy.ts"),
				kind: FileChangeKind::Modified,
				before_contents: Some("export const previous = true;".to_string()),
				after_contents: None,
			},
		];
		let deno_changed_files = vec![
			AnalyzedFileChange {
				path: PathBuf::from("mod.ts"),
				package_path: PathBuf::from("mod.ts"),
				kind: FileChangeKind::Modified,
				before_contents: Some("export const previous = true;".to_string()),
				after_contents: None,
			},
			AnalyzedFileChange {
				path: PathBuf::from("cli.mts"),
				package_path: PathBuf::from("cli.mts"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some(
					concat!(
						"export default async function run() {}\n",
						"export abstract class Runner {}\n",
						"export { build } from './build.ts';\n",
					)
					.to_string(),
				),
			},
		];

		let npm_symbols = snapshot_exported_symbols(None, &npm_changed_files, &NPM_CONFIG);
		let deno_symbols = snapshot_exported_symbols(None, &deno_changed_files, &DENO_CONFIG);

		for expected in [
			"greet",
			"Greeter",
			"BaseGreeter",
			"Tools",
			"Bar",
			"legacy::previous",
		] {
			assert!(
				npm_symbols
					.iter()
					.any(|((_, item_path), _)| item_path == expected)
			);
		}
		for expected in ["previous", "cli::run", "cli::Runner", "cli::build"] {
			assert!(
				deno_symbols
					.iter()
					.any(|((_, item_path), _)| item_path == expected)
			);
		}

		assert!(is_source_file(Path::new("src/index.cts"), &NPM_CONFIG));
		assert!(is_source_file(Path::new("cli.mts"), &DENO_CONFIG));
		assert!(!is_source_file(Path::new("build/index.ts"), &NPM_CONFIG));
		assert!(!is_source_file(Path::new("build/mod.ts"), &DENO_CONFIG));
		assert!(!is_source_file(Path::new("src/index"), &NPM_CONFIG));
		assert!(!is_source_file(Path::new("mod"), &DENO_CONFIG));
		assert_eq!(
			module_prefix_for_file(Path::new("lib/utils/index.d.ts"), &NPM_CONFIG),
			vec!["utils".to_string()]
		);
		assert_eq!(
			module_prefix_for_file(Path::new("mod.ts"), &DENO_CONFIG),
			Vec::<String>::new()
		);
		assert_eq!(
			module_prefix_for_file(Path::new("src/mod.ts"), &DENO_CONFIG),
			Vec::<String>::new()
		);

		assert!(parse_named_exports("const nope = true;").is_empty());
		assert_eq!(
			parse_named_exports("export type { Foo as Bar, , Baz } from './types';"),
			vec![
				("type_reexport".to_string(), "Bar".to_string()),
				("type_reexport".to_string(), "Baz".to_string()),
			]
		);
		assert!(parse_named_exports("export { Foo as Bar from './types';").is_empty());
		assert_eq!(
			parse_declaration_export("export default {};", &NPM_CONFIG),
			Some(("default_export", "default".to_string()))
		);
		assert_eq!(
			parse_declaration_export("export declare function greet() {}", &NPM_CONFIG),
			Some(("function", "greet".to_string()))
		);
		assert_eq!(
			parse_declaration_export("export namespace Toolkit {}", &NPM_CONFIG),
			Some(("namespace", "Toolkit".to_string()))
		);
		assert!(
			parse_declaration_export("export declare function run() {}", &DENO_CONFIG).is_none()
		);
		assert!(parse_declaration_export("export namespace Toolkit {}", &DENO_CONFIG).is_none());
		assert_eq!(
			extract_quoted_text("from './toolkit'"),
			Some("./toolkit".to_string())
		);
	}

	#[test]
	fn legacy_scanner_and_snapshot_helpers_cover_remaining_patch_lines() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("exports.unknown"),
			contents: concat!(
				"const hidden = true;\n",
				"export * from './types';\n",
				"export { foo as bar, baz } from './shared';\n",
				"export async function load() {}\n",
				"export abstract class Base {}\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols_with_legacy_scanner(&file, &NPM_CONFIG);
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "wildcard_reexport" && symbol.item_path == "exports::./types"
		}));
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "reexport" && symbol.item_path == "exports::bar"
		}));
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "reexport" && symbol.item_path == "exports::baz"
		}));
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "function" && symbol.item_path == "exports::load"
		}));
		assert!(
			symbols.iter().any(|symbol| {
				symbol.item_kind == "class" && symbol.item_path == "exports::Base"
			})
		);

		let skipped_symbols = snapshot_exported_symbols(
			None,
			&[
				AnalyzedFileChange {
					path: PathBuf::from("src/empty.ts"),
					package_path: PathBuf::from("src/empty.ts"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: None,
				},
				AnalyzedFileChange {
					path: PathBuf::from("README.md"),
					package_path: PathBuf::from("README.md"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: Some("ignored".to_string()),
				},
			],
			&NPM_CONFIG,
		);
		assert!(skipped_symbols.is_empty());

		let unchanged_before = BTreeMap::from([(
			("function".to_string(), "same".to_string()),
			ExportedSymbol {
				item_kind: "function".to_string(),
				item_path: "same".to_string(),
				signature: "export function same() {}".to_string(),
				file_path: PathBuf::from("src/index.ts"),
			},
		)]);
		let unchanged_after = unchanged_before.clone();
		assert!(diff_public_symbols(&unchanged_before, &unchanged_after).is_empty());

		assert_eq!(
			parse_wildcard_reexport("export * from './types';"),
			Some("./types".to_string())
		);
		assert_eq!(
			parse_declaration_export("export async function load() {}", &NPM_CONFIG),
			Some(("function", "load".to_string()))
		);
		assert_eq!(
			parse_declaration_export("export abstract class Base {}", &NPM_CONFIG),
			Some(("class", "Base".to_string()))
		);
	}

	#[test]
	fn diff_public_symbols_reports_added_modified_and_removed_entries() {
		let before = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				ExportedSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "export function greet() {}".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
			(
				("class".to_string(), "Greeter".to_string()),
				ExportedSymbol {
					item_kind: "class".to_string(),
					item_path: "Greeter".to_string(),
					signature: "export class Greeter {}".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
		]);
		let after = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				ExportedSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "export function greet(name: string) {}".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
			(
				("constant".to_string(), "version".to_string()),
				ExportedSymbol {
					item_kind: "constant".to_string(),
					item_path: "version".to_string(),
					signature: "export const version = '1.0.0'".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
		]);

		let changes = diff_public_symbols(&before, &after);

		assert!(changes.iter().any(|change| {
			change.kind == SemanticChangeKind::Added && change.item_path == "version"
		}));
		assert!(changes.iter().any(|change| {
			change.kind == SemanticChangeKind::Modified && change.item_path == "greet"
		}));
		assert!(changes.iter().any(|change| {
			change.kind == SemanticChangeKind::Removed && change.item_path == "Greeter"
		}));
	}
}
