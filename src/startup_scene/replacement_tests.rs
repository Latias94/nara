use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use nara_app::CoreStage;
use nara_core::ItemLimit;
use nara_ecs::Resource;
use nara_identity::{EntityLookup, SceneEntityId, WorldIdentityDomain};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId, component_registry,
};
use nara_scene::{
    SceneComponentRecord, SceneEntityRecord, SceneSpawnReport,
    advanced::{SceneProductResource, SceneProductTransactionLimits},
};

use super::*;

#[derive(Debug, Resource)]
struct RunValue(u32, Arc<()>);

impl RunValue {
    fn new(value: u32) -> Self {
        Self(value, Arc::new(()))
    }
}

impl SceneProductResource for RunValue {
    type Baseline = (u32, Arc<()>);

    fn capture_baseline(&self) -> Self::Baseline {
        (self.0, Arc::clone(&self.1))
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.0 == baseline.0 && Arc::ptr_eq(&self.1, &baseline.1)
    }
}

fn source(name: &str) -> SceneSource {
    SceneSource::direct(
        candidate(Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id(name)),
        ]))),
        ByteLimit::new(16 * 1024).unwrap(),
    )
    .unwrap()
}

fn candidate(document: Arc<SceneDocument>) -> ExpandedSceneCandidate {
    ExpandedSceneCandidate::plain(
        nara_scene::SourceDocumentKey::named(
            "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
            "current-scene-replacement-fixture",
            nara_scene::SourceDocumentKind::Scene,
        )
        .unwrap(),
        document,
    )
}

fn id(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::new(4).unwrap(), ItemLimit::new(4).unwrap())
}

fn source_view(world: &World) -> SceneSourceView {
    let source = &world.resource::<CurrentSceneOwner>().source;
    SceneSourceView {
        source: Arc::downgrade(source),
        retained_bytes: source.retained_bytes(),
    }
}

fn assert_rejected(report: &SceneSpawnReport, code: &str) {
    assert!(report.instance.is_none());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == code)
    );
}

fn with_current_scene(inspect: impl FnOnce(&mut World) + Send + 'static) {
    with_selected_current_scene([], source("a"), inspect);
}

fn with_selected_current_scene(
    owners: impl IntoIterator<Item = ComponentSchemaOwnerSnapshot>,
    startup: SceneSource,
    inspect: impl FnOnce(&mut World) + Send + 'static,
) {
    let inspect = Arc::new(Mutex::new(Some(inspect)));
    let inspection = Arc::clone(&inspect);
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_scene::SceneComponentsPlugin,
        nara_scene::SceneComponentBindingsPlugin::new(owners),
        StartupSceneActivationPlugin,
    ))
    .unwrap()
    .insert_resource(RunValue::new(1))
    .unwrap()
    .add_systems(CoreStage::Update, move |world: &mut World| {
        if let Some(inspect) = inspection.lock().unwrap().take() {
            inspect(world);
        }
    })
    .unwrap();
    let mut candidate = nara_app::RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            nara_app::RuntimeObligationLedger::new(),
            nara_app::RuntimeClosePolicy::default(),
        )
        .unwrap();
    let materialization = Arc::new(OnceLock::new());
    let command_result = Arc::clone(&materialization);
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                assert!(
                    command_result
                        .set(materialize_startup_scene(world, startup))
                        .is_ok()
                );
            });
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap();
    assert!(materialization.get().unwrap().is_ok());
    let mut runtime = candidate.complete_startup().unwrap().promote();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(inspect.lock().unwrap().is_none());
    let mut retirement = runtime.begin_retirement();
    assert_eq!(
        retirement.drive_retirement(),
        nara_app::RuntimeCandidateRetirementState::Retired
    );
}

fn data_owner() -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("test.data").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.data.Flag"),
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

fn mixed_source(name: &str, enabled: bool) -> SceneSource {
    SceneSource::direct(
        candidate(Arc::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id(name))
                .with_component(
                    ComponentTypeId::new("nara.scene.Name"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::String(name.to_owned()),
                    ),
                )
                .with_component(
                    ComponentTypeId::new("test.data.Flag"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([("enabled", ComponentValue::Bool(enabled))]),
                    )
                    .with_owner(PersistentComponentSchemaOwnerId::try_new("test.data").unwrap()),
                ),
        ]))),
        ByteLimit::new(16 * 1024).unwrap(),
    )
    .unwrap()
}

