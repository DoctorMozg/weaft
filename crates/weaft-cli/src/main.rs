//! weaft — compile one source project to host-specific agent skills and subagents.

// weaft-cli is the user-facing binary: stdout is its output surface and stderr its
// diagnostics channel. The workspace forbids direct printing in the library crates
// (weaft-core / weaft-targets); the CLI opts back in here.
#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "weaft-cli is the user-facing binary; stdout/stderr are its output channels"
)]

mod commands;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

/// weaft: a capability-aware compiler for agent skills and subagents.
#[derive(Parser)]
#[command(name = "weaft", version, about, long_about = None)]
struct Cli {
    /// Path to weaft.yaml (or a directory containing it).
    #[arg(long, global = true, default_value = "weaft.yaml")]
    manifest_path: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new weaft project.
    Init(commands::init::Args),
    /// Compile skills and subagents for one or all targets.
    Build(commands::build::Args),
    /// Run lint passes over the project.
    Lint(commands::lint::Args),
    /// Report token usage per target (counts are tokenizer approximations).
    Tokens(commands::tokens::Args),
    /// Render artifacts to stdout without writing files.
    Preview(commands::preview::Args),
    /// List supported targets and their capabilities.
    Targets,
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .without_time()
        .init();

    let cli = Cli::parse();
    let manifest = &cli.manifest_path;

    let result = match cli.command {
        Command::Init(args) => commands::init::run(&args),
        Command::Build(args) => commands::build::run(manifest, &args),
        Command::Lint(args) => commands::lint::run(manifest, &args),
        Command::Tokens(args) => commands::tokens::run(manifest, &args),
        Command::Preview(args) => commands::preview::run(manifest, &args),
        Command::Targets => commands::targets::run(),
    };

    match result {
        Ok(code) => code,
        Err(report) => {
            eprintln!("{report:?}");
            ExitCode::FAILURE
        },
    }
}
