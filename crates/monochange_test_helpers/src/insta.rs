pub fn snapshot_settings() -> insta::Settings {
	let mut settings = insta::Settings::clone_current();

	// Path filters - normalize temporary directories across platforms
	settings.add_filter(r"/private/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/private/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/home/runner/work/_temp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"\b[A-Z]:\\[^\s]+?\\Temp\\[^\\\s]+", "[ROOT]");

	// Position filters
	settings.add_filter(r"near position \d+", "near position [POS]");
	settings.add_filter(r"SourceOffset\(\d+\)", "SourceOffset([OFFSET])");
	settings.add_filter(r"length: \d+", "length: [LEN]");
	settings.add_filter(r"@ bytes \d+\.\.\d+", "@ bytes [OFFSET]..[END]");

	// Hash and date filters
	settings.add_filter(r"\b[0-9a-f]{7,40}\b", "[SHA]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", "[DATETIME]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}", "[DATE]");

	// Release-record schema version filters — redact wire-format fields and diagnostic
	// text so snapshots assert behavior instead of the schema crate version produced by
	// the current release PR. This keeps schema-version bumps upgrade-compatible.
	settings.add_filter(
		r#""schemaVersion": "\d+\.\d+""#,
		r#""schemaVersion": "[SCHEMA_VERSION]""#,
	);
	settings.add_filter(
		r"schema version `\d+\.\d+(?:\.\d+)?`",
		"schema version `[SCHEMA_VERSION]`",
	);
	settings.add_filter(
		r"supported version is `\d+\.\d+`",
		"supported version is `[SCHEMA_VERSION]`",
	);
	settings.add_filter(r"schemaVersion \d+\.\d+", "schemaVersion [SCHEMA_VERSION]");

	settings
}
