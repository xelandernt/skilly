//! Skilly CLI binary — delegates to the library crate via shared modules.

mod cli;
mod client;
mod config;
mod core;

fn main() {
    let exit_code = cli::run(std::env::args().skip(1).collect());
    std::process::exit(exit_code);
}
