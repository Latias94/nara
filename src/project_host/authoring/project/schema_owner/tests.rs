use std::{
    fs,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara_core::ItemLimit;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::{borrow_prepared_prefab_schema_source, borrow_prepared_scene_schema_source},
    PrefabAuthoringSession, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
    PrefabExpansionFailureKind, PrefabOverlay, SceneAuthoringSession, SceneComponentRecord,
    SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    SchemaReclassificationWork,
};
use nara_tasks::{TaskPoolConfig, TaskPools};

use super::*;
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionHost, AuthoringSchemaOwnerPreparation, ProjectSourceIndexSnapshot,
        document::AuthoringDocumentSourceFact,
        project_authoring_adapters,
        tests::{TestAuthoringProject, independent_capture::independent_project},
    },
    file_publication::{FilePublicationHost, FilePublicationOutcome},
};

const STARTUP: &str = "scenes/startup.scene.json";
const BUNDLE: &str = "schemas/authoring.bundle.json";

fn installed_owner() -> ComponentSchemaOwnerSnapshot {
    AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("test.prepared-project.owner").unwrap(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(
                    ComponentTypeId::new("test.PreparedProject"),
                    "Prepared project",
                    ComponentSchemaVersion::ONE,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([
                    ComponentFieldSchema::required(
                        ComponentFieldId::new("value"),
                        "Value",
                        ComponentFieldPath::from_fields(["value"]),
                        ComponentValueKind::U64,
                    )
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                    ComponentFieldSchema::optional(
                        ComponentFieldId::new("image"),
                        "Image",
                        ComponentFieldPath::from_fields(["image"]),
                        ComponentValueKind::AssetRef,
                    )
                    .with_capability(ComponentCapability::AssetRef)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap()
    .owners()[0]
        .clone()
}

fn open_project(project: &TestAuthoringProject) -> AuthoringProjectSessionSnapshot {
    let root = crate::project_tooling::host_fs::writable_directory(&project.root).unwrap();
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(root, None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapters = project_authoring_adapters(schema.bundle()).unwrap();
    PreparedAuthoringProject::new(schema, adapters)
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap()
}

fn prepare_owner(
    snapshot: &AuthoringProjectSessionSnapshot,
) -> (PreparedAuthoringSchemaOwner, FilePublicationHost) {
    let AuthoringSchemaOwnerPreparation::Prepared { owner, publication } =
        snapshot.schema.prepare_owner(&installed_owner()).unwrap()
    else {
        panic!("the independent project must not already admit the installed owner");
    };
    (owner, publication)
}

fn publish_owner(
    owner: PreparedAuthoringSchemaOwner,
    publication: &mut FilePublicationHost,
) -> AdmittedAuthoringSchemaOwner {
    owner.validate().unwrap();
    let FilePublicationOutcome::Published(accepted) = publication.publish(&owner.bytes()) else {
        panic!("the actual bundle leaf must publish");
    };
    owner.attach(accepted).unwrap()
}

fn add_entity(id: &str) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: SceneEntityRecord::new(SceneEntityId::new(id).unwrap()),
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }])
}

fn finish_source_index(
    snapshot: &mut AuthoringProjectSessionSnapshot,
    pools: &TaskPools,
) -> Arc<ProjectSourceIndexSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while snapshot.source_index_has_pending_work() {
        snapshot.advance_source_index(pools);
        assert!(Instant::now() < deadline, "actual source index timed out");
        thread::sleep(Duration::from_millis(1));
    }
    snapshot
        .source_index_snapshot()
        .expect("the actual source index must publish")
}

