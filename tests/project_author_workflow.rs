#![cfg(all(
    feature = "project-workflow-cli",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

#[path = "support/project_author_workflow/mod.rs"]
mod workflow;

#[test]
#[ignore = "serial nested Cargo: two real external starters, first system, win/reset and relocated source-free Players"]
fn same_named_new_games_keep_their_identities_and_complete_source_and_cooked_workflows() {
    workflow::accept();
}

#[test]
#[ignore = "serial nested Cargo: generated collection project, missing Prefab metadata location and Check recovery"]
fn missing_prefab_metadata_reports_its_project_location_and_recovers() {
    workflow::missing_metadata_location();
}
