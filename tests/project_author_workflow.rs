#![cfg(all(
    feature = "project-tooling",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

#[path = "support/authored_project.rs"]
mod authored_project;
#[path = "support/project_author_workflow/mod.rs"]
mod workflow;

#[test]
#[ignore = "serial nested Cargo: v2 starter, authored puzzle and input, cooked and relocated Player"]
fn generated_v2_puzzle_observes_the_edited_scene_and_input_after_source_removal() {
    workflow::puzzle::accept();
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "interactive Windows desktop with native first presentation and WGI startup"]
fn generated_v2_desktop_verifies_first_present() {
    workflow::puzzle::accept_native();
}
