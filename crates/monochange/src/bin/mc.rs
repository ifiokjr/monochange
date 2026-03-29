fn main() {
	if let Err(error) = monochange::run_from_env("mc") {
		eprintln!("{}", error.render());
		std::process::exit(1);
	}
}
