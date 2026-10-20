use nara::{
    advanced_prelude::{CurrentScene, SceneSource},
    identity::SpawnedSceneInstance,
};

fn forge(
    source: SceneSource,
    receipt: SpawnedSceneInstance,
) -> CurrentScene<'static> {
    CurrentScene { source, receipt }
}

fn main() {}
