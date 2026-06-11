//! Skilly: manage agent skills — create, install, scan, and update skills
//! for AI coding agents.

mod cli;
mod client;
mod core;

#[cfg(feature = "python-bindings")]
mod python;

/// Run the skilly CLI entry point with the given arguments.
///
/// Returns an exit code suitable for [`std::process::exit`].
pub fn run_cli_entry(args: Vec<String>) -> i32 {
    cli::run(args)
}
