use std::{
    fs, thread,
    time::{Duration, Instant},
};

use nara_project::ProjectPath;
use nara_scene::{PrefabDocument, SceneDocument, SceneEntityId, SceneEntityRecord};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost,
    project::{
        PreparedAuthoringProject,
        prefab_component_apply::{
            physical::{
                capture_pending_inventory,
                tests::{CONTAINING, DESTINATION, budget},
            },
            publication::{ComponentApplyPublication, ComponentApplyPublicationError},
            recovery_data::{
                PrefabComponentApplyRecoveryData, PrefabComponentApplyRecoveryInput,
                PrefabComponentApplyRecoveryLimits, RecoveryOutputInput, RecoveryRootInput,
                RecoverySourceInput,
            },
        },
    },
    recovery_storage::{
        RecoveryPendingPublication, RecoveryStorageHost, RecoveryStoragePublication,
    },
    source_inventory::{SourceRootObservation, test_support::Fixture},
};

#[derive(Debug, thiserror::Error)]
enum ReadyTestError {
    #[error(transparent)]
    Publication(#[from] ComponentApplyPublicationError),
    #[error(transparent)]
    Preparation(#[from] ComponentApplyReadyError),
}

#[test]
fn ready_installs_actual_saved_startup_and_refreshes_index_under_original_schema() {
    let fixture = Fixture::new(
        "project_location",
        "[startup]\ndefault_scene = 'containing.scene.json'\n\
         [profiles.selected.paths]\nassets = 'assets'\n",
    );
    fixture.scene(CONTAINING);
    fixture.prefab(DESTINATION);
    fs::create_dir(fixture.root.join("recovery")).unwrap();
    let root = crate::test_host_fs::writable_directory(&fixture.root).unwrap();
    let original_schema = AuthoringProjectAdmissionHost::default()
        .prepare(root.share(), Some("selected"))
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let limits = SceneFileLimits::default();
    let original =
        PreparedAuthoringProject::with_scene_limits(original_schema, Box::new([]), limits)
            .admit_startup_closure()
            .unwrap()
            .publish()
            .into_session_snapshot()
            .unwrap();
    let schema = &original.schema;
    let basis = original.startup_scene.schema_basis();
    let mut operation_budget = budget();
    let marker = SceneEntityId::new("saved-startup-marker").unwrap();
    let containing = SceneDocument::from_ordered_roots([SceneEntityRecord::new(marker.clone())])
        .to_json_string()
        .unwrap();
    let destination = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(marker.clone())])
        .to_ron_string()
        .unwrap();
    let recovery = package(&original, &containing, &destination);

