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
	/// Check or update skill documentation inventories
	Skill(SkillArgs),
}

#[derive(Parser)]
struct SchemaArgs {
	#[command(subcommand)]
	command: SchemaCommands,
}

#[derive(Parser)]
struct SkillArgs {
	#[command(subcommand)]
	command: SkillCommands,
}

#[derive(Subcommand)]
enum SkillCommands {
	/// Check or update packages/monochange__skill/skills/commands.md
	Commands(SkillCommandsArgs),
}

#[derive(Parser)]
struct SkillCommandsArgs {
	#[command(subcommand)]
	command: SkillCommandActions,
}

#[derive(Subcommand)]
enum SkillCommandActions {
	/// Update the committed command inventory
	Update,
	/// Check the committed command inventory
	Check,
}

#[derive(Parser)]
struct SchemaReleaseArgs {
	#[command(subcommand)]
	command: SchemaReleaseCommands,
}

#[derive(Subcommand)]
enum SchemaCommands {
	/// Write (update) current schema files to disk
	Update,
	/// Check committed current schema files are up to date
	Check,
	/// Generate or check release schema files, including versioned assets
	Release(SchemaReleaseArgs),
}

#[derive(Subcommand)]
enum SchemaReleaseCommands {
	/// Write (update) release schema files to disk
	Update {
		/// Include immutable versioned schema files and artifact fixtures.
		#[arg(long)]
		versioned: bool,
	},
	/// Check committed release schema files are up to date
	Check {
		/// Include immutable versioned schema files and artifact fixtures.
		#[arg(long)]
		versioned: bool,
	},
}

#[coverage(off)]
fn main() {
	let cli = Cli::parse();
	let result = match cli.command {
		Commands::Schema(args) => {
			match args.command {
				SchemaCommands::Update => xtask::run(true),
				SchemaCommands::Check => xtask::run(false),
				SchemaCommands::Release(release_args) => {
					match release_args.command {
						SchemaReleaseCommands::Update { versioned } => {
							xtask::run_release(true, versioned)
						}
						SchemaReleaseCommands::Check { versioned } => {
							xtask::run_release(false, versioned)
						}
					}
				}
			}
		}
		Commands::Skill(args) => {
			match args.command {
				SkillCommands::Commands(commands_args) => {
					match commands_args.command {
						SkillCommandActions::Update => xtask::run_skill_commands(true),
						SkillCommandActions::Check => xtask::run_skill_commands(false),
					}
				}
			}
		}
	};
	if let Err(msg) = result {
		eprintln!("{msg}");
		std::process::exit(1);
	}
}
