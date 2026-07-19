use std::num::NonZeroU32;

use nara::{
    fs::DirectoryCapability,
    gameplay::GameplayCommandSubmission,
    prelude::Resource,
    project_host::{HeadlessRun, HeadlessRunIntent},
};

#[derive(Clone, Resource)]
struct ProductOutcome;

fn submission() -> GameplayCommandSubmission {
    unreachable!()
}

fn reject_unbounded_commands(project: DirectoryCapability) {
    let intent = HeadlessRunIntent::<ProductOutcome>::new(NonZeroU32::new(1).unwrap());
    let commands = std::iter::repeat_with(submission as fn() -> GameplayCommandSubmission);

    let _run = HeadlessRun::new(project, intent, commands);
}

fn main() {}
