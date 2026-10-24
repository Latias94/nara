use std::{collections::BTreeMap, fs, path::PathBuf};

use bevy_ecs::world::World;
use nara_scene::{
    Name, SceneDocumentCandidate, SceneEntityId, SceneEntitySource, Visibility,
    retire_and_despawn_scene_instance,
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
    let source_key = scene_source_key("module-consumer/basic-scene");
    let occurrence = scene_occurrence("module-consumer/basic-scene");
    let candidate = nara_scene::ExpandedSceneCandidate::plain(
        source_key.clone(),
        std::sync::Arc::new(published.document().clone()),
    );
    assert_eq!(candidate.source_key(), &source_key);
    assert_eq!(candidate.document(), published.document());
    let spawn = nara_scene::spawn_scene(&mut world, &registry, &candidate, &occurrence, None);
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
            assert_eq!(source.containing_source(), &source_key);
            assert_eq!(source.occurrence(), &occurrence);
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

    assert_eq!(
        retire_and_despawn_scene_instance(&mut world, instance).unwrap(),
        2
    );
    assert!(
        world
            .query::<&SceneEntitySource>()
            .iter(&world)
            .next()
            .is_none()
    );
    assert!(matches!(
        instance.resolve(&world, &SceneEntityId::new("player").unwrap()),
        nara_identity::EntityLookup::Tombstoned(_)
    ));
    assert!(retire_and_despawn_scene_instance(&mut world, instance).is_err());
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/basic.scene.ron")
}

fn scene_source_key(name: &str) -> nara_scene::SourceDocumentKey {
    nara_scene::SourceDocumentKey::named(
        "ae9c4204-2585-4d22-9da7-6400ebd89664".parse().unwrap(),
        name,
        nara_scene::SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn scene_occurrence(slot: &str) -> nara_scene::SceneOccurrenceKey {
    nara_scene::SceneOccurrenceKey::new("nara.scene.tests", slot).unwrap()
}