#[test]
fn mixed_startup_replacement_and_restart_preserve_selected_data_definitions() {
    with_selected_current_scene([data_owner()], mixed_source("a", true), |world| {
        let bindings = scene_component_bindings(world).unwrap().clone();
        let initial = world.resource::<CurrentSceneOwner>().receipt().clone();
        let EntityLookup::Resolved(entity) = initial.resolve(world, &id("a")) else {
            panic!("startup publishes the mixed entity");
        };
        assert_eq!(world.get::<nara_scene::Name>(entity).unwrap().as_str(), "a");
        assert_eq!(
            bindings
                .data_component(world, entity, &ComponentTypeId::new("test.data.Flag"))
                .unwrap()
                .unwrap()
                .value,
            ComponentValue::map([("enabled", ComponentValue::Bool(true))])
        );

        let replaced = replace_current_scene_with_product(
            world,
            &initial,
            mixed_source("b", false),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue::new(2));
            },
        );
        assert!(
            !replaced.diagnostics.has_errors(),
            "{:?}",
            replaced.diagnostics
        );
        let replaced = replaced.instance.unwrap();
        let restarted = restart_current_scene_with_product(world, &replaced, limits(), &[], |_| {});
        assert!(
            !restarted.diagnostics.has_errors(),
            "{:?}",
            restarted.diagnostics
        );
        let restarted = restarted.instance.unwrap();
        let EntityLookup::Resolved(entity) = restarted.resolve(world, &id("b")) else {
            panic!("restart publishes the retained mixed source");
        };
        assert_eq!(world.get::<nara_scene::Name>(entity).unwrap().as_str(), "b");
        assert_eq!(
            bindings
                .data_component(world, entity, &ComponentTypeId::new("test.data.Flag"))
                .unwrap()
                .unwrap()
                .value,
            ComponentValue::map([("enabled", ComponentValue::Bool(false))])
        );
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &restarted);
        assert_eq!(world.resource::<RunValue>().0, 2);
        assert!(!matches!(
            initial.resolve(world, &id("a")),
            EntityLookup::Resolved(_)
        ));
        assert!(!matches!(
            replaced.resolve(world, &id("b")),
            EntityLookup::Resolved(_)
        ));
    });
}

#[test]
fn replacement_rejects_equivalent_owner_catalog_with_another_admission_basis() {
    with_selected_current_scene([data_owner()], mixed_source("a", true), |world| {
        let installed = scene_component_bindings(world).unwrap().clone();
        let foreign = nara_scene::SceneComponentBindings::new(
            installed.native_snapshot().clone(),
            [data_owner()],
        )
        .unwrap();
        assert!(!installed.same_definitions(&foreign));
        let current = world.resource::<CurrentSceneOwner>().receipt().clone();
        let source_view = source_view(world);
        let entities = world.entities().len();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let next = mixed_source("b", false);
        let assets = nara_scene::advanced::SceneAssetBindings::from_world(world);
        let result = foreign.prepare_scene_replacement_with_claims(
            world,
            next.expanded_scene().unwrap(),
            None,
            &current,
            assets,
            &nara_scene::advanced::SceneNativeRetirementClaims::default(),
            limits(),
            &[],
            |_| panic!("another owner admission cannot configure product state"),
        );
        let Err(report) = result else {
            panic!("another owner admission cannot prepare replacement");
        };
        assert_rejected(&report, "scene.prepared-definition-changed");
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &current);
        assert_eq!(
            source_view.with_document(|scene| scene.roots.clone()),
            Some(vec![id("a")])
        );
        assert_eq!(world.entities().len(), entities);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(world.resource::<RunValue>().0, 1);
    });
}

#[test]
fn direct_replacement_changes_source_and_receipt_and_restart_uses_b() {
    with_current_scene(|world| {
        let first = world.resource::<CurrentSceneOwner>().receipt().clone();
        let first_view = source_view(world);
        let before_entities = world.entities().len();
        let report = replace_current_scene_with_product(
            world,
            &first,
            source("b"),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue::new(2));
            },
        );
        assert!(!report.diagnostics.has_errors());
        let second = report.instance.unwrap();
        let current = world.resource::<CurrentSceneOwner>();
        assert_eq!(current.receipt(), &second);
        assert_eq!(current.source().roots, [id("b")]);
        assert_ne!(first.instance_id(), second.instance_id());
        assert_eq!(world.entities().len(), before_entities);
        assert!(matches!(
            second.resolve(world, &id("b")),
            EntityLookup::Resolved(_)
        ));
        assert!(!matches!(
            first.resolve(world, &id("a")),
            EntityLookup::Resolved(_)
        ));
        assert_eq!(first_view.with_document(|_| ()), None);
        assert_eq!(world.resource::<RunValue>().0, 2);
        let second_view = source_view(world);

        let report = restart_current_scene_with_product(world, &second, limits(), &[], |writer| {
            writer.replace_resource(RunValue::new(3));
        });
        assert!(!report.diagnostics.has_errors());
        let restarted = report.instance.unwrap();
        assert_ne!(second.instance_id(), restarted.instance_id());
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &restarted);
        assert_eq!(
            world.resource::<CurrentSceneOwner>().source().roots,
            [id("b")]
        );
        assert_eq!(
            second_view.with_document(|scene| scene.roots.clone()),
            Some(vec![id("b")])
        );
        assert!(matches!(
            restarted.resolve(world, &id("b")),
            EntityLookup::Resolved(_)
        ));
        assert!(!matches!(
            second.resolve(world, &id("b")),
            EntityLookup::Resolved(_)
        ));
        assert_eq!(world.resource::<RunValue>().0, 3);

        let report = replace_current_scene_with_product(
            world,
            &restarted,
            source("a"),
            limits(),
            &[],
            |_| {},
        );
        assert!(!report.diagnostics.has_errors());
        let returned = report.instance.unwrap();
        assert_ne!(returned.instance_id(), first.instance_id());
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &returned);
        assert_eq!(
            world.resource::<CurrentSceneOwner>().source().roots,
            [id("a")]
        );
        assert_eq!(second_view.with_document(|_| ()), None);
        assert_eq!(world.entities().len(), before_entities);
    });
}

