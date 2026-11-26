use nara_app::App;
use nara_ecs::{
    Component, Resource,
    bevy_ecs::{lifecycle::HookContext, world::DeferredWorld},
};
use nara_reflect::{
    ComponentRegistryError, ComponentRegistryPlugin, PersistentComponent, RegisterSchema,
    SchemaRegistrationError, component_registry,
};

#[derive(Component, Default)]
struct DerivedRuntimeState;

#[derive(Default, Resource)]
struct Added(usize);

#[derive(Component, PersistentComponent)]
#[require(DerivedRuntimeState)]
#[component(on_add = count_on_add)]
#[nara(
    id = "game.Rules",
    version = 1,
    alias = "Rules",
    component_capabilities(inspect, edit),
    field_capabilities(inspect, edit)
)]
struct Rules {
    #[nara(id = "score", alias = "Score")]
    score: u32,
}

fn count_on_add(mut world: DeferredWorld<'_>, _: HookContext) {
    world.resource_mut::<Added>().0 += 1;
}

#[test]
fn app_registration_exports_metadata_and_leaves_native_ecs_semantics_intact() {
    let mut app = App::new();
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    app.world_mut().unwrap().init_resource::<Added>();
    app.register_schema::<Rules>().unwrap();
    let catalog = component_registry(app.world()).unwrap();
    assert_eq!(catalog.components()[0].fields()[0].id().as_str(), "score");
    assert!(app.world().components().component_id::<Rules>().is_none());
    #[cfg(feature = "serde")]
    assert_eq!(
        &nara_reflect::ComponentSchemaCatalog::from_json_bytes(
            nara_reflect::schema_json(&app).unwrap().as_bytes()
        )
        .unwrap(),
        catalog
    );
    assert_eq!(app.world().resource::<Added>().0, 0);

    let entity = app.world_mut().unwrap().spawn(Rules { score: 5 }).id();
    assert!(app.world().get::<DerivedRuntimeState>(entity).is_some());
    assert_eq!(app.world().resource::<Added>().0, 1);
}

#[test]
fn duplicate_registration_preserves_the_complete_catalog() {
    let mut app = App::new();
    assert!(matches!(
        app.register_schema::<Rules>(),
        Err(SchemaRegistrationError::MissingRegistry)
    ));
    app.add_plugin(ComponentRegistryPlugin).unwrap();
    app.register_schema::<Rules>().unwrap();
    assert!(
        matches!(app.register_schema::<Rules>(), Err(SchemaRegistrationError::Catalog(ComponentRegistryError::DuplicateComponentId(id))) if id.as_str() == "game.Rules")
    );
    assert_eq!(
        component_registry(app.world()).unwrap().components().len(),
        1
    );
}