    let storage = RecoveryStorageHost::capture(root.share(), &mut operation_budget)
        .unwrap()
        .prepare(
            Arc::clone(&recovery),
            ProjectPath::new("recovery/component-apply.json").unwrap(),
            &mut operation_budget,
        )
        .unwrap();
    let RecoveryStoragePublication::Published(stored) = storage.publish(&mut operation_budget)
    else {
        panic!("Expected actual package publication");
    };
    let pending = stored.bind_pending(&mut operation_budget).unwrap();
    let RecoveryPendingPublication::Published(pending) = pending.publish(&mut operation_budget)
    else {
        panic!("Expected actual Pending publication");
    };
    let pending_identity = pending.capability().identity();
    assert_ne!(pending_identity, schema.manifest_identity());
    drop(pending);
    let mut resume_budget = budget();
    let pending = RecoveryStorageHost::load_pending(
        root.share(),
        PrefabComponentApplyRecoveryLimits::default().encoded_bytes,
        &mut resume_budget,
    )
    .unwrap()
    .bind(recovery, &mut resume_budget)
    .unwrap();
    let physical = capture_pending_inventory(&pending, limits, &mut resume_budget).unwrap();
    let mut publication = ComponentApplyPublication::new(pending, physical, resume_budget);
    assert_eq!(publication.advance().unwrap(), Some(0));
    assert_eq!(publication.advance().unwrap(), Some(1));
    let accepted_startup = publication.accepted_output(0).unwrap().file().identity();
    let (ready, receipt) = publication
        .publish_ready(|input, budget| {
            let owner = PreparedComponentApplyReady::prepare(
                schema,
                &basis,
                Arc::clone(&original.adapters),
                limits,
                input,
                budget,
            )?;
            assert_eq!(owner.bytes(), schema.manifest_bytes());
            assert!(owner.manifest.capability.get().is_none());
            assert_eq!(owner.session.startup_source().identity(), accepted_startup);
            assert_eq!(owner.session.schema.manifest_identity(), pending_identity);
            let manifest = fs::read(fixture.root.join(PROJECT_MANIFEST_PATH)).unwrap();
            assert_ne!(manifest.as_slice(), owner.bytes());
            Ok::<_, ReadyTestError>(owner)
        })
        .unwrap();
    let ready_identity = receipt.source().identity();
    assert_ne!(ready_identity, pending_identity);
    let mut session = ready.install(receipt, publication.budget_mut());
    assert_eq!(session.schema.manifest_identity(), ready_identity);
    assert_eq!(
        session.schema.attempt_generation(),
        schema.attempt_generation()
    );
    assert_eq!(session.schema.manifest_bytes(), schema.manifest_bytes());
    assert_eq!(session.settings(), original.settings());
    assert_eq!(session.startup_scene.schema_basis(), basis);
    assert!(Arc::ptr_eq(&session.adapters, &original.adapters));
    assert!(std::ptr::eq(session.schema.bundle(), schema.bundle()));
    assert_eq!(session.startup_source().identity(), accepted_startup);
    assert_eq!(session.startup_scene.document().entities[0].id, marker);
    session
        .startup_source_validation()
        .validate_with_budget(publication.budget_mut())
        .unwrap();

    let pools = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while session.source_index_has_pending_work() {
        session.advance_source_index(&pools);
        assert!(
            Instant::now() < deadline,
            "Ready source index did not finish"
        );
        thread::sleep(Duration::from_millis(1));
    }
    let index = session.source_index_snapshot().expect("Ready source index");
    assert_eq!(index.manifest_identity(), ready_identity);
    assert_eq!(index.manifest_digest(), schema.manifest_digest());
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

fn package(
    original: &AuthoringProjectSessionSnapshot,
    containing_bytes: &str,
    destination_bytes: &str,
) -> Arc<PrefabComponentApplyRecoveryData> {
    let schema = &original.schema;
    let mut package_budget = budget();
    let inventory =
        SourceInventory::capture(schema, &[], original.scene_limits, &mut package_budget).unwrap();
    let originals: Vec<_> = inventory
        .participants()
        .map(|member| RecoverySourceInput {
            path: member.file().path(),
            key: member.source_key(),
            encoding: member.encoding(),
            explicit_native: member.explicit_native(),
            original_disk: member.bytes(),
        })
        .collect();
    let roots: Vec<_> = inventory
        .roots()
        .map(|root| RecoveryRootInput {
            path: root.path(),
            missing_ancestor: match root.observation() {
                SourceRootObservation::Available { .. } => None,
                SourceRootObservation::Missing { ancestor_path, .. } => Some(ancestor_path),
            },
        })
        .collect();
    let missing_sidecars: Vec<_> = inventory.missing_sidecars().collect();
    PrefabComponentApplyRecoveryData::create(
        PrefabComponentApplyRecoveryInput {
            ready_manifest: schema.manifest_bytes(),
            bundle_path: schema.bundle_path().as_str(),
            bundle_bytes: schema.bundle_bytes(),
            source_universe: original.settings().project.source_universe,
            selected_profile: original.settings().profile_name.as_deref(),
            complete_original_sources: &originals,
            containing: RecoveryOutputInput {
                key: inventory.participant(CONTAINING).unwrap().source_key(),
                output: containing_bytes.as_bytes(),
            },
            destination: RecoveryOutputInput {
                key: inventory.participant(DESTINATION).unwrap().source_key(),
                output: destination_bytes.as_bytes(),
            },
            metadata_guards: &[],
            roots: &roots,
            missing_sidecars: &missing_sidecars,
        },
        PrefabComponentApplyRecoveryLimits::default(),
        &mut package_budget,
    )
    .unwrap()
}
