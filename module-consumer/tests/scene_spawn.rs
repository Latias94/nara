use std::{collections::BTreeMap, fs, path::PathBuf};

use bevy_ecs::world::World;
use nara_scene::{
    Name, SceneDocumentCandidate, SceneEntityId, SceneEntitySource, Visibility, spawn_scene,
};
use nara_scene_module_consumer::frozen_scene_registry;

#[test]
fn committed_scene_fixture_parses_validates_and_spawns() {
    let registry = frozen_scene_registry().expect("scene registry should freeze");
    let encoded = fs::read(fixture_path()).expect("committed scene fixture should be readable");
    let candidate = SceneDocumentCandidate::decode_ron_bytes(&encoded)
        .expect("scene fixture should parse within default limits");
    let published = candidate
        .publish(&registry)
        .expect("scene fixture should validate for authoring");
    assert!(!published.source_upgrade_required());

    let validation = published.document().validate(&registry);
    assert!(!validation.has_errors(), "{validation:#?}");

    let mut world = World::new();
    let spawn = spawn_scene(&mut world, &registry, published.document());
    assert!(!spawn.diagnostics.has_errors(), "{:#?}", spawn.diagnostics);
    let instance = spawn
        .instance
        .as_ref()
        .expect("successful spawn should publish stable scene identity");
    assert_eq!(
        instance.entity_ids(),
        &[
            SceneEntityId::new("enemy").unwrap(),
            SceneEntityId::new("player").unwrap(),
        ]
    );

    let mut spawned = world
        .query::<(&SceneEntitySource, &Name, &Visibility)>()
        .iter(&world)
        .map(|(source, name, visibility)| {
            (
                source.entity_id.as_str().to_owned(),
                (name.as_str().to_owned(), *visibility),
            )
        })
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        spawned.remove("enemy"),
        Some(("Enemy".to_owned(), Visibility::Hidden))
    );
    assert_eq!(
        spawned.remove("player"),
        Some(("Player".to_owned(), Visibility::Visible))
    );
    assert!(spawned.is_empty());
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/basic.scene.ron")
}
