use nara::{
    fs::DirectoryCapability,
    gameplay::GameplayCommandSubmission,
    prelude::Resource,
    project_host::{HeadlessRun, HeadlessRunIntent},
};

#[derive(Clone, Resource)]
struct ProductOutcome;

#[test]
fn headless_run_constructor_accepts_an_owned_command_buffer() {
    type Constructor = fn(
        DirectoryCapability,
        HeadlessRunIntent<ProductOutcome>,
        Vec<GameplayCommandSubmission>,
    ) -> HeadlessRun<ProductOutcome>;

    let _: Constructor = HeadlessRun::new;
}
