use nara::identity::{
    SceneEntityId, SpawnedSceneInstance, TombstoneCause, WorldEntityToken, WorldIdentityDomain,
};
use nara::ecs::World;

fn invoke(
    world: &mut World,
    current: &SpawnedSceneInstance,
    entries: &[(SceneEntityId, WorldEntityToken)],
) {
    let _ = WorldIdentityDomain::prepare_exact_scene_instance_replacement(
        world,
        current,
        entries,
        TombstoneCause::Replaced,
    );
}

fn main() {}
