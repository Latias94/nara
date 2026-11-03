use std::fs;

use nara_asset::{AssetMeta, AssetMetaIdentity, StableAssetId};
use nara_core::ItemLimit;

use super::*;
use crate::{
    project_host::{
        authoring::{
            AuthoringProjectAdmissionHost,
            source_inventory::SourceInventory,
            tests::{
                TestAuthoringProject,
                independent_capture::{bound_source_host, independent_project, open_second_scene},
            },
        },
        file_publication::{FilePublicationHost, FilePublicationOutcome},
    },
    test_host_fs as host_fs,
};

const STARTUP: &str = "scenes/startup.scene.json";

fn fixture() -> TestAuthoringProject {
    let project = independent_project();
    // Complete inventory needs valid stored members, including unused ordinary-capture canaries.
    fs::write(
        project.root.join("prefabs/unused.prefab.json"),
        PrefabDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("assets/unused.png.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("0733d45e-4c6b-4c09-b7a2-425b4a351d24").unwrap(),
                AssetPath::new("unused.png").unwrap(),
                AssetSourceKind::Image,
            ),
            (),
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    project
}

fn open(
    project: &TestAuthoringProject,
) -> (
    AuthoringProjectSessionSnapshot,
    SourceInventory,
    AuthoringProjectAdmissionBudget,
) {
    let schema = AuthoringProjectAdmissionHost::default()
        .prepare(host_fs::writable_directory(&project.root).unwrap(), None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(
        &schema.bundle().owners()[0],
        &Default::default(),
    )
    .unwrap();
    let snapshot = PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap();
    let mut budget = snapshot.schema.budget;
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let inventory =
        SourceInventory::capture(&snapshot.schema, &[], snapshot.scene_limits, &mut budget)
            .unwrap();
    (snapshot, inventory, budget)
}

#[test]
fn escaped_startup_validation_keeps_native_custody_after_session_and_inventory_drop() {
    let project = fixture();
    let (mut snapshot, inventory, mut budget) = open(&project);
    assert!(snapshot.source_observation_custody.is_none());
    assert!(
        snapshot
            .startup_source_validation()
            .source_observation_custody
            .is_none()
    );
    snapshot.install_source_observation_custody(inventory.shared_custody());
    let validation = snapshot.startup_source_validation();
    let escaped = validation.clone();
    let original = std::ptr::from_ref(
        validation
            .source_observation_custody
            .as_ref()
            .unwrap()
            .file(STARTUP)
            .unwrap(),
    );
    drop(validation);
    drop(snapshot);
    drop(inventory);

    let held = escaped
        .source_observation_custody
        .as_ref()
        .unwrap()
        .file(STARTUP)
        .unwrap();
    assert_eq!(std::ptr::from_ref(held), original);
    let current = escaped
        .schema
        .project_root
        .open_file_exact(&RelativePath::new(STARTUP).unwrap())
        .unwrap();
    assert!(held.same_object(&current).unwrap());
    escaped.validate_with_budget(&mut budget).unwrap();
    let mut reclamation = Vec::new();
    escaped
        .collect_reclamation_sources(&mut reclamation)
        .unwrap();
    assert!(
        reclamation
            .iter()
            .any(|file| file.same_object(held).unwrap())
    );
}

#[test]
fn transaction_leaf_refuses_a_replacement_that_content_capture_accepts() {
    let project = fixture();
    let (snapshot, inventory, mut budget) = open(&project);
    let root = snapshot.project_root();
    let original = root
        .open_file_exact(&RelativePath::new(STARTUP).unwrap())
        .unwrap();
    let bytes = fs::read(project.root.join(STARTUP)).unwrap();
    let mut publisher = FilePublicationHost::bind_existing(
        &root,
        STARTUP,
        &original,
        ContentDigest::of_bytes(&bytes),
        bytes.len() as u64,
    )
    .unwrap();
    let current = match publisher.publish(&bytes) {
        FilePublicationOutcome::Published(accepted) => accepted.into_parts().1.into_parts().0,
        _ => panic!("the equal-byte replacement must publish"),
    };
    assert!(!original.same_object(&current).unwrap());
    let observed = snapshot.startup_source_validation();
    observed.validate_with_budget(&mut budget).unwrap();
    let guarded = observed.with_source_observation_custody(Some(inventory.shared_custody()));
    assert!(matches!(
        guarded.validate_with_budget(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == STARTUP
    ));
    assert!(
        guarded
            .collect_reclamation_sources(&mut Vec::new())
            .is_err()
    );
}

#[test]
fn transaction_parent_refuses_a_moved_leaf_that_content_capture_accepts() {
    let project = fixture();
    let (snapshot, inventory, mut budget) = open(&project);
    let custody = inventory.shared_custody();
    if let Err(error) = fs::rename(
        project.root.join("scenes"),
        project.root.join("held-scenes"),
    ) {
        assert!(
            cfg!(windows)
                && error.kind() == std::io::ErrorKind::PermissionDenied
                && error.raw_os_error() == Some(5),
            "unexpected directory rename failure: {error}"
        );
        // A native lease can prevent the move; verify the original binding remains intact.
        assert!(project.root.join("scenes").is_dir());
        assert!(project.root.join(STARTUP).is_file());
        assert!(!project.root.join("held-scenes").exists());
        let root = snapshot.project_root();
        let current = root
            .open_file_exact(&RelativePath::new(STARTUP).unwrap())
            .unwrap();
        let parent = root
            .open_directory_exact(&RelativePath::new("scenes").unwrap())
            .unwrap();
        assert!(current.same_object(custody.file(STARTUP).unwrap()).unwrap());
        assert!(
            parent
                .same_object(custody.parent(STARTUP).unwrap())
                .unwrap()
        );
        assert_eq!(
            current.parent_identity(),
            Some(custody.parent(STARTUP).unwrap().identity())
        );
        let bytes = fs::read(project.root.join(STARTUP)).unwrap();
        assert_eq!(
            ContentDigest::of_bytes(&bytes),
            snapshot.startup_source.digest()
        );
        let observed = snapshot.startup_source_validation();
        observed.validate_with_budget(&mut budget).unwrap();
        observed
            .with_source_observation_custody(Some(custody))
            .validate_with_budget(&mut budget)
            .unwrap();
        return;
    }
    fs::create_dir(project.root.join("scenes")).unwrap();
    fs::rename(
        project.root.join("held-scenes/startup.scene.json"),
        project.root.join(STARTUP),
    )
    .unwrap();
    let current = snapshot
        .project_root()
        .open_file_exact(&RelativePath::new(STARTUP).unwrap())
        .unwrap();
    assert!(current.same_object(custody.file(STARTUP).unwrap()).unwrap());
    assert_ne!(
        current.parent_identity(),
        Some(custody.parent(STARTUP).unwrap().identity())
    );
    let observed = snapshot.startup_source_validation();
    observed.validate_with_budget(&mut budget).unwrap();
    let guarded = observed.with_source_observation_custody(Some(custody));
    assert!(matches!(
        guarded.validate_with_budget(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == STARTUP
    ));
}

#[test]
fn installed_custody_keeps_digest_checks_without_expanding_dependency_membership() {
    let project = fixture();
    let (mut snapshot, inventory, mut budget) = open(&project);
    let source_count = snapshot.source_facts.len();
    snapshot.install_source_observation_custody(inventory.shared_custody());
    fs::write(
        project.root.join("prefabs/unused.prefab.json"),
        b"changed unused member",
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/later.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let validation = snapshot.startup_source_validation();
    assert_eq!(validation.sources.len(), source_count);
    validation.validate_with_budget(&mut budget).unwrap();

    let mut bytes = fs::read(project.root.join(STARTUP)).unwrap();
    bytes.push(b'\n');
    fs::write(project.root.join(STARTUP), bytes).unwrap();
    assert!(matches!(
        validation.validate_with_budget(&mut budget),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == STARTUP
    ));
}

#[test]
fn escaped_working_root_keeps_original_custody_and_accepts_only_a_genuine_new_baseline() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_from_bound_source(&source, &session, &[])
        .unwrap();
    let validation = capture.source_validation();
    let original = snapshot
        .project_root()
        .open_file_exact(&RelativePath::new(source.relative_path()).unwrap())
        .unwrap();
    assert!(validation.source_object_in_use(&original).unwrap());
    let bytes = fs::read(project.root.join(source.relative_path())).unwrap();
    // This capture fixture is intentionally read-only. A real external writer replaces the
    // leaf; fresh admission then establishes the new observation in the original root scope.
    fs::rename(
        project.root.join(source.relative_path()),
        project.root.join("retired-root.json"),
    )
    .unwrap();
    fs::write(project.root.join(source.relative_path()), &bytes).unwrap();
    let current = snapshot
        .project_root()
        .open_file_exact(&RelativePath::new(source.relative_path()).unwrap())
        .unwrap();
    let digest = current.digest(bytes.len() as u64).unwrap();
    assert!(!current.same_object(&original).unwrap());
    let current_source = source
        .with_persistence_observation(current.identity(), current.parent_identity(), digest)
        .unwrap();
    let current_capture = snapshot
        .capture_scene_from_bound_source(&current_source, &session, &[])
        .unwrap();
    current_capture.source_validation().validate().unwrap();
    drop(current_capture);
    drop(capture);
    drop(snapshot);
    drop(session);
    // Neither dropping the Session nor accepting a new root can refresh an escaped old token.
    assert!(validation.source_object_in_use(&original).unwrap());
    assert!(matches!(validation.validate(),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == source.relative_path()));
}

#[test]
fn fresh_dependency_content_does_not_inherit_the_old_project_inventory_object_pin() {
    let project = fixture();
    let (mut snapshot, source, session) = open_second_scene(&project);
    let mut budget = snapshot.schema.budget;
    budget.limits.decode_work = ItemLimit::new(200_000_000).unwrap();
    let inventory =
        SourceInventory::capture(&snapshot.schema, &[], snapshot.scene_limits, &mut budget)
            .unwrap();
    snapshot.install_source_observation_custody(inventory.shared_custody());
    let path = "prefabs/leaf.prefab.json";
    let root = snapshot.project_root();
    let original = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    let bytes = fs::read(project.root.join(path)).unwrap();
    fs::rename(
        project.root.join(path),
        project.root.join("retired-leaf.json"),
    )
    .unwrap();
    fs::write(project.root.join(path), &bytes).unwrap();
    let current = root
        .open_file_exact(&RelativePath::new(path).unwrap())
        .unwrap();
    assert!(!current.same_object(&original).unwrap());
    let capture = snapshot
        .capture_scene_from_bound_source(&source, &session, &[])
        .unwrap();
    assert_eq!(capture.source_observations.physical_count(), 1);
    let validation = capture.source_validation();
    assert!(validation.source_observation_custody.is_none());
    drop(capture);
    drop(snapshot);
    drop(inventory);
    drop(session);
    validation.clone().validate().unwrap();
    let mut changed = bytes;
    changed.push(b' ');
    fs::write(project.root.join(path), changed).unwrap();
    assert!(matches!(validation.validate(),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path: changed, .. }) if changed.as_str() == path));
}

#[test]
fn standalone_validation_cannot_spend_an_unrelated_handle_as_its_original_proof_capacity() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let capture = snapshot
        .capture_scene_from_bound_source(&source, &session, &[])
        .unwrap();
    let mut validation = capture.source_validation();
    assert_eq!(validation.source_observations.physical_count(), 1);
    drop(capture);
    drop(snapshot);
    let schema =
        Arc::get_mut(&mut validation.schema).expect("the escaped validation owns its receipt");
    let unrelated = schema
        .project_root
        .open_file_exact(&RelativePath::new("assets/leaf.png").unwrap())
        .unwrap();
    schema.budget.retain_open_handles(1).unwrap();
    schema.budget.limits.peak_open_handles =
        ItemLimit::new(FileSourceObservation::retained_handle_capacity()).unwrap();
    assert!(matches!(
        validation.validate(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. }
        ))
    ));
    drop(unrelated);
}

