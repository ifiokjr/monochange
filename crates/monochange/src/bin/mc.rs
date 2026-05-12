#![allow(clippy::large_futures)]
#[allow(clippy::disallowed_methods)]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
	let quiet = std::env::args_os().any(|arg| matches!(arg.to_str(), Some("--quiet" | "-q")));

	let result = monochange::run_from_env("mc").await;
	let Err(error) = result else {
		return;
	};

	if !quiet {
		eprintln!("{}", error.render());
	}

	std::process::exit(1);
}
