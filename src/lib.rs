mod cli;
mod client;
mod core;

#[cfg(feature = "python-bindings")]
mod python;

pub fn run_cli_entry(args: Vec<String>) -> i32 {
    cli::run(args)
}
