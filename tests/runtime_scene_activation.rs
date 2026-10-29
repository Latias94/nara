#![cfg(all(feature = "runtime-core", feature = "serde"))]

use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use nara::{
    advanced_prelude::{
        CurrentScene, SceneSource, SceneSourceView, StartupSceneActivationPlugin,
        materialize_startup_scene, replace_current_scene_with_product,
        restart_current_scene_with_product,
    },
    app::{
        App, CoreStage, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
        RuntimeClosePolicy, RuntimeObligationLedger,
    },
    asset::ProjectAssetDatabase,
    core::{ByteLimit, ItemLimit},
    ecs::{
        Component, Entity, Resource, World,
        bevy_ecs::{lifecycle::HookContext, world::DeferredWorld},
    },
    identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain},
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentRegistryPlugin, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueKind, PersistentComponentSchemaOwnerId,
    },
    scene::{
        ExpandedSceneCandidate, Name, RuntimeSceneCandidate, RuntimeSceneDocumentCandidate,
        RuntimeSceneLimits, SceneComponentBindings, SceneComponentBindingsPlugin,
        SceneComponentRecord, SceneComponentsPlugin, SceneDocument, SceneEntityId,
        SceneEntityRecord, SceneEntitySource, SceneSpawnReport, SourceDocumentKey,
        SourceDocumentKind, Visibility,
        advanced::{SceneProductResource, SceneProductTransactionLimits},
        scene_component_bindings,
    },
};

#[derive(Resource)]
struct RunValue(u32);

impl SceneProductResource for RunValue {}

#[derive(Resource, Default)]
struct NativeRetirements(usize);

#[derive(Component)]
#[component(on_remove = record_native_retirement)]
struct NativeOwned;

fn record_native_retirement(mut world: DeferredWorld<'_>, _: HookContext) {
    world.resource_mut::<NativeRetirements>().0 += 1;
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn source_key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "e398f4eb-9205-45bb-a021-5464b2ca3e26".parse().unwrap(),
        name,
        SourceDocumentKind::Scene,
    )
    .unwrap()
}

fn data_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("test.runtime-activation.data").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.runtime-activation.Flag"),
                    "Flag",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::required(
                    ComponentFieldId::new("enabled"),
                    "Enabled",
                    ComponentFieldPath::from_fields(["enabled"]),
                    ComponentValueKind::Bool,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap()
}

fn document(name: &str, enabled: bool, visibility: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(id(name))
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(name.into()),
            ),
        )
        .with_component(
            ComponentTypeId::new("nara.scene.Visibility"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(visibility.into()),
            ),
        )
        .with_component(
            ComponentTypeId::new("test.runtime-activation.Flag"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("enabled", ComponentValue::Bool(enabled))]),
            )
            .with_owner(
                PersistentComponentSchemaOwnerId::try_new("test.runtime-activation.data").unwrap(),
            ),
        )])
}

fn runtime_source(
    bindings: &SceneComponentBindings,
    name: &str,
    enabled: bool,
    visibility: &str,
) -> SceneSource {
    let key = source_key(name);
    let bytes = {
        let expanded = ExpandedSceneCandidate::plain(
            key.clone(),
            Arc::new(document(name, enabled, visibility)),
        );
        let database = ProjectAssetDatabase::default();
        RuntimeSceneCandidate::cook(
            &expanded,
            bindings,
            &database,
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap()
    };
    // Execution retains only a separately admitted processed candidate and the actual definitions.
    let candidate =
        RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, RuntimeSceneLimits::default())
            .unwrap()
            .admit(&key, bindings)
            .unwrap();
    SceneSource::runtime(candidate, ByteLimit::new(2 * 1024 * 1024).unwrap()).unwrap()
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::new(4).unwrap(), ItemLimit::ONE)
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        SceneComponentsPlugin,
        SceneComponentBindingsPlugin::new([data_owner()]),
        StartupSceneActivationPlugin,
    ))
    .unwrap()
    .insert_resource(RunValue(1))
    .unwrap()
    .insert_resource(NativeRetirements::default())
    .unwrap();
    app
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance, name: &str) -> Entity {
    match receipt.resolve(world, &id(name)) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live current Scene entity: {other:?}"),
    }
}

fn assert_rejected(report: &SceneSpawnReport, code: &str) {
    assert!(report.instance.is_none());
    assert!(report.diagnostics.has_errors());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code),
        "{report:?}"
    );
}

