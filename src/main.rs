use std::process::ExitCode;

fn main() -> ExitCode {
    match rankewd::cli::run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rankewd: {error}");
            ExitCode::FAILURE
        }
    }
}
