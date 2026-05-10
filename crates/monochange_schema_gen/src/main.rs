#![feature(coverage_attribute)]

#[coverage(off)]
fn main() {
	if let Err(error) = monochange_schema_gen::run_cli(std::env::args().nth(1).as_deref()) {
		eprintln!("{error}");
		std::process::exit(1);
	}
}
