//! Actual project Input observations, shared identity admission and finite temporary storage.

use std::fs;

use nara_asset::{AssetMeta, AssetMetaIdentity, StableAssetId};
use nara_core::ItemLimit;
use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
use nara_input::ActionMapFileLimits;

use super::*;
use crate::project_host::authoring::tests::{
    TestAuthoringProject,
    independent_capture::{independent_project, open_second_scene},
};
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
};

const INPUT_PATH: &str = "controls/player.actions.json";
const INPUT_METADATA: &str = "controls/player.actions.json.meta";
const INPUT_ID: &str = "919617ea-c89c-4f83-9cce-b084de435a83";
const INPUT_GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/crates/nara_input/tests/fixtures/formats/v1/action-map.json"
));

#[test]
fn startup_retains_the_actual_input_fact_and_optional_sidecar_once() {
    for metadata in [false, true] {
        let project = fixture();
        select_input(&project);
        if metadata {
            write_metadata(&project, INPUT_METADATA, INPUT_PATH, INPUT_ID, input_kind());
        }
        let snapshot = admit(&project);
        let expected = snapshot
            .schema
            .project_root
            .open_file_exact(&RelativePath::new(INPUT_PATH).unwrap())
            .unwrap();
        let facts = snapshot.source_facts().collect::<Vec<_>>();
        let inputs = facts
            .iter()
            .filter(|fact| fact.kind() == AuthoringSourceKind::InputActionMap)
            .collect::<Vec<_>>();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].project_path().as_str(), INPUT_PATH);
        assert_eq!(inputs[0].identity(), expected.identity());
        assert_eq!(inputs[0].parent_identity(), expected.parent_identity());
        assert_eq!(
            inputs[0].digest(),
            ContentDigest::of_bytes(INPUT_GOLDEN.as_bytes())
        );
        assert_eq!(
            facts
                .iter()
                .filter(|fact| fact.project_path().as_str() == INPUT_METADATA)
                .count(),
            usize::from(metadata)
        );
        snapshot.startup_source_validation().validate().unwrap();
    }
}

#[test]
fn ordinary_scene_capture_does_not_activate_the_selected_project_input() {
    let project = fixture();
    select_input(&project);
    let (snapshot, source, session) = open_second_scene(&project);
    fs::write(
        project.root.join(INPUT_PATH),
        b"invalid global input canary",
    )
    .unwrap();
    let capture = snapshot
        .capture_scene_document(&source, &session, &[])
        .unwrap();
    assert!(
        capture
            .source_facts()
            .all(|fact| fact.kind() != AuthoringSourceKind::InputActionMap)
    );
    capture.source_validation().validate().unwrap();
    assert!(snapshot.startup_source_validation().validate().is_err());
}

#[test]
fn source_validation_rejects_changed_input_and_present_metadata_observations() {
    for metadata in [false, true] {
        let project = fixture();
        select_input(&project);
        if metadata {
            write_metadata(&project, INPUT_METADATA, INPUT_PATH, INPUT_ID, input_kind());
        }
        let snapshot = admit(&project);
        let (path, kind) = if metadata {
            (INPUT_METADATA, AuthoringSourceKind::AssetMetadata)
        } else {
            (INPUT_PATH, AuthoringSourceKind::InputActionMap)
        };
        let bytes = fs::read_to_string(project.root.join(path)).unwrap();
        fs::write(project.root.join(path), format!("{bytes} ")).unwrap();
        assert!(matches!(
            snapshot.startup_source_validation().validate(),
            Err(AuthoringProjectSourceAdmissionError::SourceChanged {
                kind: actual, path: actual_path
            })
                if actual == kind && actual_path.as_str() == path
        ));
    }
}

#[test]
fn a_new_optional_sidecar_invalidates_the_observed_absence() {
    let project = fixture();
    select_input(&project);
    let snapshot = admit(&project);
    write_metadata(&project, INPUT_METADATA, INPUT_PATH, INPUT_ID, input_kind());
    assert!(matches!(
        snapshot.startup_source_validation().validate(),
        Err(AuthoringProjectSourceAdmissionError::SourceChanged {
            kind: AuthoringSourceKind::AssetMetadata, path
        })
            if path.as_str() == INPUT_METADATA
    ));
}

