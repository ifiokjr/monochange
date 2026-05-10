#![feature(coverage_attribute)]

use clap::Parser;
use clap::Subcommand;

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	/// Regenerate committed JSON Schema assets
	Schema(SchemaArgs),
}

#[derive(Parser)]
struct SchemaArgs {
	#[command(subcommand)]
	command: SchemaCommands,
}

#[derive(Subcommand)]
enum SchemaCommands {
	/// Write (update) schema files to disk
	Update,
	/// Check committed schema files are up to date
	Check,
}

#[coverage(off)]
fn main() {
	let cli = Cli::parse();
	let result = match cli.command {
		Commands::Schema(args) => {
			match args.command {
				SchemaCommands::Update => xtask::run(true),
				SchemaCommands::Check => xtask::run(false),
			}
		}
	};
	if let Err(msg) = result {
		eprintln!("{msg}");
		std::process::exit(1);
	}
}
