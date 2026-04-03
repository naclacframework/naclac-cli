use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "naclac")]
#[command(about = "A hyper-optimized native Solana framework generator", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initializes a new Naclac Workspace
    Init { name: String },
    /// Adds a new program to an existing Naclac workspace
    Add { name: String },
    /// Builds the SBF binary and generates the IDL JSON
    Build {
        #[arg(short, long)]
        generate_client: bool,
    },
    /// Generates a TypeScript SDK from the IDL
    Generate {
        program_id: Option<String>,
    },
    /// Deploys all programs in the workspace to the configured network
    Deploy,
    /// Runs the test suite defined in Naclac.toml
    Test {
        /// Optional specific test file to run (e.g., counter_test.test.ts)
        file: Option<String>,
    },
    /// Fetches and deserializes an on-chain account using the local IDL
    Account { address: String },
    /// Manages on-chain IDL (e.g., idl init)
    Idl {
        #[command(subcommand)]
        action: IdlAction,
    },
    /// Upgrades a deployed Naclac program and securely refunds buffer accounts
    Upgrade {
        program_id: Option<String>,
        #[arg(short, long)]
        filepath: Option<String>,
        #[arg(short, long)]
        buffer: Option<String>,
    },
    /// Verifies the determinism of local codebase against an on-chain program
    Verify { program_id: Option<String> },
}

#[derive(Subcommand)]
pub enum IdlAction {
    Init { program_id: Option<String> },
}

use std::fs;
use toml::Value;

fn get_target_programs(cli_arg: Option<&String>) -> Vec<String> {
    if let Some(pid) = cli_arg {
        return vec![pid.clone()];
    }

    let current_dir = std::env::current_dir().unwrap();
    let toml_path = if current_dir.join("Naclac.toml").exists() {
        current_dir.join("Naclac.toml")
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../../Naclac.toml").canonicalize().unwrap()
    } else {
        eprintln!("❌ Not a Naclac workspace.");
        std::process::exit(1);
    };

    let content = fs::read_to_string(&toml_path).unwrap_or_default();
    let parsed: Value = toml::from_str(&content).unwrap_or(Value::Table(Default::default()));

    let cluster = parsed
        .get("provider")
        .and_then(|p| p.get("cluster"))
        .and_then(|c| c.as_str())
        .unwrap_or("devnet");

    let mut programs = Vec::new();

    if let Some(programs_table) = parsed.get("programs").and_then(|p| p.get(cluster)).and_then(|c| c.as_table()) {
        for val in programs_table.values() {
            if let Some(pid) = val.as_str() {
                programs.push(pid.to_string());
            }
        }
    }

    if programs.is_empty() {
        eprintln!("❌ Error: No program IDs found in Naclac.toml for cluster '{}', and no ID was explicitly provided.", cluster);
        std::process::exit(1);
    }

    programs
}

fn main() {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Init { name } => commands::init::execute(name),
        Commands::Add { name } => commands::add::execute(name),
        Commands::Build { generate_client } => commands::build::execute(*generate_client),
        Commands::Generate { program_id } => commands::generate::execute(program_id.as_ref()),
        Commands::Deploy => commands::deploy::execute(),
        Commands::Test { file } => commands::test::execute(file.as_deref()),
        Commands::Account { address } => commands::account::execute(address),
        Commands::Idl { action } => match action {
            IdlAction::Init { program_id } => {
                let pids = get_target_programs(program_id.as_ref());
                for pid in pids {
                    commands::idl::execute(&IdlAction::Init { program_id: Some(pid) });
                }
            }
        },
        Commands::Upgrade { program_id, filepath, buffer } => {
            let pids = get_target_programs(program_id.as_ref());
            for pid in pids {
                commands::upgrade::execute(&pid, filepath.as_deref(), buffer.as_deref());
            }
        },
        Commands::Verify { program_id } => {
            let pids = get_target_programs(program_id.as_ref());
            for pid in pids {
                commands::verify::execute(&pid);
            }
        },
    }
}