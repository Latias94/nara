#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use nara::{
    asset::AssetRef,
    reflect::{ComponentTypeId, collect_declared_asset_references},
};
use project_content_fixture::{
    expected_enemy_prefab, expected_startup_scene, load_project_content,
};

#[test]
fn committed_prefab_expands_into_the_exact_startup_document() {
    let loaded = load_project_content();
    let expected_scene = expected_startup_scene(&loaded.plan);
    let expected_prefab = expected_enemy_prefab(&loaded.plan);
    let enemy_prefab = loaded
        .snapshot
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "enemy.prefab.json")
        .unwrap();
    let ui_prefab = loaded
        .snapshot
        .prefabs()
        .iter()
        .find(|prefab| prefab.path().as_str() == "reference-ui.prefab.json")
        .unwrap();

    assert_eq!(loaded.snapshot.scene(), &expected_scene);
    assert_eq!(enemy_prefab.document(), &expected_prefab);
    assert_eq!(
        format!("{}\n", loaded.snapshot.scene().to_json_string().unwrap(),),
        include_str!("../scenes/startup.scene.json"),
    );
    assert_eq!(
        format!("{}\n", enemy_prefab.document().to_json_string().unwrap(),),
        include_str!("../prefabs/enemy.prefab.json"),
    );

    let startup_ids = loaded
        .snapshot
        .scene()
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        startup_ids,
        [
            "enemy-anchor",
            "enemy-anchor-2",
            "enemy-anchor-3",
            "player",
            "player-weapon",
            "reference-ui",
        ]
    );
    let prefab_ids = enemy_prefab
        .document()
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(prefab_ids, ["enemy"]);

    let expanded = loaded.snapshot.expanded_scene();
    let provenance = loaded.snapshot.expanded_scene_provenance();
    assert_eq!(expanded.entities.len(), 24);
    assert_eq!(provenance.len(), expanded.entities.len());
    assert_eq!(
        expanded
            .entities
            .iter()
            .map(|entity| &entity.id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        expanded.entities.len()
    );
    let enemy_projections = provenance
        .iter()
        .filter_map(|(id, source)| {
            let path = source.projection_path()?;
            (source.local_entity_id().as_str() == "enemy")
                .then_some((path.root_anchor().as_str(), id))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        enemy_projections.keys().copied().collect::<Vec<_>>(),
        ["enemy-anchor", "enemy-anchor-2", "enemy-anchor-3"]
    );
    let ui_source_ids = provenance
        .values()
        .filter(|source| {
            source.projection_path().is_some_and(|path| {
                path.root_anchor().as_str() == "reference-ui" && path.steps().len() == 1
            })
        })
        .map(|source| source.local_entity_id().as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ui_source_ids,
        ui_prefab
            .document()
            .entities
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
    );
    assert_eq!(
        ui_prefab
            .document()
            .roots
            .iter()
            .map(|entity| entity.as_str())
            .collect::<Vec<_>>(),
        ["reference-hud", "terminal-overlay"],
    );
    let player = expanded
        .entities
        .iter()
        .find(|entity| {
            entity
                .components
                .contains_key(&ComponentTypeId::new("reference_game.PlayerRole"))
        })
        .unwrap();
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("reference_game.PlayerRole")),
    );
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("reference_game.InitialHealth")),
    );
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("reference_game.InitialVelocity2d")),
    );
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("nara.transform.Transform2d")),
    );
    let weapon = expanded
        .entities
        .iter()
        .find(|entity| {
            entity
                .components
                .contains_key(&ComponentTypeId::new("reference_game.Weapon"))
        })
        .unwrap();
    assert_eq!(player.children.as_slice(), std::slice::from_ref(&weapon.id));
    assert!(
        weapon
            .components
            .contains_key(&ComponentTypeId::new("reference_game.Weapon")),
    );
    let weapon_transform =
        &weapon.components[&ComponentTypeId::new("nara.transform.Transform2d")].value;
    assert_eq!(
        weapon_transform
            .field("translation")
            .unwrap()
            .field_f64("x")
            .unwrap(),
        f64::from(1.2_f32),
    );
    let enemy = expanded
        .entities
        .iter()
        .find(|entity| &entity.id == enemy_projections["enemy-anchor"])
        .unwrap();
    let enemy_anchor = expanded
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "enemy-anchor")
        .unwrap();
    assert_eq!(
        enemy_anchor.children.as_slice(),
        std::slice::from_ref(&enemy.id)
    );
    assert!(
        enemy
            .components
            .contains_key(&ComponentTypeId::new("reference_game.EnemyRole")),
    );
    assert!(
        enemy
            .components
            .contains_key(&ComponentTypeId::new("reference_game.InitialHealth")),
    );
    assert!(
        enemy
            .components
            .contains_key(&ComponentTypeId::new("reference_game.InitialVelocity2d")),
    );
    let second_enemy = expanded
        .entities
        .iter()
        .find(|entity| &entity.id == enemy_projections["enemy-anchor-2"])
        .unwrap();
    assert_eq!(
        second_enemy
            .components
            .get(&ComponentTypeId::new("reference_game.WaveSpawn"))
            .unwrap()
            .value
            .field_u64("tick")
            .unwrap(),
        5,
    );
    let second_transform =
        &second_enemy.components[&ComponentTypeId::new("nara.transform.Transform2d")].value;
    assert_eq!(
        second_transform
            .field("translation")
            .unwrap()
            .field_f64("x")
            .unwrap(),
        9.0,
    );
    for removed_id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.Projectile",
    ] {
        let removed_id = ComponentTypeId::new(removed_id);
        assert!(
            expanded
                .entities
                .iter()
                .all(|entity| !entity.components.contains_key(&removed_id)),
            "expanded content retained removed aggregate {removed_id}",
        );
    }
    let sprite_id = ComponentTypeId::new("nara.sprite.Sprite");
    let sprite = enemy.components.get(&sprite_id).unwrap();
    let references = collect_declared_asset_references(
        loaded.plan.schema_validation().registry(),
        &sprite_id,
        sprite.version,
        &sprite.value,
    )
    .unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(
        references[0].asset_ref(),
        &AssetRef::path("textures/tiny-dungeon.png").unwrap(),
    );

    drop(loaded.snapshot);
    assert_eq!(loaded.loader.budget_snapshot().active_reservations(), 0);
}