#[test]
fn observation_factory_admits_transfer_before_open_and_leaves_body_validation_to_the_worker() {
    let project = independent_project();
    let (snapshot, source, session) = open_second_scene(&project);
    let host = bound_source_host(&snapshot, &source).unwrap();
    for exact in [false, true] {
        let mut inputs = snapshot.admit_source_capture_inputs(0, 0).unwrap();
        let before = inputs.budget.usage();
        let cost = host.source_observation_cost().unwrap();
        let persistent = cost.retained_bytes;
        let capacity = cost.retained_handle_capacity;
        let maximum = before.retained_candidate_bytes() + persistent - usize::from(!exact);
        inputs.budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let called = std::cell::Cell::new(false);
        let result = inputs.capture_source_observation(cost, || {
            called.set(true);
            host.capture_source_observation(host.accepted_source_observation().unwrap())
                .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        });
        assert_eq!(called.get(), exact);
        assert_eq!(result.is_ok(), exact);
        assert_eq!(
            inputs.budget.retained_candidate_bytes,
            before.retained_candidate_bytes() + if exact { persistent } else { 0 }
        );
        assert_eq!(
            inputs.budget.retained_open_handles,
            before.retained_open_handles() + if exact { capacity } else { 0 }
        );
    }
    let mut inputs = snapshot.admit_source_capture_inputs(0, 0).unwrap();
    let before = inputs.budget.usage();
    let rejected =
        inputs.capture_source_observation(host.source_observation_cost().unwrap(), || {
            let _temporary = host
                .capture_source_observation(host.accepted_source_observation().unwrap())
                .unwrap();
            Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        });
    assert!(rejected.is_err());
    assert_eq!(
        inputs.budget.retained_candidate_bytes,
        before.retained_candidate_bytes()
    );
    assert_eq!(
        inputs.budget.retained_open_handles,
        before.retained_open_handles()
    );
    assert!(inputs.budget.usage().decode_work() > before.decode_work());
    let mut bytes = fs::read(project.root.join(source.relative_path())).unwrap();
    bytes.push(b' ');
    fs::write(project.root.join(source.relative_path()), bytes).unwrap();
    let mut inputs = snapshot
        .admit_source_capture_inputs(0, source.checked_retained_bytes().unwrap())
        .unwrap();
    let observation = inputs
        .capture_source_observation(host.source_observation_cost().unwrap(), || {
            host.capture_source_observation(host.accepted_source_observation().unwrap())
                .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        })
        .expect("same-object owner handoff does not read the externally changed body");
    assert_eq!(observation.observation().2, source.digest());
    let request = snapshot
        .capture_scene_request(&source, observation, &session, inputs, Box::new([]))
        .unwrap();
    assert!(
        matches!(request.prepare(), Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
        if path.as_str() == source.relative_path())
    );
}

