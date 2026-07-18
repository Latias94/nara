use std::num::NonZeroU32;

use nara::{
    diagnostic::DiagnosticReport,
    fs::DirectoryCapability,
    gameplay::GameplayCommandSubmission,
    prelude::Resource,
    project_host::{HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport},
};

#[derive(Clone, Resource)]
struct ProductOutcome;

fn run_project(
    project: DirectoryCapability,
    command: GameplayCommandSubmission,
) -> HeadlessRunReport<ProductOutcome> {
    let intent = HeadlessRunIntent::new(NonZeroU32::new(1).unwrap());
    let mut run = HeadlessRun::new(project, intent, [command]);
    run.execute_bounded()
}

fn observe(report: &HeadlessRunReport<ProductOutcome>) {
    let diagnostics: &DiagnosticReport = report.diagnostics();
    let _has_errors = diagnostics.has_errors();
    match report.outcome() {
        HeadlessRunOutcome::Completed(_) => {}
        HeadlessRunOutcome::Failed | HeadlessRunOutcome::CleanupIncomplete => {}
    }
}

fn main() {
    let _ = run_project;
    let _ = observe;
}
