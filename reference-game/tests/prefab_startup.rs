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

    assert_eq!(loaded.snapshot.startup_scene(), &expected_scene);
    assert_eq!(loaded.snapshot.prefabs()[0].document(), &expected_prefab);
    assert_eq!(
        format!(
            "{}\n",
            loaded.snapshot.startup_scene().to_json_string().unwrap(),
        ),
        include_str!("../scenes/startup.scene.json"),
    );
    assert_eq!(
        format!(
            "{}\n",
            loaded.snapshot.prefabs()[0]
                .document()
                .to_json_string()
                .unwrap(),
        ),
        include_str!("../prefabs/enemy.prefab.json"),
    );

    let startup_ids = loaded
        .snapshot
        .startup_scene()
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
            "projectile-fixture",
        ]
    );
    assert_eq!(
        loaded.snapshot.prefabs()[0].path().as_str(),
        "enemy.prefab.json"
    );
    let prefab_ids = loaded.snapshot.prefabs()[0]
        .document()
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(prefab_ids, ["enemy"]);

    let expanded = loaded.snapshot.expanded_startup_scene();
    let expanded_ids = expanded
        .entities
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        expanded_ids,
        [
            "enemy-anchor",
            "enemy-anchor-2",
            "enemy-anchor-2/enemy",
            "enemy-anchor-3",
            "enemy-anchor-3/enemy",
            "enemy-anchor/enemy",
            "player",
            "projectile-fixture",
        ],
    );
    let player = expanded
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "player")
        .unwrap();
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("reference_game.Player")),
    );
    assert!(
        player
            .components
            .contains_key(&ComponentTypeId::new("reference_game.Weapon")),
    );
    let enemy = expanded
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "enemy-anchor/enemy")
        .unwrap();
    assert_eq!(enemy.parent.as_ref().unwrap().as_str(), "enemy-anchor");
    assert!(
        enemy
            .components
            .contains_key(&ComponentTypeId::new("reference_game.Enemy")),
    );
    let second_enemy = expanded
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "enemy-anchor-2/enemy")
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
        &AssetRef::path("textures/player.png").unwrap(),
    );

    drop(loaded.snapshot);
    assert_eq!(loaded.loader.budget_snapshot().active_reservations(), 0);
}
