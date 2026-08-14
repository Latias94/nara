use std::process::ExitCode;

use nara::project_workflow::ProjectCreationOutcome;

mod args;
mod host_fs;
mod new;

const HELP: &str = "\
Nara project workflow

Usage:
  nara new desktop-2d/v1 <slug> --parent <directory> --local-checkout <directory>
  nara new desktop-2d/v1 <slug> --parent <directory> --git <https-url> --rev <object-id>
  nara --help
  nara --version
";

fn main() -> ExitCode {
    match args::parse(std::env::args_os().skip(1)) {
        Ok(args::Command::Help) => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Ok(args::Command::Version) => {
            println!("nara {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(args::Command::New(arguments)) => run_new(arguments),
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

fn run_new(arguments: args::NewArguments) -> ExitCode {
    match new::execute(arguments) {
        Ok(ProjectCreationOutcome::Created(receipt)) => {
            println!(
                "Created {} from {}.",
                receipt.plan().slug().as_str(),
                receipt.plan().starter().id()
            );
            ExitCode::SUCCESS
        }
        Ok(ProjectCreationOutcome::Rejected(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(ProjectCreationOutcome::Orphaned(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(ProjectCreationOutcome::Unresolved(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(ProjectCreationOutcome::PublishedButUnadmitted(outcome)) => {
            eprintln!(
                "error: project directory was published but ordinary admission failed: {}",
                outcome.error()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
