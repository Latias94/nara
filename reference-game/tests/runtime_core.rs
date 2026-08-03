use nara::prelude::{App, ComponentTypeId, HeadlessRuntimePlugins};
use nara_reference_game::{ReferenceGamePlugin, ReferenceWavePlugin, WaveOutcome, WaveSnapshot};

#[test]
fn rust_code_first_plugins_publish_the_complete_game_schema() {
    let mut app = App::new();
    app.add_plugins((
        HeadlessRuntimePlugins,
        nara::advanced_prelude::StartupSceneActivationPlugin,
        ReferenceGamePlugin,
        ReferenceWavePlugin,
    ))
    .unwrap();
    let app = app.seal().unwrap();

    let registry = nara::reflect::component_registry(app.world()).unwrap();
    for component_id in [
        "reference_game.PlayerRole",
        "reference_game.EnemyRole",
        "reference_game.InitialHealth",
        "reference_game.InitialVelocity2d",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
    ] {
        assert!(
            registry
                .schema(&ComponentTypeId::new(component_id))
                .is_some(),
            "missing code-first schema {component_id}"
        );
    }
    for component_id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.Projectile",
        "reference_game.Health",
        "reference_game.Velocity2d",
        "reference_game.WeaponCooldown",
        "reference_game.ProjectileRole",
        "reference_game.ProjectileDamage",
        "reference_game.ProjectileLifetime",
        "reference_game.ProjectileId",
        "nara.transform.GlobalTransform2d",
    ] {
        assert!(
            registry
                .schema(&ComponentTypeId::new(component_id))
                .is_none(),
            "removed or runtime-only component {component_id} entered the schema",
        );
    }
    assert_eq!(
        app.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Running
    );
}
