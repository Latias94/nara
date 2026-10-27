//! Managed Scene selections retain exact native and data-owner admissions.

use std::time::Duration;

use nara_app::{AddPluginsError, App, PluginError, PluginPlanError, RuntimeInstance, StartupStage};
use nara_core::ItemLimit;
use nara_ecs::{Resource, World};
use nara_reflect::{
    COMPONENT_REGISTRY_PLUGIN_ID, ComponentCapability, ComponentRegistryPlugin, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    PersistentComponentSchemaOwnerId, component_registry, validate_component_registry_authority,
};
use nara_scene::{
    SCENE_COMPONENT_BINDINGS_PLUGIN_ID, SCENE_COMPONENTS_SCHEMA_OWNER_ID,
    SceneComponentBindingLimits, SceneComponentBindings, SceneComponentBindingsAuthorityError,
    SceneComponentBindingsPlugin, SceneComponentsPlugin, scene_component_bindings,
    validate_scene_component_bindings_authority,
};

fn owner(name: &str, component: &str) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(name).unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new(component),
                    "Data",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn owners() -> [ComponentSchemaOwnerSnapshot; 2] {
    [
        owner("game.motion", "game.Mover"),
        owner("game.sensing", "game.Sensor"),
    ]
}

#[derive(Default, Resource)]
struct StartupObservations(u32);

fn inspect_startup(world: &mut World) {
    validate_scene_component_bindings_authority(world).unwrap();
    validate_component_registry_authority(world).unwrap();
    let bindings = scene_component_bindings(world).unwrap();
    for component in ["nara.scene.Name", "game.Mover", "game.Sensor"] {
        assert!(bindings.schema(&ComponentTypeId::new(component)).is_some());
    }
    world.resource_mut::<StartupObservations>().0 += 1;
}

#[test]
fn native_registry_is_required_and_never_implicitly_installed() {
    let mut app = App::new();
    let error = app
        .add_plugin(SceneComponentBindingsPlugin::new(owners()))
        .err()
        .expect("the native registry requirement must reject during planning");

    assert!(matches!(
        error,
        AddPluginsError::Plan(PluginPlanError::MissingPlugin {
            plugin: SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
            required: COMPONENT_REGISTRY_PLUGIN_ID,
        })
    ));
    assert!(component_registry(app.world()).is_none());
    assert!(scene_component_bindings(app.world()).is_none());
}

#[test]
fn caller_definitions_do_not_install_a_missing_managed_scene_owner() {
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, SceneComponentsPlugin))
        .unwrap();
    let runtime = RuntimeInstance::start(app).unwrap();
    let caller_bindings = SceneComponentBindings::new(
        component_registry(runtime.world())
            .unwrap()
            .snapshot()
            .unwrap(),
        owners(),
    )
    .unwrap();

    assert_eq!(caller_bindings.data_owners().len(), 2);
    assert!(scene_component_bindings(runtime.world()).is_none());
    assert_eq!(
        validate_scene_component_bindings_authority(runtime.world()),
        Err(SceneComponentBindingsAuthorityError::MissingAuthority)
    );
    validate_component_registry_authority(runtime.world()).unwrap();
    runtime.finish().unwrap();
}

#[test]
fn seal_selects_the_actual_native_snapshot_and_exact_data_admissions() {
    let selected = owners();
    let mut app = App::new();
    // Native providers may build after the Scene selection plugin.
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentBindingsPlugin::new(selected.clone()),
        SceneComponentsPlugin,
    ))
    .unwrap();
    assert!(scene_component_bindings(app.world()).is_none());
    let app = app.seal().unwrap();

    validate_scene_component_bindings_authority(app.world()).unwrap();
    validate_component_registry_authority(app.world()).unwrap();
    let bindings = scene_component_bindings(app.world()).unwrap();
    assert!(
        bindings
            .native_snapshot()
            .ptr_eq(&component_registry(app.world()).unwrap().snapshot().unwrap())
    );
    assert_eq!(bindings.data_owners().len(), selected.len());
    for expected in &selected {
        let actual = bindings
            .data_owners()
            .iter()
            .find(|actual| actual.owner_id() == expected.owner_id())
            .unwrap();
        assert!(actual.ptr_eq(expected));
    }
    let readmitted = owner("game.motion", "game.Mover");
    assert!(
        !bindings
            .data_owner(&ComponentTypeId::new("game.Mover"))
            .unwrap()
            .ptr_eq(&readmitted)
    );
    assert!(
        bindings
            .schema(&ComponentTypeId::new("nara.scene.Name"))
            .is_some()
    );
}

#[test]
fn managed_startup_can_read_metadata_without_a_language_module() {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentBindingsPlugin::new(owners()),
        SceneComponentsPlugin,
    ))
    .unwrap();
    app.init_resource::<StartupObservations>().unwrap();
    app.add_systems(StartupStage::Scene, inspect_startup)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();

    assert_eq!(runtime.world().resource::<StartupObservations>().0, 1);
    runtime.drive(Duration::ZERO).unwrap();
    validate_scene_component_bindings_authority(runtime.world()).unwrap();
    runtime.finish().unwrap();
}

#[test]
fn managed_selection_rejects_duplicate_owners_and_native_or_data_claim_overlap() {
    for selected in [
        vec![
            owner("game.motion", "game.Mover"),
            owner("game.motion", "game.Other"),
        ],
        vec![
            owner("game.motion", "game.Mover"),
            owner("game.sensing", "game.Mover"),
        ],
        vec![owner("game.motion", "nara.scene.Name")],
        vec![owner(
            SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str(),
            "game.Other",
        )],
    ] {
        let mut app = App::new();
        app.add_plugins((
            ComponentRegistryPlugin,
            SceneComponentsPlugin,
            SceneComponentBindingsPlugin::new(selected),
        ))
        .unwrap();
        let error = app.seal().unwrap_err();

        assert!(matches!(
            error.error(),
            PluginError::ComponentRegistrationFailed {
                plugin: SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
                ..
            }
        ));
    }
}

#[test]
fn managed_selection_applies_its_owner_limit_during_finish() {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentBindingsPlugin::new_with_limits(
            owners(),
            SceneComponentBindingLimits::default().with_owners(ItemLimit::new(1).unwrap()),
        ),
    ))
    .unwrap();
    let error = app.seal().unwrap_err();

    assert!(matches!(
        error.error(),
        PluginError::ComponentRegistrationFailed {
            plugin: SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
            message,
            ..
        } if message.contains("maximum is 1")
    ));
}

#[test]
fn plugin_configuration_stops_reading_at_the_first_excess_owner() {
    let selected = owners();
    let reads = std::cell::Cell::new(0);
    let configuration = std::iter::from_fn(|| {
        reads.set(reads.get() + 1);
        assert!(
            reads.get() <= 2,
            "owner configuration must stop at the first excess"
        );
        Some(selected[reads.get() - 1].clone())
    });
    let plugin = SceneComponentBindingsPlugin::new_with_limits(
        configuration,
        SceneComponentBindingLimits::default().with_owners(ItemLimit::ONE),
    );
    assert_eq!(reads.get(), 2);
    let mut app = App::new();
    app.add_plugins((ComponentRegistryPlugin, plugin)).unwrap();
    assert!(app.seal().is_err());
}
