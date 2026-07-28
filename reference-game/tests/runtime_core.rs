use nara::prelude::{App, ComponentTypeId, HeadlessRuntimePlugins};
use nara_reference_game::{ReferenceGamePlugin, ReferenceWavePlugin, WaveOutcome, WaveSnapshot};

#[test]
fn rust_code_first_plugins_publish_the_complete_game_schema() {
    let mut app = App::new();
    app.add_plugins((
        HeadlessRuntimePlugins,
        ReferenceGamePlugin,
        ReferenceWavePlugin,
    ))
    .unwrap();
    let app = app.seal().unwrap();

    let registry = nara::reflect::component_registry(app.world()).unwrap();
    for component_id in [
        "reference_game.Player",
        "reference_game.Enemy",
        "reference_game.WaveSpawn",
        "reference_game.Weapon",
        "reference_game.Projectile",
    ] {
        assert!(
            registry
                .schema(&ComponentTypeId::new(component_id))
                .is_some(),
            "missing code-first schema {component_id}"
        );
    }
    assert_eq!(
        app.world().resource::<WaveSnapshot>().outcome,
        WaveOutcome::Running
    );
}