#[test]
fn opened_startup_validation_admits_custody_before_the_factory_without_copying_source_tables() {
    for exact in [false, true] {
        let project = independent_project();
        let (snapshot, _, _) = open_second_scene(&project);
        let source = snapshot.admit_startup_document().unwrap().source().clone();
        let host = bound_source_host(&snapshot, &source).unwrap();
        let mut validation = snapshot.startup_source_validation();
        let sources = Arc::clone(&validation.sources);
        let observations = Arc::clone(&validation.source_observations);
        let cost = host.source_observation_cost().unwrap();
        let maximum = validation.retained_capture_bytes + cost.retained_bytes - usize::from(!exact);
        drop(snapshot);
        Arc::get_mut(&mut validation.schema)
            .expect("the validation exclusively owns the Schema receipt")
            .budget
            .limits
            .retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
        let called = std::cell::Cell::new(false);
        let result = validation.bind_opened_source(&source, cost, || {
            called.set(true);
            host.capture_source_observation(host.accepted_source_observation().unwrap())
                .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        });
        assert_eq!(called.get(), exact);
        if exact {
            let validation = result.expect("the exact retained capacity admits the handoff");
            assert_eq!(validation.retained_capture_bytes, maximum);
            assert_eq!(validation.physical_count().unwrap(), 1);
            assert!(Arc::ptr_eq(&validation.sources, &sources));
            assert!(Arc::ptr_eq(&validation.source_observations, &observations));
            assert_eq!(observations.physical_count(), 0);
        } else {
            assert!(matches!(
                result,
                Err(AuthoringProjectSourceAdmissionError::Budget(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
                ))
            ));
        }
    }
}

