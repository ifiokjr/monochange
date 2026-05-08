use super::*;

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
	match payload.downcast::<String>() {
		Ok(message) => *message,
		Err(payload) => {
			match payload.downcast::<&'static str>() {
				Ok(message) => (*message).to_string(),
				Err(_) => "non-string panic payload".to_string(),
			}
		}
	}
}

#[test]
fn copy_directory_reports_copy_failures() {
	let source = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let source_file = source.path().join("file.txt");
	let destination_conflict = destination.path().join("file.txt");

	fs::write(&source_file, "hello")
		.unwrap_or_else(|error| panic!("write source file {}: {error}", source_file.display()));
	fs::create_dir_all(&destination_conflict).unwrap_or_else(|error| {
		panic!(
			"create destination conflict {}: {error}",
			destination_conflict.display()
		)
	});

	let panic = std::panic::catch_unwind(|| copy_directory(source.path(), destination.path()))
		.err()
		.unwrap_or_else(|| panic!("expected copy failure panic"));
	let message = panic_message(panic);

	assert!(message.contains("copy"), "panic message: {message}");
	assert!(message.contains("file.txt"), "panic message: {message}");
}