#[test]
fn saved_project_and_dirty_scene_keep_distinct_roots_and_rebuild_the_actual_source_index() {
    let project = independent_project();
    let saved_bytes = fs::read(project.root.join(STARTUP)).unwrap();
    let mut incumbent = open_project(&project);
    let worker = ItemLimit::ONE;
    let pools =
        TaskPools::try_new(TaskPoolConfig::threaded(worker, worker, worker).unwrap()).unwrap();
    let old_index = finish_source_index(&mut incumbent, &pools);
    let saved = incumbent.fork_startup_scene();
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(incumbent.fork_startup_scene());
    let source = AuthoringDocumentSourceFact::from_captured_source(
        incumbent.startup_source(),
        incumbent.startup_native_source(),
    )
    .unwrap();
    assert!(
        scene
            .apply_authoring_patch(&add_entity("dirty-current"))
            .applied
    );
    let dirty_document = scene.shared_document();
    let dirty_revision = scene.revision();
    let old_basis = scene.authoring_schema_basis();
    let (mut owner, mut publication) = prepare_owner(&incumbent);
    let basis = owner.schema_basis().clone();
    let prepared_scene = scene
        .prepare_schema_reclassification(&basis, &mut SchemaReclassificationWork::default())
        .unwrap();
    let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
    assert_eq!(prepared.readiness(), AuthoringProjectReadiness::Complete);
    assert!(!prepared.diagnostics().has_errors());
    assert!(prepared.composition_failure().is_none());
    assert_eq!(prepared.startup.document.document(), saved.document());
    assert_eq!(prepared.startup.document.schema_basis(), basis);
    assert_eq!(prepared.startup.capture.root_revision(), None);
    let project_usage = owner.budget().usage();
    let current = owner
        .capture_scene(
            &source,
            borrow_prepared_scene_schema_source(&scene, &prepared_scene).unwrap(),
            &[],
        )
        .unwrap();
    prepared.validate(&mut owner).unwrap();
    let final_usage = owner.budget().usage();
    assert!(final_usage.decode_work() > project_usage.decode_work());
    let successor = publish_owner(owner, &mut publication);
    let current = current.attach(&successor).unwrap_or_else(|_| {
        panic!("the current Scene capture must attach to its exact nominal successor")
    });
    assert_eq!(current.root_revision(), Some(dirty_revision));
    let (receipt, evidence) = prepared.attach(successor).unwrap();
    let mut successor = receipt.into_session_snapshot().unwrap();
    assert_eq!(successor.usage(), final_usage);
    assert_eq!(successor.fork_startup_scene().document(), saved.document());
    assert_eq!(successor.fork_startup_scene().schema_basis(), basis);
    assert!(successor.take_startup_preview().is_none());
    assert!(successor.source_index_snapshot().is_none());
    assert!(successor.source_index_has_pending_work());
    let new_index = finish_source_index(&mut successor, &pools);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
    let actual_bundle = successor
        .project_root()
        .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
        .unwrap();
    let actual_digest = ContentDigest::of_bytes(&fs::read(project.root.join(BUNDLE)).unwrap());
    assert!(!Arc::ptr_eq(&old_index, &new_index));
    assert_ne!(new_index.bundle_identity(), old_index.bundle_identity());
    assert_ne!(new_index.bundle_digest(), old_index.bundle_digest());
    assert_eq!(new_index.bundle_identity(), actual_bundle.identity());
    assert_eq!(new_index.bundle_digest(), actual_digest);
    assert_eq!(
        new_index.bundle_identity(),
        successor.schema.bundle_identity()
    );
    assert_eq!(
        new_index.bundle_digest(),
        successor.authoring_bundle_digest()
    );
    assert_eq!(
        evidence.receipt().previous_identity(),
        Some(old_index.bundle_identity())
    );
    assert_eq!(
        evidence.receipt().published_identity(),
        Some(actual_bundle.identity())
    );
    assert_eq!(
        evidence.receipt().verified_content_digest(),
        Some(actual_digest)
    );
    assert_eq!(scene.authoring_schema_basis(), old_basis);
    assert_eq!(scene.revision(), dirty_revision);
    assert!(Arc::ptr_eq(&scene.shared_document(), &dirty_document));
    assert!(scene.commit_schema_reclassification(prepared_scene).is_ok());
    assert_eq!(scene.revision(), dirty_revision);
    assert!(Arc::ptr_eq(&scene.shared_document(), &dirty_document));
    assert_eq!(fs::read(project.root.join(STARTUP)).unwrap(), saved_bytes);
    assert_eq!(incumbent.fork_startup_scene().document(), saved.document());
}

