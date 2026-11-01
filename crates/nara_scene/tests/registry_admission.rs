//! Owner presence is distinct from admitted native execution authority.

use std::sync::{Arc, OnceLock};

use nara_app::App;
use nara_asset::AssetServer;
use nara_diagnostic::DiagnosticReport;
use nara_ecs::World;
use nara_identity::{EntityLookup, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryAuthorityError, ComponentRegistryBuilder, ComponentRegistryPlugin,
    ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits, PersistentComponentSchemaOwnerId,
    component_registry, component_registry_is_installed, preloaded_component_registry_plugin,
    validate_component_registry_authority,
};
use nara_scene::{
    ExpandedSceneCandidate, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings,
    SceneComponentBindingsPlugin, SceneComponentRecord, SceneComponentsPlugin,
    SceneDataComponentError, SceneDocument, SceneEntityId, SceneEntityRecord, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind, export_scene, prepare_scene, scene_component_bindings,
};

fn registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_into(&mut builder)
        .unwrap();
    builder.freeze().unwrap()
}

fn candidate() -> ExpandedSceneCandidate {
    ExpandedSceneCandidate::plain(
        SourceDocumentKey::named(
            "d75f3f5d-23a8-4a51-a85c-1d2314a7db6f".parse().unwrap(),
            "registry-admission",
            SourceDocumentKind::Scene,
        )
        .unwrap(),
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("retained".into()),
            ),
        )])),
    )
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("scene.tests", "registry-admission").unwrap()
}

fn install_unfinished_owner(app: &mut App, preloaded: bool, snapshot: &ComponentRegistrySnapshot) {
    if preloaded {
        app.add_plugins((
            preloaded_component_registry_plugin(Arc::new(OnceLock::from(snapshot.clone()))),
            SceneComponentsPlugin,
        ))
        .unwrap();
    } else {
        app.add_plugins((ComponentRegistryPlugin, SceneComponentsPlugin))
            .unwrap();
    }
    assert!(component_registry_is_installed(app.world()));
    assert!(component_registry(app.world()).is_none());
    assert_eq!(
        validate_component_registry_authority(app.world()),
        Err(ComponentRegistryAuthorityError::MissingAuthority)
    );
}

fn assert_code(diagnostics: &DiagnosticReport, code: &str) {
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "expected {code}: {diagnostics:?}"
    );
}

#[test]
fn installed_unadmitted_owners_reject_native_and_mixed_preparation_and_export() {
    for preloaded in [false, true] {
        let caller = registry();
        let installed = registry();
        assert_eq!(caller.catalog(), installed.catalog());
        assert!(!caller.ptr_eq(&installed));
        let bindings = SceneComponentBindings::new(caller.clone(), []).unwrap();
        let mut app = App::new();
        install_unfinished_owner(&mut app, preloaded, &installed);
        let world = app.world_mut().unwrap();
        let assets = AssetServer::new();
        world.insert_resource(assets.clone());
        let entities = world.entities().len();

        let report = prepare_scene(world, &caller, &candidate(), &occurrence(), None).unwrap_err();
        assert_code(&report.diagnostics, "scene.prepared-definition-changed");
        let report = bindings
            .prepare_scene(world, &candidate(), &occurrence(), None)
            .unwrap_err();
        assert_code(&report.diagnostics, "scene.prepared-definition-changed");

        let report = export_scene(world, &caller);
        assert!(report.output().is_none());
        assert_code(&report.diagnostics, "scene.export-native-authority-invalid");
        let report = bindings.export_scene(world);
        assert!(report.output().is_none());
        assert_code(
            &report.diagnostics,
            "scene.export-bindings-authority-invalid",
        );
        assert_eq!(world.entities().len(), entities);
        assert_eq!(world.resource::<AssetServer>(), &assets);
        assert!(!world.contains_resource::<WorldIdentityDomain>());
    }
}