#[test]
fn unrelated_resource_metadata_cannot_claim_the_intrinsic_input_uuid() {
    for after_capture in [false, true] {
        let project = fixture();
        select_input(&project);
        let snapshot = after_capture.then(|| admit(&project));
        fs::write(
            project.root.join("assets/conflict.png"),
            b"unrelated source",
        )
        .unwrap();
        write_metadata(
            &project,
            "assets/conflict.png.meta",
            "conflict.png",
            INPUT_ID,
            AssetSourceKind::Image,
        );
        let result = match snapshot {
            Some(snapshot) => snapshot.startup_source_validation().validate(),
            None => prepared(&project).admit_startup_closure().map(|_| ()),
        };
        assert!(matches!(
            result,
            Err(AuthoringProjectSourceAdmissionError::AssetIdentity { .. })
        ));
    }
}

#[test]
fn input_sidecar_requires_canonical_path_source_uuid_and_kind_agreement() {
    for (path, id, kind) in [
        ("wrong/actions.json", INPUT_ID, input_kind()),
        (
            INPUT_PATH,
            "30f80f44-be66-45a4-8edf-5087adb117cb",
            input_kind(),
        ),
        (INPUT_PATH, INPUT_ID, AssetSourceKind::Scene),
    ] {
        let project = fixture();
        select_input(&project);
        write_metadata(&project, INPUT_METADATA, path, id, kind);
        assert!(matches!(
            prepared(&project).admit_startup_closure(),
            Err(AuthoringProjectSourceAdmissionError::IntrinsicSourceMetadataMismatch { .. })
        ));
    }
}

#[test]
fn a_configured_assets_scope_can_bind_the_same_canonical_input_metadata_path() {
    let project = fixture();
    select_input(&project);
    let manifest_path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap()
        .replace(INPUT_PATH, "assets/player.actions.json");
    fs::write(manifest_path, manifest).unwrap();
    fs::write(
        project.root.join("assets/player.actions.json"),
        INPUT_GOLDEN,
    )
    .unwrap();
    write_metadata(
        &project,
        "assets/player.actions.json.meta",
        "player.actions.json",
        INPUT_ID,
        input_kind(),
    );
    let snapshot = admit(&project);
    assert!(
        snapshot
            .source_facts()
            .any(|fact| fact.project_path().as_str() == "assets/player.actions.json.meta")
    );
    snapshot.startup_source_validation().validate().unwrap();
}

#[test]
fn input_cannot_alias_an_already_admitted_scene_file_object() {
    let project = fixture();
    select_input(&project);
    let path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&path)
        .unwrap()
        .replace(INPUT_PATH, "scenes/startup.scene.json");
    fs::write(path, manifest).unwrap();
    assert!(matches!(
        prepared(&project).admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict { .. })
    ));
}

#[test]
fn startup_uses_the_strict_input_format_owner() {
    let project = fixture();
    select_input(&project);
    fs::write(
        project.root.join(INPUT_PATH),
        br#"{"bindings":[],"disabled_contexts":[]}"#,
    )
    .unwrap();
    assert!(matches!(
        prepared(&project).admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::InputFormat { .. })
    ));
}

