use nara::{
    advanced_prelude::{StartupSceneActivation, StartupSceneSource},
    identity::SpawnedSceneInstance,
};

fn forge(
    source: StartupSceneSource,
    receipt: SpawnedSceneInstance,
) -> StartupSceneActivation<'static> {
    StartupSceneActivation { source, receipt }
}

fn main() {}
