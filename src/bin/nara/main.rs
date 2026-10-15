use std::process::ExitCode;

use nara::project_tooling::{ProjectToolSummary, host_fs};
use nara::project_workflow::ProjectCreationOutcome;

mod args;
mod check;
mod export;
mod new;
mod repair;

const HELP: &str = "\
Nara project workflow

Usage:
  nara check <project-directory> [--profile <name>]
  nara new desktop-2d/v1 <slug> --parent <directory> --local-checkout <directory>
  nara new desktop-2d/v1 <slug> --parent <directory> --git <https-url> --rev <object-id>
  nara export <project-directory> --parent <directory> --destination <name>
  nara repair startup-scene <project-directory> [--profile <name>] --replacement <scene-path>
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
        Ok(args::Command::Check(arguments)) => run_check(arguments),
        Ok(args::Command::New(arguments)) => run_new(arguments),
        Ok(args::Command::Export(arguments)) => run_export(arguments),
        Ok(args::Command::Repair(arguments)) => run_repair(arguments),
        Err(error) => {
            eprintln!("error: {error}\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

fn run_check(arguments: args::CheckArguments) -> ExitCode {
    match check::execute(arguments) {
        Ok(ProjectToolSummary::Checked { startup_members }) => {
            println!(
                "Project check passed with {} startup-closure members.",
                startup_members
            );
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_repair(arguments: args::RepairArguments) -> ExitCode {
    match repair::execute(arguments) {
        Ok(ProjectToolSummary::Repaired { replacement }) => {
            println!("Repaired startup Scene selection to {}.", replacement);
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_export(arguments: args::ExportArguments) -> ExitCode {
    match export::execute(arguments) {
        Ok(ProjectToolSummary::Exported {
            startup_members,
            product_observations,
        }) => {
            println!(
                "Exported and accepted a candidate with {} startup-closure members through {} exact product observations.",
                startup_members, product_observations
            );
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::FAILURE,
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