#[test]
fn saved_startup_is_reread_as_b_while_the_incumbent_retains_a_and_the_scene_is_dirty_c() {
    let project = independent_project();
    let incumbent = open_project(&project);
    let saved_a = incumbent.fork_startup_scene();
    let original = AuthoringDocumentSourceFact::from_captured_source(
        incumbent.startup_source(),
        incumbent.startup_native_source(),
    )
    .unwrap();
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(incumbent.fork_startup_scene());
    assert!(scene.apply_authoring_patch(&add_entity("saved-b")).applied);
    let saved_b = scene.document().clone();
    let bytes_b = saved_b.to_json_string().unwrap();
    let file = incumbent
        .project_root()
        .open_file_exact(&RelativePath::new(STARTUP).unwrap())
        .unwrap();
    let mut save = FilePublicationHost::bind_existing(
        &incumbent.project_root(),
        STARTUP,
        &file,
        original.digest(),
        65_536,
    )
    .unwrap();
    let FilePublicationOutcome::Published(accepted) = save.publish(bytes_b.as_bytes()) else {
        panic!("saving B through the actual file owner must succeed");
    };
    let (_, leaf) = accepted.into_parts();
    save.commit_published(leaf);
    let (identity, parent, digest) = save.accepted_source_observation().unwrap();
    let current_source = original
        .with_persistence_observation(identity, parent, digest)
        .unwrap();
    assert!(scene.apply_authoring_patch(&add_entity("dirty-c")).applied);
    let dirty_c = scene.shared_document();
    let revision_c = scene.revision();
    let (mut owner, mut publication) = prepare_owner(&incumbent);
    let basis = owner.schema_basis().clone();
    let reclassified = scene
        .prepare_schema_reclassification(&basis, &mut SchemaReclassificationWork::default())
        .unwrap();
    let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
    assert_ne!(saved_a.document(), &saved_b);
    assert_ne!(dirty_c.as_ref(), &saved_b);
    assert_eq!(prepared.startup.document.document(), &saved_b);
    assert_eq!(prepared.startup.source.identity(), identity);
    assert_eq!(prepared.startup.source.parent_identity(), parent);
    assert_eq!(prepared.startup.source.digest(), digest);
    assert_eq!(prepared.startup.source.encoded_bytes(), bytes_b.len());
    assert_eq!(prepared.startup.capture.root_revision(), None);
    let current = owner
        .capture_scene(
            &current_source,
            borrow_prepared_scene_schema_source(&scene, &reclassified).unwrap(),
            &[],
        )
        .unwrap();
    prepared.validate(&mut owner).unwrap();
    let successor = publish_owner(owner, &mut publication);
    let current = current.attach(&successor).unwrap_or_else(|_| {
        panic!("dirty C must retain the same prepared basis independently of saved B")
    });
    assert_eq!(current.root_revision(), Some(revision_c));
    let (receipt, _) = prepared.attach(successor).unwrap();
    let mut successor = receipt.into_session_snapshot().unwrap();
    assert_eq!(successor.fork_startup_scene().document(), &saved_b);
    assert_eq!(successor.fork_startup_scene().schema_basis(), basis);
    assert_eq!(successor.startup_source().identity(), identity);
    assert_eq!(successor.startup_source().digest(), digest);
    assert!(successor.take_startup_preview().is_none());
    successor.startup_source_validation().validate().unwrap();
    assert_eq!(
        incumbent.fork_startup_scene().document(),
        saved_a.document()
    );
    assert_eq!(incumbent.startup_source().identity(), original.identity());
    assert_ne!(original.identity(), identity);
    assert_eq!(scene.revision(), revision_c);
    assert!(Arc::ptr_eq(&scene.shared_document(), &dirty_c));
    assert_eq!(
        fs::read(project.root.join(STARTUP)).unwrap(),
        bytes_b.as_bytes()
    );
}