#[test]
fn stale_and_foreign_expected_receipts_reject_before_product_configuration() {
    with_current_scene(|world| {
        let original = world.resource::<CurrentSceneOwner>().receipt().clone();
        let replacement = replace_current_scene_with_product(
            world,
            &original,
            source("b"),
            limits(),
            &[],
            |_| {},
        );
        let current = replacement.instance.unwrap();
        let view = source_view(world);
        let entity_count = world.entities().len();
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let report = restart_current_scene_with_product(world, &original, limits(), &[], |_| {
            panic!("a stale request cannot configure the next Scene");
        });
        assert_rejected(&report, "scene.current.expected-instance-mismatch");

        let registry = ComponentRegistry::from_snapshot(
            component_registry(world).unwrap().snapshot().unwrap(),
        );
        let mut foreign_world = World::new();
        let foreign = nara_scene::spawn_scene(
            &mut foreign_world,
            &registry,
            &candidate(Arc::new(SceneDocument::default())),
            &SceneOccurrenceKey::new("replacement-fixture", "foreign").unwrap(),
            None,
        )
        .instance
        .unwrap();
        let report =
            replace_current_scene_with_product(world, &foreign, source("c"), limits(), &[], |_| {
                panic!("a foreign request cannot configure the next Scene");
            });
        assert_rejected(&report, "scene.current.expected-instance-mismatch");
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &current);
        assert_eq!(
            view.with_document(|scene| scene.roots.clone()),
            Some(vec![id("b")])
        );
        assert_eq!(world.entities().len(), entity_count);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(world.resource::<RunValue>().0, 1);
    });
}

#[test]
fn rejected_candidate_keeps_the_current_pair_and_product_state() {
    with_current_scene(|world| {
        let current = world.resource::<CurrentSceneOwner>().receipt().clone();
        let view = source_view(world);
        let stats = world.resource::<WorldIdentityDomain>().stats();
        let entities = world.entities().len();
        let invalid = Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id("b"),
        )
        .with_component(
            ComponentTypeId::new("test.unavailable"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Bool(true)),
        )]));
        let rejected_source = Arc::downgrade(&invalid);
        let report = replace_current_scene_with_product(
            world,
            &current,
            SceneSource::direct(candidate(invalid), ByteLimit::new(16 * 1024).unwrap()).unwrap(),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue::new(9));
            },
        );
        assert!(report.diagnostics.has_errors());
        assert!(report.instance.is_none());
        assert!(rejected_source.upgrade().is_none());
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &current);
        assert_eq!(
            view.with_document(|scene| scene.roots.clone()),
            Some(vec![id("a")])
        );
        assert_eq!(world.entities().len(), entities);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), stats);
        assert_eq!(world.resource::<RunValue>().0, 1);
        assert!(matches!(
            current.resolve(world, &id("a")),
            EntityLookup::Resolved(_)
        ));
    });
}

#[test]
fn current_replacement_requires_finalized_startup_and_an_existing_owner() {
    with_current_scene(|world| {
        let current = world.resource::<CurrentSceneOwner>().receipt().clone();
        world
            .resource_mut::<StartupSceneActivationAuthority>()
            .phase = StartupSceneActivationPhase::Materialized;
        let report = restart_current_scene_with_product(world, &current, limits(), &[], |_| {
            panic!("replacement cannot run inside the startup activation window");
        });
        assert_rejected(&report, "scene.current.startup-not-finalized");
        world
            .resource_mut::<StartupSceneActivationAuthority>()
            .phase = StartupSceneActivationPhase::Finalized;
        let owner = world.remove_resource::<CurrentSceneOwner>().unwrap();
        let report = restart_current_scene_with_product(world, &current, limits(), &[], |_| {
            panic!("replacement cannot fabricate a current owner");
        });
        assert_rejected(&report, "scene.current.owner-missing");
        world.insert_resource(owner);
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &current);
    });
}
