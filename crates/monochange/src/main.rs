#![allow(clippy::large_futures)]
#[allow(clippy::disallowed_methods)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
	let quiet = std::env::args_os().any(|arg| matches!(arg.to_str(), Some("--quiet" | "-q")));

	let Err(error) = monochange::run_from_env("monochange").await else {
		return;
	};

	if !quiet {
		eprintln!("{}", error.render());
	}

	std::process::exit(1);
}
