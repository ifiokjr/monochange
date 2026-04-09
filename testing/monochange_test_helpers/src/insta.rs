pub fn snapshot_settings() -> insta::Settings {
	let mut settings = insta::Settings::clone_current();
	settings.add_filter(r"/private/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/private/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/tmp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"/home/runner/work/_temp/[^/\s]+", "[ROOT]");
	settings.add_filter(r"\b[A-Z]:\\[^\s]+?\\Temp\\[^\\\s]+", "[ROOT]");
	settings.add_filter(r"SourceOffset\(\d+\)", "SourceOffset([OFFSET])");
	settings.add_filter(r"length: \d+", "length: [LEN]");
	settings.add_filter(r"@ bytes \d+\.\.\d+", "@ bytes [OFFSET]..[END]");
	settings.add_filter(r"\b[0-9a-f]{7,40}\b", "[SHA]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", "[DATETIME]");
	settings.add_filter(r"\d{4}-\d{2}-\d{2}", "[DATE]");
	settings
}