#[test]
fn installing_an_unadmitted_owner_rejects_existing_standalone_preparation() {
    for preloaded in [false, true] {
        for mixed in [false, true] {
            let caller = registry();
            let installed = registry();
            assert!(!caller.ptr_eq(&installed));
            let bindings = SceneComponentBindings::new(caller.clone(), []).unwrap();
            let mut app = App::new();
            let assets = AssetServer::new();
            app.insert_resource(assets.clone()).unwrap();
            let prepared = if mixed {
                bindings.prepare_scene(app.world_mut().unwrap(), &candidate(), &occurrence(), None)
            } else {
                prepare_scene(
                    app.world_mut().unwrap(),
                    &caller,
                    &candidate(),
                    &occurrence(),
                    None,
                )
            }
            .unwrap();
            install_unfinished_owner(&mut app, preloaded, &installed);
            let world = app.world_mut().unwrap();
            let entities = world.entities().len();

            let report = prepared.commit(world);

            assert!(report.instance.is_none());
            assert_code(&report.diagnostics, "scene.prepared-definition-changed");
            assert_eq!(world.entities().len(), entities);
            assert_eq!(world.resource::<AssetServer>(), &assets);
            assert!(!world.contains_resource::<WorldIdentityDomain>());
        }
    }
}

#[test]
fn owner_free_worlds_keep_explicit_standalone_execution() {
    for mixed in [false, true] {
        let registry = registry();
        let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
        let candidate = candidate();
        let mut world = World::new();
        assert!(!component_registry_is_installed(&world));
        let prepared = if mixed {
            bindings.prepare_scene(&mut world, &candidate, &occurrence(), None)
        } else {
            prepare_scene(&mut world, &registry, &candidate, &occurrence(), None)
        }
        .unwrap();
        let report = prepared.commit(&mut world);
        assert!(report.instance.is_some(), "{report:?}");
        let report = if mixed {
            bindings.export_scene(&world)
        } else {
            export_scene(&world, &registry)
        };
        assert_eq!(&report.output().unwrap().document, candidate.document());
    }
}

#[test]
fn unfinished_owner_rejects_actual_data_access_and_preserves_records_through_admission() {
    let owner_id = PersistentComponentSchemaOwnerId::try_new("game.registry-admission").unwrap();
    let component = ComponentTypeId::new("game.registry-admission.Score");
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner_id.clone(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component.clone(), "Score", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::empty(),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let plain = candidate();
    let mut document = plain.document().clone();
    document.entities[0].components.insert(
        component.clone(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7))
            .with_owner(owner_id),
    );
    let source = ExpandedSceneCandidate::plain(plain.source_key().clone(), Arc::new(document));
    for preloaded in [false, true] {
        let registry = registry();
        let bindings = SceneComponentBindings::new(registry.clone(), [owner.clone()]).unwrap();
        let mut app = App::new();
        let instance = bindings
            .prepare_scene(app.world_mut().unwrap(), &source, &occurrence(), None)
            .unwrap()
            .commit(app.world_mut().unwrap())
            .instance
            .unwrap();
        let EntityLookup::Resolved(entity) =
            instance.resolve(app.world(), &SceneEntityId::new("root").unwrap())
        else {
            panic!("the standalone source entity is live")
        };
        assert_eq!(
            bindings
                .data_component(app.world(), entity, &component)
                .unwrap()
                .unwrap()
                .value,
            ComponentValue::U64(7)
        );
        install_unfinished_owner(&mut app, preloaded, &registry);
        app.add_plugins(SceneComponentBindingsPlugin::new([owner.clone()]))
            .unwrap();
        assert!(matches!(
            bindings.data_component(app.world(), entity, &component),
            Err(SceneDataComponentError::NativeAuthority(
                ComponentRegistryAuthorityError::MissingAuthority
            ))
        ));
        assert!(matches!(
            bindings.set_data_component(
                app.world_mut().unwrap(),
                entity,
                &component,
                ComponentValue::U64(9),
                None,
                ComponentValueMaterializationLimits::default(),
            ),
            Err(SceneDataComponentError::NativeAuthority(
                ComponentRegistryAuthorityError::MissingAuthority
            ))
        ));
        assert!(matches!(
            bindings.remove_data_component(app.world_mut().unwrap(), entity, &component),
            Err(SceneDataComponentError::NativeAuthority(
                ComponentRegistryAuthorityError::MissingAuthority
            ))
        ));
        let app = app.seal().unwrap();
        let admitted = scene_component_bindings(app.world()).unwrap();
        assert_eq!(
            admitted
                .data_components(app.world(), entity)
                .unwrap()
                .count(),
            1
        );
        assert_eq!(
            admitted
                .data_component(app.world(), entity, &component)
                .unwrap()
                .unwrap()
                .value,
            ComponentValue::U64(7)
        );
    }
}