#[test]
fn prepared_project_selects_the_dirty_prefab_arc_and_revision_without_disk_preview_seeding() {
    let project = independent_project();
    let mut root = SceneEntityRecord::new(SceneEntityId::new("root").unwrap());
    root.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join(STARTUP),
        SceneDocument::from_ordered_roots([root])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let disk_prefab = fs::read(project.root.join("prefabs/outer.prefab.json")).unwrap();
    let incumbent = open_project(&project);
    let prefab = incumbent
        .prefab(&AssetPath::new("outer.prefab.json").unwrap())
        .unwrap();
    let source =
        AuthoringDocumentSourceFact::from_captured_source(prefab.source(), prefab.native_source())
            .unwrap();
    let mut session = PrefabAuthoringSession::from_published_authoring_document(
        prefab.publish_document().unwrap(),
    );
    assert!(
        session
            .apply_authoring_patch(&add_entity("dirty-prefab"))
            .applied
    );
    let dirty = session.shared_document();
    let revision = session.revision();
    let (mut owner, mut publication) = prepare_owner(&incumbent);
    let reclassified = session
        .prepare_schema_reclassification(
            owner.schema_basis(),
            &mut SchemaReclassificationWork::default(),
        )
        .unwrap();
    let open = [AuthoringPrefabSessionSnapshot::capture_prepared(
        &source,
        borrow_prepared_prefab_schema_source(&session, &reclassified).unwrap(),
    )
    .unwrap()];
    let prepared = owner.prepare_project(&incumbent, &open).unwrap();
    assert_eq!(prepared.readiness(), AuthoringProjectReadiness::Complete);
    assert!(!prepared.diagnostics().has_errors());
    assert!(prepared.composition_failure().is_none());
    prepared.validate(&mut owner).unwrap();
    let basis = owner.schema_basis().clone();
    let successor = publish_owner(owner, &mut publication);
    let (receipt, _) = prepared.attach(successor).unwrap();
    let mut successor = receipt.into_session_snapshot().unwrap();
    let selected = successor
        .prefab(&AssetPath::new("outer.prefab.json").unwrap())
        .unwrap();
    assert!(Arc::ptr_eq(selected.shared_source_document(), &dirty));
    assert_eq!(selected.session_revision(), Some(revision));
    assert_eq!(selected.basis, basis);
    assert!(
        selected
            .document()
            .entities
            .iter()
            .any(|entity| entity.id.as_str() == "dirty-prefab")
    );
    assert_eq!(
        selected.source().digest(),
        ContentDigest::of_bytes(&disk_prefab)
    );
    assert!(successor.take_startup_preview().is_none());
    assert!(session.commit_schema_reclassification(reclassified).is_ok());
    assert_eq!(session.revision(), revision);
    assert!(Arc::ptr_eq(&session.shared_document(), &dirty));
    assert_eq!(
        fs::read(project.root.join("prefabs/outer.prefab.json")).unwrap(),
        disk_prefab
    );
    assert_eq!(
        incumbent
            .prefab(&AssetPath::new("outer.prefab.json").unwrap())
            .unwrap()
            .session_revision(),
        None
    );
}

#[test]
fn newly_known_asset_reference_captures_actual_asset_metadata_bytes_and_database() {
    let project = independent_project();
    let root = SceneEntityRecord::new(SceneEntityId::new("root").unwrap()).with_component(
        ComponentTypeId::new("test.PreparedProject"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([
                ("value", ComponentValue::U64(7)),
                (
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".into())),
                        ("value", ComponentValue::String("alternate.png".into())),
                    ]),
                ),
            ]),
        )
        .with_owner(installed_owner().owner_id().clone()),
    );
    let saved = SceneDocument::from_ordered_roots([root]);
    let saved_bytes = saved.to_json_string().unwrap();
    fs::write(project.root.join(STARTUP), &saved_bytes).unwrap();
    let incumbent = open_project(&project);
    assert_eq!(incumbent.readiness(), AuthoringProjectReadiness::Degraded);
    assert_eq!(incumbent.asset_sources().count(), 0);
    assert!(incumbent.asset_database().is_empty());
    let (mut owner, mut publication) = prepare_owner(&incumbent);
    let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
    assert_eq!(prepared.readiness(), AuthoringProjectReadiness::Complete);
    assert!(!prepared.diagnostics().has_errors());
    assert!(prepared.composition_failure().is_none());
    prepared.validate(&mut owner).unwrap();
    let successor = publish_owner(owner, &mut publication);
    let (receipt, _) = prepared.attach(successor).unwrap();
    let successor = receipt.into_session_snapshot().unwrap();
    assert_eq!(successor.fork_startup_scene().document(), &saved);
    assert_eq!(successor.asset_sources().count(), 1);
    let asset = successor.asset_sources().next().unwrap();
    assert_eq!(asset.path().as_str(), "alternate.png");
    assert_eq!(
        asset.bytes(),
        fs::read(project.root.join("assets/alternate.png")).unwrap()
    );
    assert_eq!(
        asset.source().digest(),
        ContentDigest::of_bytes(asset.bytes())
    );
    let metadata_bytes = fs::read(project.root.join("assets/alternate.png.meta")).unwrap();
    assert_eq!(
        asset.metadata_source().digest(),
        ContentDigest::of_bytes(&metadata_bytes)
    );
    for (path, fact) in [
        ("assets/alternate.png", asset.source()),
        ("assets/alternate.png.meta", asset.metadata_source()),
    ] {
        let file = successor
            .project_root()
            .open_file_exact(&RelativePath::new(path).unwrap())
            .unwrap();
        assert_eq!(fact.identity(), file.identity());
        assert_eq!(fact.parent_identity(), file.parent_identity());
        assert!(successor.source_facts().any(|source| source == fact));
    }
    let database = successor.asset_database();
    assert_eq!(database.len(), 1);
    assert_eq!(database.record_for_path(asset.path()), Some(asset.record()));
    assert_eq!(asset.record().source_kind(), &AssetSourceKind::Image);
    successor.startup_source_validation().validate().unwrap();
    assert_eq!(
        fs::read(project.root.join(STARTUP)).unwrap(),
        saved_bytes.as_bytes()
    );
    assert_eq!(incumbent.asset_sources().count(), 0);
}

