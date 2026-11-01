//! The `tapestry` command surface (directive §22). Exit codes are stable.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Stable exit codes: agents depend on these, never on terminal scraping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Ok = 0,
    Usage = 2,
    Auth = 3,
    NoDevice = 4,
    ReviewRejected = 5,
    Timeout = 6,
}

#[derive(Parser, Debug)]
#[command(
    name = "tapestry",
    version,
    about = "Tapestry — distributed presentation and human-inspection harness"
)]
pub struct Cli {
    /// Machine-readable request file instead of a prompt (with --machine).
    /// If a subcommand is also given, the subcommand wins.
    #[arg(long, value_name = "FILE")]
    pub request: Option<PathBuf>,

    /// Print only machine-readable JSON output.
    #[arg(long, requires = "request")]
    pub machine: bool,

    /// Bare positional prompt: `tapestry "prompt"` (§22 primary interaction).
    #[arg(value_name = "PROMPT", conflicts_with_all = ["request", "machine"])]
    pub prompt_text: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run a prompt through the full drive: inference → artefact → review.
    Prompt {
        /// The prompt text (quoted).
        text: String,
    },
    /// Stage a local file as an artefact.
    Send {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Present a single artefact for review.
    Show {
        #[arg(value_name = "ARTIFACT")]
        artifact: String,
    },
    /// Present artefacts side by side for comparison.
    Compare {
        #[arg(value_name = "A")]
        a: String,
        #[arg(value_name = "B")]
        b: String,
    },
    /// Show the current presentation target.
    Target,
    /// List discovered devices.
    Devices,
    /// Tactical snapshot of the current drive.
    Status,
    /// List sessions known to this coordinator.
    Sessions,
    /// Unified storage visibility (§16).
    Storage,
    /// Create an ephemeral share link for the latest artefact.
    Share,
    /// Diagnostics: credentials, transport, knowledge journal, resources.
    Doctor,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// Dispatch a parsed CLI. Returns the process exit code.
pub fn run(cli: &Cli) -> i32 {
    if let Some(text) = &cli.prompt_text {
        return crate::slice::run_prompt(text, cli.machine);
    }
    match &cli.command {
        Some(Command::Prompt { text }) => crate::slice::run_prompt(text, cli.machine),
        Some(Command::Send { file }) => crate::slice::send_file(file, cli.machine),
        Some(Command::Show { artifact }) => crate::slice::show(artifact),
        Some(Command::Compare { a, b }) => crate::slice::compare(a, b),
        Some(Command::Target) => crate::slice::target(),
        Some(Command::Devices) => crate::slice::devices(),
        Some(Command::Status) => crate::slice::status(),
        Some(Command::Sessions) => crate::slice::sessions(),
        Some(Command::Storage) => crate::slice::storage(),
        Some(Command::Share) => crate::slice::share(),
        Some(Command::Doctor) => crate::slice::doctor(),
        None if cli.request.is_some() => {
            crate::slice::machine_request(cli.request.as_ref().expect("checked above"))
        }
        None => {
            eprintln!("usage: tapestry \"prompt\" | tapestry <command> | tapestry --request f.json --machine");
            ExitCode::Usage.as_i32()
        }
    }
}
