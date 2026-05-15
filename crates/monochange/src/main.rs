#![allow(unstable_features)]
#![allow(clippy::large_futures)]
#![feature(coverage_attribute)]

#[coverage(off)]
#[allow(clippy::disallowed_methods)]
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
	monochange::run_cli_binary_from_env("monochange").await
}