#[test]
fn wrong_nominal_project_attachment_retains_the_actual_successor_leaf_and_evidence() {
    let project = independent_project();
    let incumbent = open_project(&project);
    let (mut first, _) = prepare_owner(&incumbent);
    let (mut second, mut publication) = prepare_owner(&incumbent);
    assert_eq!(first.bytes(), second.bytes());
    assert_ne!(first.schema_basis(), second.schema_basis());
    let prepared_first = first.prepare_project(&incumbent, &[]).unwrap();
    let prepared_second = second.prepare_project(&incumbent, &[]).unwrap();
    assert!(matches!(
        prepared_first.validate(&mut second),
        Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch)
    ));
    prepared_second.validate(&mut second).unwrap();
    let successor = publish_owner(second, &mut publication);
    let actual_schema = Arc::clone(&successor.schema);
    let identity = successor.schema.bundle_identity();
    let digest = successor.schema.bundle_digest();
    let basis = successor.basis.clone();
    let published = successor.publication.receipt().published_identity();
    let verified = successor.publication.receipt().verified_content_digest();
    let failure = prepared_first.attach(successor).unwrap_err();
    assert_eq!(failure.prepared.basis, *first.schema_basis());
    assert_eq!(
        failure.prepared.readiness(),
        AuthoringProjectReadiness::Complete
    );
    assert!(failure.prepared.composition_failure().is_none());
    assert!(Arc::ptr_eq(&failure.successor.schema, &actual_schema));
    assert_eq!(failure.successor.basis, basis);
    assert_eq!(
        failure.successor.publication.receipt().published_identity(),
        published
    );
    assert_eq!(
        failure
            .successor
            .publication
            .receipt()
            .verified_content_digest(),
        verified
    );
    let file = incumbent
        .project_root()
        .open_file_exact(&RelativePath::new(BUNDLE).unwrap())
        .unwrap();
    assert_eq!(file.identity(), identity);
    file.verify_digest(digest, 65_536).unwrap();
    let (receipt, evidence) = prepared_second.attach(failure.successor).unwrap();
    let successor = receipt.into_session_snapshot().unwrap();
    assert_eq!(successor.schema.bundle_identity(), identity);
    assert_eq!(successor.authoring_bundle_digest(), digest);
    assert_eq!(successor.fork_startup_scene().schema_basis(), basis);
    assert_eq!(evidence.receipt().published_identity(), Some(identity));
    assert_eq!(evidence.receipt().verified_content_digest(), Some(digest));
}