fn with_current_runtime_scene(
    inspect: impl FnOnce(&mut World) -> Vec<SceneSourceView> + Send + 'static,
) {
    let mut app = app();
    let inspect = Arc::new(Mutex::new(Some(inspect)));
    let inspection = Arc::clone(&inspect);
    let observations = Arc::new(OnceLock::new());
    let output = Arc::clone(&observations);
    app.add_systems(CoreStage::Update, move |world: &mut World| {
        if let Some(inspect) = inspection.lock().unwrap().take() {
            assert!(output.set(inspect(world)).is_ok());
        }
    })
    .unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let materialization = Arc::new(OnceLock::new());
    let result = Arc::clone(&materialization);
    let outcome = candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                let bindings = scene_component_bindings(world).unwrap();
                let source = runtime_source(bindings, "startup", true, "visible");
                assert!(result.set(materialize_startup_scene(world, source)).is_ok());
                assert!(CurrentScene::get(world).is_none());
            });
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    assert!(materialization.get().unwrap().is_ok());
    let mut runtime = candidate.complete_startup().unwrap().promote();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(inspect.lock().unwrap().is_none());
    let views: Vec<SceneSourceView> = observations.get().unwrap().clone();
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    drop(retirement);
    for view in views {
        assert_eq!(view.with_document(|_| ()), None);
        assert_eq!(view.with_runtime_scene(|_| ()), None);
        assert_eq!(view.with_expanded_scene(|_, _| ()), None);
    }
}

#[test]
fn runtime_startup_replacement_restart_and_source_transition_share_one_owner() {
    with_current_runtime_scene(|world| {
        let current = CurrentScene::get(world).unwrap();
        let initial = current.receipt().clone();
        let initial_view = current.source_view();
        let actor = resolve(world, &initial, "startup");
        let occurrence = world
            .get::<SceneEntitySource>(actor)
            .unwrap()
            .occurrence()
            .clone();
        assert_eq!(world.get::<Name>(actor).unwrap().as_str(), "startup");
        assert_eq!(world.get::<Visibility>(actor), Some(&Visibility::Visible));
        assert_eq!(initial_view.with_expanded_scene(|_, _| ()), None);
        assert_eq!(
            initial_view.with_runtime_scene(|candidate| {
                assert_eq!(candidate.document(), current.source());
                assert!(
                    candidate
                        .definitions()
                        .matches(scene_component_bindings(world).unwrap())
                );
                candidate.source_key().clone()
            }),
            Some(source_key("startup"))
        );
        let bindings = scene_component_bindings(world).unwrap().clone();
        let source = runtime_source(&bindings, "next", false, "hidden");
        let report =
            replace_current_scene_with_product(world, &initial, source, limits(), &[], |writer| {
                writer.replace_resource(RunValue(2));
            });
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        let second = report.instance.unwrap();
        let actor = resolve(world, &second, "next");
        assert_eq!(world.get::<Visibility>(actor), Some(&Visibility::Hidden));
        assert_eq!(
            world.get::<SceneEntitySource>(actor).unwrap().occurrence(),
            &occurrence
        );
        assert_eq!(
            bindings
                .data_component(
                    world,
                    actor,
                    &ComponentTypeId::new("test.runtime-activation.Flag")
                )
                .unwrap()
                .unwrap()
                .value,
            ComponentValue::map([("enabled", ComponentValue::Bool(false))])
        );
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &second);
        assert_eq!(initial_view.with_document(|_| ()), None);
        assert!(!matches!(
            initial.resolve(world, &id("startup")),
            EntityLookup::Resolved(_)
        ));
        assert_eq!(world.resource::<RunValue>().0, 2);
        let second_view = CurrentScene::get(world).unwrap().source_view();
        let report = restart_current_scene_with_product(world, &second, limits(), &[], |writer| {
            writer.replace_resource(RunValue(3));
        });
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        let restarted = report.instance.unwrap();
        assert_ne!(second.instance_id(), restarted.instance_id());
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &restarted);
        assert_eq!(
            second_view.with_runtime_scene(|candidate| candidate.source_key().clone()),
            Some(source_key("next"))
        );
        assert!(!matches!(
            second.resolve(world, &id("next")),
            EntityLookup::Resolved(_)
        ));
        assert_eq!(world.resource::<RunValue>().0, 3);

        let expanded = ExpandedSceneCandidate::plain(
            source_key("source"),
            Arc::new(document("source", true, "visible")),
        );
        let report = replace_current_scene_with_product(
            world,
            &restarted,
            SceneSource::direct(expanded, ByteLimit::new(64 * 1024).unwrap()).unwrap(),
            limits(),
            &[],
            |_| {},
        );
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        let expanded_receipt = report.instance.unwrap();
        let expanded_view = CurrentScene::get(world).unwrap().source_view();
        assert_eq!(expanded_view.with_runtime_scene(|_| ()), None);
        assert_eq!(
            expanded_view.with_expanded_scene(|_, database| database.is_none()),
            Some(true)
        );
        assert_eq!(second_view.with_document(|_| ()), None);
        let report = replace_current_scene_with_product(
            world,
            &expanded_receipt,
            runtime_source(&bindings, "returned", true, "visible"),
            limits(),
            &[],
            |_| {},
        );
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        let returned = report.instance.unwrap();
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &returned);
        assert_eq!(
            world
                .get::<Name>(resolve(world, &returned, "returned"))
                .unwrap()
                .as_str(),
            "returned"
        );
        assert_eq!(expanded_view.with_document(|_| ()), None);
        vec![
            initial_view,
            second_view,
            expanded_view,
            CurrentScene::get(world).unwrap().source_view(),
        ]
    });
}