#[test]
fn temporary_input_compilation_is_reserved_before_parsing_and_not_retained_in_the_snapshot() {
    let project = fixture();
    let baseline = admit(&project).usage();
    let temporary = ActionMapFileLimits::default()
        .predecode_work_bytes()
        .unwrap();
    select_input(&project);
    let snapshot = admit(&project);
    let complete = snapshot.usage();
    assert!(complete.retained_candidate_bytes() - baseline.retained_candidate_bytes() < temporary);

    // Isolate the real Input acquisition boundary from later preview-store reservations.
    // Its pristine Scene/Schema identities remain the production capture inputs.
    let mut budget = snapshot.schema.budget;
    let basis = snapshot.startup_scene.schema_basis();
    let mut acquisition = prefabs::AuthoringPrefabAcquisition::new(
        &snapshot.schema,
        snapshot.scene_limits,
        &basis,
        Arc::clone(&snapshot.startup_source),
        &snapshot.startup_native_source,
        &mut budget,
        &[],
    )
    .unwrap();
    resize_compiler_work(acquisition.budget, &mut acquisition.compiler_reservation, 0).unwrap();
    let maximum = acquisition.budget.retained_candidate_bytes + temporary - 1;
    acquisition.budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    fs::write(
        project.root.join(INPUT_PATH),
        b"invalid JSON should remain unparsed",
    )
    .unwrap();
    assert!(matches!(
        input::capture(
            &mut acquisition,
            &ProjectPath::new(INPUT_PATH).unwrap(),
            &mut ProjectAssetDatabase::default(),
        ),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
}

#[test]
fn input_read_memory_is_admitted_before_the_buffer_or_source_fact_is_allocated() {
    let project = fixture();
    select_input(&project);
    let snapshot = admit(&project);
    let mut budget = snapshot.schema.budget;
    let basis = snapshot.startup_scene.schema_basis();
    let mut acquisition = prefabs::AuthoringPrefabAcquisition::new(
        &snapshot.schema,
        snapshot.scene_limits,
        &basis,
        Arc::clone(&snapshot.startup_source),
        &snapshot.startup_native_source,
        &mut budget,
        &[],
    )
    .unwrap();
    resize_compiler_work(acquisition.budget, &mut acquisition.compiler_reservation, 0).unwrap();
    let before = acquisition.budget.usage();
    let maximum = before.retained_candidate_bytes() + 1;
    acquisition.budget.limits.retained_candidate_bytes = ByteLimit::new(maximum).unwrap();
    let mut bytes = INPUT_GOLDEN.as_bytes().to_vec();
    bytes.resize(ActionMapFileLimits::default().encoded_bytes().get(), b' ');
    fs::write(project.root.join(INPUT_PATH), bytes).unwrap();
    assert!(matches!(
        input::capture(
            &mut acquisition,
            &ProjectPath::new(INPUT_PATH).unwrap(),
            &mut ProjectAssetDatabase::default(),
        ),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
        ))
    ));
    let after = acquisition.budget.usage();
    assert_eq!(
        after.aggregate_encoded_bytes(),
        before.aggregate_encoded_bytes()
    );
    assert_eq!(after.unique_files(), before.unique_files());
}

#[test]
fn input_and_sidecar_aggregate_short_reads_preserve_the_budget_error() {
    for sidecar in [false, true] {
        let project = fixture();
        select_input(&project);
        if sidecar {
            write_metadata(&project, INPUT_METADATA, INPUT_PATH, INPUT_ID, input_kind());
        }
        let snapshot = admit(&project);
        let mut budget = snapshot.schema.budget;
        let basis = snapshot.startup_scene.schema_basis();
        let mut acquisition = prefabs::AuthoringPrefabAcquisition::new(
            &snapshot.schema,
            snapshot.scene_limits,
            &basis,
            Arc::clone(&snapshot.startup_source),
            &snapshot.startup_native_source,
            &mut budget,
            &[],
        )
        .unwrap();
        resize_compiler_work(acquisition.budget, &mut acquisition.compiler_reservation, 0).unwrap();
        let mut incoming = fs::metadata(project.root.join(INPUT_PATH)).unwrap().len() as usize;
        if sidecar {
            incoming += fs::metadata(project.root.join(INPUT_METADATA))
                .unwrap()
                .len() as usize;
        }
        let maximum = acquisition.budget.usage().aggregate_encoded_bytes() + incoming - 1;
        acquisition.budget.limits.aggregate_encoded_bytes = ByteLimit::new(maximum).unwrap();
        assert!(
            matches!(
                input::capture(
                    &mut acquisition,
                    &ProjectPath::new(INPUT_PATH).unwrap(),
                    &mut ProjectAssetDatabase::default(),
                ),
                Err(AuthoringProjectSourceAdmissionError::Budget(
                    AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { .. }
                ))
            ),
            "aggregate exhaustion must retain its meaning for sidecar={sidecar}"
        );
    }
}

