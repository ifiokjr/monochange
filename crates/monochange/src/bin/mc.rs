fn main() {
	let quiet = std::env::args_os().any(|arg| matches!(arg.to_str(), Some("--quiet" | "-q")));

	let result = monochange::run_from_env("mc");
	let Err(error) = result else {
		return;
	};

	if !quiet {
		eprintln!("{}", error.render());
	}

	std::process::exit(1);
}
