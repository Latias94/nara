#![cfg(all(
    feature = "project-workflow-cli",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

#[path = "support/project_content_fixture.rs"]
mod content_fixture;
#[path = "support/project_cook/mod.rs"]
mod project_cook;

#[test]
#[ignore = "runs serial offline Rust 1.95.0 nested Cargo, generated-project Cook and a relocated source-free Player"]
fn generated_project_cooks_custom_game_and_runs_after_relocation() {
    project_cook::accept();
}

#[test]
#[ignore = "actual contained child and unwind cleanup; run serially"]
fn live_cook_consumer_timeout_preserves_and_retires_the_original_child() {
    project_cook::accept_timeout_cleanup();
}

#[test]
#[ignore = "private waiting child for the original process cleanup control"]
fn project_cook_waiting_child() {
    use std::io::Write;

    assert_eq!(
        std::env::var("NARA_PROJECT_COOK_WAITING_CHILD").as_deref(),
        Ok("1")
    );
    println!("PROJECT_COOK_WAITING_CHILD");
    std::io::stdout().flush().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(60));
}
