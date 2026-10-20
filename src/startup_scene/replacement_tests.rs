use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use nara_app::CoreStage;
use nara_core::ItemLimit;
use nara_ecs::Resource;
use nara_identity::{EntityLookup, SceneEntityId, WorldIdentityDomain};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{
    SceneComponentRecord, SceneEntityRecord, SceneSpawnReport,
    advanced::{SceneProductResource, SceneProductTransactionLimits},
};

use super::*;

#[derive(Debug, PartialEq, Eq, Resource)]
struct RunValue(u32);

impl SceneProductResource for RunValue {}

fn source(name: &str) -> SceneSource {
    SceneSource::direct(
        Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            id(name),
        )])),
        ByteLimit::new(16 * 1024).unwrap(),
    )
    .unwrap()
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
    let inspect = Arc::new(Mutex::new(Some(inspect)));
    let inspection = Arc::clone(&inspect);
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_scene::SceneComponentsPlugin,
        StartupSceneActivationPlugin,
    ))
    .unwrap()
    .insert_resource(RunValue(1))
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
                        .set(materialize_startup_scene(world, source("a")))
                        .is_ok()
                );
            });
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
                writer.replace_resource(RunValue(2));
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
        assert_eq!(*world.resource::<RunValue>(), RunValue(2));
        let second_view = source_view(world);

        let report = restart_current_scene_with_product(world, &second, limits(), &[], |writer| {
            writer.replace_resource(RunValue(3));
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
        assert_eq!(*world.resource::<RunValue>(), RunValue(3));

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
        let foreign =
            nara_scene::spawn_scene(&mut foreign_world, &registry, &SceneDocument::default())
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
        assert_eq!(*world.resource::<RunValue>(), RunValue(1));
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
            SceneSource::direct(invalid, ByteLimit::new(16 * 1024).unwrap()).unwrap(),
            limits(),
            &[],
            |writer| {
                writer.replace_resource(RunValue(9));
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
        assert_eq!(*world.resource::<RunValue>(), RunValue(1));
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
