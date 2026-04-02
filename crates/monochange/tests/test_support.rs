use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub fn fixture_root(manifest_dir: &str, relative_fixture_root: &str) -> PathBuf {
	Path::new(manifest_dir).join(relative_fixture_root)
}

pub fn copy_directory(source: &Path, destination: &Path) {
	fs::create_dir_all(destination)
		.unwrap_or_else(|error| panic!("create destination {}: {error}", destination.display()));
	for entry in fs::read_dir(source)
		.unwrap_or_else(|error| panic!("read dir {}: {error}", source.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
		let source_path = entry.path();
		let destination_path = destination.join(entry.file_name());
		let file_type = entry
			.file_type()
			.unwrap_or_else(|error| panic!("file type {}: {error}", source_path.display()));
		if file_type.is_dir() {
			copy_directory(&source_path, &destination_path);
		} else if file_type.is_file() {
			if let Some(parent) = destination_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
				panic!(
					"copy {} -> {}: {error}",
					source_path.display(),
					destination_path.display()
				)
			});
		}
	}
}
