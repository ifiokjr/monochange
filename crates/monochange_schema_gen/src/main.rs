use std::fs;
use std::path::PathBuf;

fn post_process(schema: &mut serde_json::Value, id: &str, title: &str) {
	if let Some(obj) = schema.as_object_mut() {
		obj.insert("$id".to_string(), serde_json::Value::String(id.to_string()));
		obj.insert(
			"title".to_string(),
			serde_json::Value::String(title.to_string()),
		);
	}
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let update_mode = args.get(1).map(|s| s.as_str()) == Some("update");

	let schemas_dir = PathBuf::from("crates/monochange_schema/schemas");
	let docs_schemas_dir = PathBuf::from("docs/src/schemas");

	// Release record schema
	let release_schema = schemars::schema_for!(monochange_core::ReleaseRecord);
	let mut release_value = release_schema.to_value();
	post_process(
		&mut release_value,
		"https://monochange.github.io/monochange/schemas/release-record.schema.json",
		"monochange release record",
	);

	// Config schema
	let config_schema = schemars::schema_for!(monochange_core::WorkspaceConfiguration);
	let mut config_value = config_schema.to_value();
	post_process(
		&mut config_value,
		"https://monochange.github.io/monochange/schemas/monochange.schema.json",
		"monochange configuration",
	);

	let release_json = serde_json::to_string_pretty(&release_value).unwrap();
	let config_json = serde_json::to_string_pretty(&config_value).unwrap();

	let release_path = schemas_dir.join("release-record.schema.json");
	let config_path = schemas_dir.join("monochange.schema.json");

	if update_mode {
		fs::create_dir_all(&schemas_dir).unwrap();
		fs::create_dir_all(&docs_schemas_dir).unwrap();

		fs::write(&release_path, &release_json).unwrap();
		fs::write(&config_path, &config_json).unwrap();

		// Also write to docs directory (unversioned aliases)
		fs::write(
			docs_schemas_dir.join("release-record.schema.json"),
			&release_json,
		)
		.unwrap();
		fs::write(
			docs_schemas_dir.join("monochange.schema.json"),
			&config_json,
		)
		.unwrap();

		println!("Schemas updated successfully.");
	} else {
		// Check mode: compare existing files
		let check_file = |path: &PathBuf, content: &str| {
			if path.exists() {
				let existing = fs::read_to_string(path).unwrap();
				if existing != content {
					eprintln!("Schema mismatch: {}", path.display());
					std::process::exit(1);
				}
			} else {
				eprintln!("Schema file missing: {}", path.display());
				std::process::exit(1);
			}
		};

		check_file(&release_path, &release_json);
		check_file(&config_path, &config_json);

		println!("Schemas are up to date.");
	}
}