#[test]
fn opened_startup_assertion_keeps_original_custody_without_strengthening_saved_content_consumers() {
    let project = fixture();
    let (snapshot, _, _) = open(&project);
    let source = snapshot.admit_startup_document().unwrap().source().clone();
    let mut host = bound_source_host(&snapshot, &source).unwrap();
    let saved = snapshot.startup_source_validation();
    let opened = saved
        .clone()
        .bind_opened_source(&source, host.source_observation_cost().unwrap(), || {
            host.capture_source_observation(host.accepted_source_observation().unwrap())
                .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
        })
        .unwrap();
    opened.clone().validate().unwrap();
    let original = snapshot
        .project_root()
        .open_file_exact(&RelativePath::new(STARTUP).unwrap())
        .unwrap();
    assert!(opened.source_object_in_use(&original).unwrap());
    let bytes = fs::read(project.root.join(STARTUP)).unwrap();
    let current = match host.publish(&bytes) {
        FilePublicationOutcome::Published(accepted) => accepted.into_parts().1.into_parts().0,
        _ => panic!("the equal-byte source replacement must publish"),
    };
    assert!(!original.same_object(&current).unwrap());
    saved.validate().unwrap();
    assert!(opened.collect_reclamation_sources(&mut Vec::new()).is_err());
    drop((snapshot, host));
    assert!(opened.source_object_in_use(&original).unwrap());
    assert!(matches!(
        opened.validate(),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged { path, .. })
            if path.as_str() == STARTUP
    ));
}

