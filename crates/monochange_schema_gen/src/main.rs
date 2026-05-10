#[allow(dead_code)]
fn main() {
	let update_mode = std::env::args().nth(1).as_deref() == Some("update");
	monochange_schema_gen::run(update_mode);
}

#[test]
fn main_is_covered() {
	main();
}
