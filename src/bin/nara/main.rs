use std::process::ExitCode;

use nara::project_workflow::ProjectCreationOutcome;

mod args;
mod export;
mod host_fs;
mod new;

const HELP: &str = "\
Nara project workflow

Usage:
  nara new desktop-2d/v1 <slug> --parent <directory> --local-checkout <directory>
  nara new desktop-2d/v1 <slug> --parent <directory> --git <https-url> --rev <object-id>
  nara export <project-directory> --parent <directory> --destination <name>
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
        Ok(args::Command::Export(arguments)) => run_export(arguments),
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

fn run_export(arguments: args::ExportArguments) -> ExitCode {
    match export::execute(arguments) {
        Ok(export::ExportCommandOutcome::Accepted(receipt)) => {
            println!(
                "Exported and accepted a candidate with {} startup-closure members.",
                receipt.plan().members().len()
            );
            ExitCode::SUCCESS
        }
        Ok(export::ExportCommandOutcome::Rejected(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(export::ExportCommandOutcome::Orphaned(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(export::ExportCommandOutcome::Unresolved(error)) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
        Ok(export::ExportCommandOutcome::PublishedButUnaccepted { receipt, error }) => {
            eprintln!(
                "error: candidate with {} startup-closure members was published but acceptance failed: {error}",
                receipt.plan().members().len()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
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