#[test]
fn reclamation_admits_its_live_input_and_schema_scratch_before_collecting_exclusions() {
    for opened in [false, true] {
        let project = independent_project();
        let (snapshot, _, _) = open_second_scene(&project);
        let source = snapshot.admit_startup_document().unwrap().source().clone();
        let host = bound_source_host(&snapshot, &source).unwrap();
        let mut validation = snapshot.startup_source_validation();
        if opened {
            validation = validation
                .bind_opened_source(&source, host.source_observation_cost().unwrap(), || {
                    host.capture_source_observation(host.accepted_source_observation().unwrap())
                        .map_err(|_| AuthoringProjectSourceAdmissionError::DocumentSourceMismatch)
                })
                .unwrap();
        }
        validation
            .collect_reclamation_sources(&mut Vec::new())
            .expect("the unchanged source fits the ordinary reclamation allowance");
        let retained = validation.retained_capture_bytes;
        drop(snapshot);
        Arc::get_mut(&mut validation.schema)
            .expect("the validation exclusively owns the Schema receipt")
            .budget
            .limits
            .retained_candidate_bytes = ByteLimit::new(retained).unwrap();
        let mut exclusions = Vec::new();
        assert!(
            validation
                .collect_reclamation_sources(&mut exclusions)
                .is_err()
        );
        assert!(
            exclusions.is_empty(),
            "a full live-input allowance must refuse before opening current exclusions"
        );
    }
}
