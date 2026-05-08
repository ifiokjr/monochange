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
	assert!(
		npm_symbols.iter().any(|symbol| {
			symbol.item_kind == "default_export" && symbol.item_path == "default"
		})
	);
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
		assert!(
			symbols
				.iter()
				.any(|symbol| { symbol.item_kind == "type_reexport" && symbol.item_path == "Bar" })
		);
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
	let anonymous_class_symbols = collect_public_symbols(&anonymous_default_class, &DENO_CONFIG);
	let interface_symbols = collect_public_symbols(&default_interface, &DENO_CONFIG);

	assert!(
		function_symbols
			.iter()
			.any(|symbol| { symbol.item_kind == "function" && symbol.item_path == "namedDefault" })
	);
	assert!(
		class_symbols
			.iter()
			.any(|symbol| { symbol.item_kind == "class" && symbol.item_path == "NamedGreeter" })
	);
	assert!(
		anonymous_class_symbols.iter().any(|symbol| {
			symbol.item_kind == "default_export" && symbol.item_path == "default"
		})
	);
	assert!(
		interface_symbols.iter().any(|symbol| {
			symbol.item_kind == "interface" && symbol.item_path == "RunnerContract"
		})
	);
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
	assert!(parse_declaration_export("export declare function run() {}", &DENO_CONFIG).is_none());
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
	assert!(
		symbols
			.iter()
			.any(|symbol| { symbol.item_kind == "reexport" && symbol.item_path == "exports::bar" })
	);
	assert!(
		symbols
			.iter()
			.any(|symbol| { symbol.item_kind == "reexport" && symbol.item_path == "exports::baz" })
	);
	assert!(
		symbols.iter().any(|symbol| {
			symbol.item_kind == "function" && symbol.item_path == "exports::load"
		})
	);
	assert!(
		symbols
			.iter()
			.any(|symbol| { symbol.item_kind == "class" && symbol.item_path == "exports::Base" })
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
