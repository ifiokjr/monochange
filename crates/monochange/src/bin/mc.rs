fn main() {
	if let Err(error) = monochange::run_from_env("mc") {
		eprintln!("{error}");
		std::process::exit(1);
	}
}