#[test]
fn failed_saved_source_captures_and_validation_attempts_share_the_owner_budget() {
    let project = independent_project();
    let saved_bytes = fs::read(project.root.join(STARTUP)).unwrap();
    let incumbent = open_project(&project);
    let (mut owner, _) = prepare_owner(&incumbent);
    fs::write(project.root.join(STARTUP), b"{").unwrap();
    let before = owner.budget().usage();
    assert!(matches!(
        owner.prepare_project(&incumbent, &[]),
        Err(AuthoringProjectSourceAdmissionError::SourceFormat { .. })
    ));
    let failed_once = owner.budget().usage();
    assert!(failed_once.decode_work() > before.decode_work());
    assert!(failed_once.aggregate_encoded_bytes() > before.aggregate_encoded_bytes());
    assert!(matches!(
        owner.prepare_project(&incumbent, &[]),
        Err(AuthoringProjectSourceAdmissionError::SourceFormat { .. })
    ));
    let failed_twice = owner.budget().usage();
    assert!(failed_twice.decode_work() > failed_once.decode_work());
    assert!(failed_twice.aggregate_encoded_bytes() > failed_once.aggregate_encoded_bytes());
    fs::write(project.root.join(STARTUP), &saved_bytes).unwrap();
    let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
    prepared.validate(&mut owner).unwrap();
    let validated_once = owner.budget().usage().decode_work();
    prepared.validate(&mut owner).unwrap();
    assert!(owner.budget().usage().decode_work() > validated_once);
    // Bound the next real validation immediately above its accumulated work. A fresh
    // per-validation allowance would incorrectly admit this otherwise unchanged source.
    let mut budget = owner.budget();
    let maximum = budget.usage().decode_work() + 1;
    budget.limits.decode_work = ItemLimit::new(maximum).unwrap();
    owner.retain_capture_budget(budget);
    assert!(matches!(
        prepared.validate(&mut owner),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert!(owner.budget().usage().decode_work() > maximum);
    assert!(matches!(
        owner.prepare_project(&incumbent, &[]),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::DecodeWorkExceeded { maximum: observed }
        )) if observed == maximum
    ));
    assert_eq!(fs::read(project.root.join(STARTUP)).unwrap(), saved_bytes);
    assert_eq!(incumbent.authoring_bundle().owners().len(), 1);
}

#[test]
fn prepared_validation_refuses_source_bundle_manifest_and_parent_observation_drift() {
    for change in ["source", "bundle", "manifest", "parent"] {
        let project = independent_project();
        let incumbent = open_project(&project);
        let (mut owner, publication) = prepare_owner(&incumbent);
        let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
        prepared.validate(&mut owner).unwrap();
        if change == "parent" {
            let before = prepared.startup.source.clone();
            match fs::rename(
                project.root.join("scenes"),
                project.root.join("scenes-original"),
            ) {
                Ok(()) => {}
                #[cfg(windows)]
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    // A retained native handle may prevent the replacement itself. Keep
                    // that custody result distinct from rejecting an observed replacement.
                    assert!(project.root.join("scenes").is_dir());
                    assert!(!project.root.join("scenes-original").exists());
                    prepared.validate(&mut owner).unwrap();
                    drop(prepared);
                    drop(publication);
                    drop(owner);
                    drop(incumbent);
                    fs::rename(
                        project.root.join("scenes"),
                        project.root.join("scenes-original"),
                    )
                    .unwrap();
                    let root =
                        crate::project_tooling::host_fs::writable_directory(&project.root).unwrap();
                    let moved = root
                        .open_file_exact(
                            &RelativePath::new("scenes-original/startup.scene.json").unwrap(),
                        )
                        .unwrap();
                    assert_eq!(moved.identity(), before.identity());
                    moved.verify_digest(before.digest(), 65_536).unwrap();
                    continue;
                }
                Err(error) => panic!("source parent rename failed: {error}"),
            }
            fs::create_dir(project.root.join("scenes")).unwrap();
            fs::rename(
                project.root.join("scenes-original/startup.scene.json"),
                project.root.join(STARTUP),
            )
            .unwrap();
            let file = incumbent
                .project_root()
                .open_file_exact(&RelativePath::new(STARTUP).unwrap())
                .unwrap();
            assert_eq!(file.identity(), before.identity());
            assert_ne!(file.parent_identity(), before.parent_identity());
            file.verify_digest(before.digest(), 65_536).unwrap();
        } else {
            let path = match change {
                "source" => STARTUP,
                "bundle" => BUNDLE,
                "manifest" => "nara.toml",
                _ => unreachable!(),
            };
            let mut bytes = fs::read(project.root.join(path)).unwrap();
            // Preserve the leaf identity and valid syntax, changing only its observation.
            bytes.push(b' ');
            fs::write(project.root.join(path), bytes).unwrap();
        }
        let work_before = owner.budget().usage().decode_work();
        assert!(
            matches!(
                prepared.validate(&mut owner),
                Err(AuthoringProjectSourceAdmissionError::SourceChanged { .. })
            ),
            "{change} drift must invalidate the original review"
        );
        assert!(owner.budget().usage().decode_work() > work_before);
    }
}

