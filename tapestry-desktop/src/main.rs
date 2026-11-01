//! `tapestry` — the desktop CLI binary. Thin shell over tapestry_desktop.

fn main() {
    let cli = tapestry_desktop::cli::Cli::parse();
    let code = tapestry_desktop::cli::run(&cli);
    std::process::exit(code);
}

use clap::Parser as _;
