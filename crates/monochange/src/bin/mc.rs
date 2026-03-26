use clap::Parser;

#[derive(Parser, Debug)]
#[command(
	name = "mc",
	bin_name = "mc",
	author,
	version,
	about = "Manage versions and releases for your multiplatform, multilanguage monorepo"
)]
struct Cli;

fn main() {
	let _cli = Cli::parse();
	println!("monochange is not implemented yet.");
}