#[test]
fn preserved_history_can_expose_a_composition_conflict_and_redo_its_exact_source_repair() {
    for undo_branch in [true, false] {
        let project = independent_project();
        let anchor_id = SceneEntityId::new("anchor").unwrap();
        let plain = PrefabInstance {
            source: AssetRef::path("outer.prefab.json").unwrap(),
            overrides: None,
        };
        let mut anchor = SceneEntityRecord::new(anchor_id.clone());
        anchor.prefab = Some(plain.clone());
        let saved = SceneDocument::from_ordered_roots([anchor]);
        let saved_bytes = saved.to_json_string().unwrap();
        fs::write(project.root.join(STARTUP), &saved_bytes).unwrap();
        let incumbent = open_project(&project);
        let source = AuthoringDocumentSourceFact::from_captured_source(
            incumbent.startup_source(),
            incumbent.startup_native_source(),
        )
        .unwrap();
        let actual_base = incumbent
            .prefab(&AssetPath::new("outer.prefab.json").unwrap())
            .unwrap()
            .native_source()
            .key()
            .clone();
        let declaration = |value| {
            let mut overlay = PrefabOverlay::new(actual_base.clone());
            overlay.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Add,
                target: PrefabEntityTarget::source(SceneEntityId::new("outer").unwrap()),
                component: ComponentTypeId::new("test.PreparedProject"),
                record: SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("value", value)]),
                )
                .with_owner(installed_owner().owner_id().clone()),
                bindings: Vec::new(),
            });
            PrefabInstance {
                source: plain.source.clone(),
                overrides: Some(overlay),
            }
        };
        let bad = declaration(ComponentValue::String("preserved unresolved value".into()));
        let repaired = declaration(ComponentValue::U64(42));
        let change = |expected: &PrefabInstance, value: &PrefabInstance| {
            ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: anchor_id.clone(),
                expected: Some(expected.clone()),
                value: Some(value.clone()),
            }])
        };
        let mut scene = SceneAuthoringSession::from_published_authoring_document(
            incumbent.fork_startup_scene(),
        );
        assert!(scene.apply_authoring_patch(&change(&plain, &bad)).applied);
        assert!(
            scene
                .apply_authoring_patch(&change(&bad, &repaired))
                .applied
        );
        if !undo_branch {
            assert!(scene.undo_authoring().applied);
            assert!(scene.undo_authoring().applied);
        }
        let original = scene.shared_document();
        let revision = scene.revision();
        let content = scene.content_state();
        let history = scene.history_status();
        let (mut owner, mut publication) = prepare_owner(&incumbent);
        let reclassified = scene
            .prepare_schema_reclassification(
                owner.schema_basis(),
                &mut SchemaReclassificationWork::default(),
            )
            .unwrap();
        let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
        let current = owner
            .capture_scene(
                &source,
                borrow_prepared_scene_schema_source(&scene, &reclassified).unwrap(),
                &[],
            )
            .unwrap();
        assert!(current.composition_failure().is_none());
        assert!(!current.diagnostics().has_errors());
        prepared.validate(&mut owner).unwrap();
        let successor = publish_owner(owner, &mut publication);
        current
            .attach(&successor)
            .unwrap_or_else(|_| panic!("the current source must attach"));
        let (receipt, _) = prepared.attach(successor).unwrap();
        let successor = receipt.into_session_snapshot().unwrap();
        scene.commit_schema_reclassification(reclassified).unwrap();
        assert!(Arc::ptr_eq(&scene.shared_document(), &original));
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.content_state(), content);
        assert_eq!(scene.history_status(), history);

        let exposed = if undo_branch {
            scene.undo_authoring()
        } else {
            scene.redo_authoring()
        };
        assert!(exposed.applied);
        assert_eq!(
            scene
                .document()
                .entities
                .iter()
                .find(|entity| entity.id == anchor_id)
                .unwrap()
                .prefab,
            Some(bad)
        );
        let conflict = successor
            .capture_scene_document(&source, &scene, &[])
            .unwrap();
        assert_eq!(conflict.readiness(), AuthoringProjectReadiness::Degraded);
        assert_eq!(
            conflict
                .composition_failure()
                .map(|failure| *failure.reason()),
            Some(PrefabExpansionFailureKind::Component)
        );
        assert!(conflict.diagnostics().has_errors());
        assert!(conflict.preview.is_none());

        assert!(scene.redo_authoring().applied);
        assert_eq!(
            scene
                .document()
                .entities
                .iter()
                .find(|entity| entity.id == anchor_id)
                .unwrap()
                .prefab,
            Some(repaired)
        );
        let restored = successor
            .capture_scene_document(&source, &scene, &[])
            .unwrap();
        assert!(restored.composition_failure().is_none());
        assert!(!restored.diagnostics().has_errors());
        assert_eq!(
            fs::read(project.root.join(STARTUP)).unwrap(),
            saved_bytes.as_bytes()
        );
    }
}

