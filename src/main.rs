use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = smux::cli::Cli::parse();

    match smux::app::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