#[test]
fn project_input_obeys_aggregate_file_and_encoded_sentinel_limits() {
    let project = fixture();
    select_input(&project);
    let usage = admit(&project).usage();
    let exact = usage.aggregate_encoded_bytes();
    let mut allowed = prepared(&project);
    Arc::get_mut(&mut allowed.schema)
        .unwrap()
        .budget
        .limits
        .aggregate_encoded_bytes = ByteLimit::new(exact).unwrap();
    allowed.admit_startup_closure().unwrap();
    let mut short = prepared(&project);
    Arc::get_mut(&mut short.schema)
        .unwrap()
        .budget
        .limits
        .aggregate_encoded_bytes = ByteLimit::new(exact - 1).unwrap();
    assert!(matches!(
        short.admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::AggregateEncodedBytesExceeded { .. }
        ))
    ));
    let files = usage.unique_files();
    let mut allowed_files = prepared(&project);
    Arc::get_mut(&mut allowed_files.schema)
        .unwrap()
        .budget
        .limits
        .unique_files = ItemLimit::new(files).unwrap();
    allowed_files.admit_startup_closure().unwrap();
    let mut short_files = prepared(&project);
    Arc::get_mut(&mut short_files.schema)
        .unwrap()
        .budget
        .limits
        .unique_files = ItemLimit::new(files - 1).unwrap();
    assert!(matches!(
        short_files.admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::Budget(
            AuthoringProjectAdmissionError::UniqueFileLimitExceeded { .. }
        ))
    ));
    let mut bytes = INPUT_GOLDEN.as_bytes().to_vec();
    bytes.resize(
        ActionMapFileLimits::default().encoded_bytes().get() + 1,
        b' ',
    );
    fs::write(project.root.join(INPUT_PATH), bytes).unwrap();
    assert!(matches!(
        prepared(&project).admit_startup_closure(),
        Err(AuthoringProjectSourceAdmissionError::SourceTooLarge {
            kind: AuthoringSourceKind::InputActionMap,
            ..
        })
    ));
}

fn fixture() -> TestAuthoringProject {
    let project = independent_project();
    // Startup Input checks global identity, including otherwise unused, valid metadata.
    write_metadata(
        &project,
        "assets/unused.png.meta",
        "unused.png",
        "cb03097d-6f19-45d1-a653-fef4258fc762",
        AssetSourceKind::Image,
    );
    project
}

fn select_input(project: &TestAuthoringProject) {
    fs::create_dir_all(project.root.join("controls")).unwrap();
    fs::write(project.root.join(INPUT_PATH), INPUT_GOLDEN).unwrap();
    let path = project.root.join("nara.toml");
    let manifest = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        format!("{manifest}\n[input]\naction_map = '{INPUT_PATH}'\n"),
    )
    .unwrap();
}

fn input_kind() -> AssetSourceKind {
    AssetSourceKind::Other("input-action-map".to_owned())
}

fn write_metadata(
    project: &TestAuthoringProject,
    path: &str,
    source: &str,
    id: &str,
    kind: AssetSourceKind,
) {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(id).unwrap(),
            AssetPath::new(source).unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    fs::write(project.root.join(path), metadata).unwrap();
}

fn prepared(project: &TestAuthoringProject) -> PreparedAuthoringProject {
    let schema = AuthoringProjectAdmissionHost::new(AuthoringProjectAdmissionLimits::default())
        .prepare(capability(project), None)
        .unwrap()
        .admit_bundle()
        .unwrap()
        .publish();
    let adapter = AuthoringAdapterReceipt::from_schema_owner(
        &schema.bundle().owners()[0],
        &Default::default(),
    )
    .unwrap();
    PreparedAuthoringProject::new(schema, vec![adapter].into_boxed_slice())
}

fn admit(project: &TestAuthoringProject) -> AuthoringProjectSessionSnapshot {
    prepared(project)
        .admit_startup_closure()
        .unwrap()
        .publish()
        .into_session_snapshot()
        .unwrap()
}

fn capability(project: &TestAuthoringProject) -> DirectoryCapability {
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(&project.root)
            .unwrap()
    };
    #[cfg(target_os = "linux")]
    let file = fs::File::open(&project.root).unwrap();
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap()
}