#[test]
fn saved_project_receipt_preserves_a_source_admitted_current_composition_conflict() {
    let project = independent_project();
    let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: None,
    });
    fs::write(
        project.root.join(STARTUP),
        SceneDocument::from_ordered_roots([anchor.clone()])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let incumbent = open_project(&project);
    let base = incumbent
        .prefab(&AssetPath::new("outer.prefab.json").unwrap())
        .unwrap()
        .native_source()
        .key()
        .clone();
    let mut overlay = PrefabOverlay::new(base);
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(SceneEntityId::new("outer").unwrap()),
        component: ComponentTypeId::new("test.PreparedProject"),
        record: SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([(
                "value",
                ComponentValue::String("repairable source intent".into()),
            )]),
        )
        .with_owner(installed_owner().owner_id().clone()),
        bindings: Vec::new(),
    });
    anchor.prefab.as_mut().unwrap().overrides = Some(overlay);
    let saved = SceneDocument::from_ordered_roots([anchor]);
    let bytes = saved.to_json_string().unwrap();
    fs::write(project.root.join(STARTUP), &bytes).unwrap();
    let (mut owner, mut publication) = prepare_owner(&incumbent);
    let prepared = owner.prepare_project(&incumbent, &[]).unwrap();
    assert_eq!(prepared.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(prepared.diagnostics().has_errors());
    assert_eq!(
        prepared
            .composition_failure()
            .map(|failure| *failure.reason()),
        Some(PrefabExpansionFailureKind::Component)
    );
    prepared.validate(&mut owner).unwrap();
    let successor = publish_owner(owner, &mut publication);
    let (receipt, _) = prepared.attach(successor).unwrap();
    let snapshot = receipt.snapshot();
    assert_eq!(snapshot.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(snapshot.startup_compilation_diagnostics().has_errors());
    assert_eq!(
        snapshot
            .startup_composition_failure()
            .map(|failure| *failure.reason()),
        Some(PrefabExpansionFailureKind::Component)
    );
    drop(snapshot);
    let mut successor = receipt.into_session_snapshot().unwrap();
    assert_eq!(successor.readiness(), AuthoringProjectReadiness::Degraded);
    assert!(successor.startup_compilation_diagnostics().has_errors());
    assert_eq!(
        successor
            .startup_composition_failure()
            .map(|failure| *failure.reason()),
        Some(PrefabExpansionFailureKind::Component)
    );
    assert!(successor.take_startup_preview().is_none());
    assert_eq!(successor.fork_startup_scene().document(), &saved);
    assert_eq!(
        successor.startup_source().digest(),
        ContentDigest::of_bytes(bytes.as_bytes())
    );
    assert_eq!(
        fs::read(project.root.join(STARTUP)).unwrap(),
        bytes.as_bytes()
    );
}
