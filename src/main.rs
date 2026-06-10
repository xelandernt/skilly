fn main() {
    let exit_code = _core::run_cli_entry(std::env::args().skip(1).collect());
    std::process::exit(exit_code);
}