#[test]
fn failed_runtime_replacement_preserves_last_good_views_products_and_native_lifecycle() {
    with_current_runtime_scene(|world| {
        let bindings = scene_component_bindings(world).unwrap().clone();
        let current = CurrentScene::get(world).unwrap();
        let receipt = current.receipt().clone();
        let view = current.source_view();
        let actor = resolve(world, &receipt, "startup");
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let entity_count = world.entities().len();
        // This is a valid declared string. Only the real native Visibility codec rejects it.
        let rejected = runtime_source(&bindings, "broken", false, "unrecognized");
        let report = replace_current_scene_with_product(
            world,
            &receipt,
            rejected,
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue(9));
            },
        );
        assert!(report.diagnostics.has_errors(), "{report:?}");
        assert!(report.instance.is_none());
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &receipt);
        assert_eq!(world.get::<Name>(actor).unwrap().as_str(), "startup");
        assert_eq!(world.resource::<RunValue>().0, 1);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(world.entities().len(), entity_count);
        assert_eq!(
            view.with_document(|document| document.roots.clone()),
            Some(vec![id("startup")])
        );
        assert_eq!(
            view.with_runtime_scene(|candidate| candidate.source_key().clone()),
            Some(source_key("startup"))
        );

        world.entity_mut(actor).insert(NativeOwned);
        let report = replace_current_scene_with_product(
            world,
            &receipt,
            runtime_source(&bindings, "next", false, "visible"),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue(10));
            },
        );
        assert!(report.diagnostics.has_errors(), "{report:?}");
        assert!(report.instance.is_none());
        assert_eq!(world.resource::<NativeRetirements>().0, 0);
        assert!(world.get::<NativeOwned>(actor).is_some());
        assert_eq!(world.resource::<RunValue>().0, 1);
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &receipt);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert!(view.with_runtime_scene(|_| ()).is_some());
        world.entity_mut(actor).remove::<NativeOwned>();
        assert_eq!(world.resource::<NativeRetirements>().0, 1);
        let report = replace_current_scene_with_product(
            world,
            &receipt,
            runtime_source(&bindings, "next", false, "visible"),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue(2));
            },
        );
        assert!(!report.diagnostics.has_errors(), "{report:?}");
        assert_eq!(world.resource::<NativeRetirements>().0, 1);
        assert_eq!(world.resource::<RunValue>().0, 2);
        assert_eq!(view.with_document(|_| ()), None);
        vec![view, CurrentScene::get(world).unwrap().source_view()]
    });
}

#[test]
fn runtime_replacement_rejects_readmitted_definitions_before_product_configuration() {
    with_current_runtime_scene(|world| {
        let bindings = scene_component_bindings(world).unwrap().clone();
        let foreign =
            SceneComponentBindings::new(bindings.native_snapshot().clone(), [data_owner()])
                .unwrap();
        assert!(!bindings.same_definitions(&foreign));
        let receipt = CurrentScene::get(world).unwrap().receipt().clone();
        let view = CurrentScene::get(world).unwrap().source_view();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let report = replace_current_scene_with_product(
            world,
            &receipt,
            runtime_source(&foreign, "foreign", true, "visible"),
            limits(),
            &[],
            |_| panic!("foreign definition admissions cannot configure the current product"),
        );
        assert_rejected(&report, "scene.current.component-bindings-mismatch");
        assert_eq!(CurrentScene::get(world).unwrap().receipt(), &receipt);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(world.resource::<RunValue>().0, 1);
        assert!(view.with_runtime_scene(|_| ()).is_some());
        vec![view]
    });
}

#[test]
fn runtime_startup_rejects_foreign_definitions_without_consuming_activation() {
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app().seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let observations = Arc::new(OnceLock::new());
    let output = Arc::clone(&observations);
    let outcome = candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                let bindings = scene_component_bindings(world).unwrap().clone();
                let foreign =
                    SceneComponentBindings::new(bindings.native_snapshot().clone(), [data_owner()])
                        .unwrap();
                let entities = world.entities().len();
                assert!(!world.contains_resource::<WorldIdentityDomain>());
                let error = materialize_startup_scene(
                    world,
                    runtime_source(&foreign, "foreign", true, "visible"),
                )
                .unwrap_err();
                assert!(error.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "startup.scene.component-bindings-mismatch"
                }));
                assert_eq!(world.entities().len(), entities);
                assert!(!world.contains_resource::<WorldIdentityDomain>());
                assert!(CurrentScene::get(world).is_none());
                let result = materialize_startup_scene(
                    world,
                    runtime_source(&bindings, "accepted", true, "visible"),
                );
                assert!(output.set(result).is_ok());
            });
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    assert!(observations.get().unwrap().is_ok());
    let runtime = candidate.complete_startup().unwrap().promote();
    let current = CurrentScene::get(runtime.world()).unwrap();
    assert_eq!(current.source().roots, [id("accepted")]);
    let view = current.source_view();
    assert_eq!(
        view.with_runtime_scene(|candidate| candidate.source_key().clone()),
        Some(source_key("accepted"))
    );
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        RuntimeCandidateRetirementState::Retired
    );
    drop(retirement);
    assert_eq!(view.with_runtime_scene(|_| ()), None);
}
