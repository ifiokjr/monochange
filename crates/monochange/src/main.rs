fn main() {
	if let Err(error) = monochange::run_from_env("monochange") {
		eprintln!("{error}");
		std::process::exit(1);
	}
}
