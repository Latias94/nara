use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_core::{EngineVersion, FormatGenerator, FormatKind, PersistentFileEnvelope};
use nara_fs::ContentDigest;
use nara_project::{
    ProjectAuthoringRecoveryDigest, ProjectAuthoringState, ProjectManifest, ProjectPath,
};

use super::*;
use crate::project_host::authoring::AuthoredSchemaUpgradeRecovery;

static NEXT: AtomicU64 = AtomicU64::new(1);
const PACKAGE: &str = "pending.operation.json";

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(encoded: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_pending_inspection_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join(PACKAGE), encoded).unwrap();
        let loaded = ProjectManifest::parse_toml_str(
            "schema_version = 1\n[project]\nname = 'Pending inspection'\nsource_universe = 'a05a3bd1-0dfe-4d9f-a8bb-81c29d175f6f'\nnative_document_identity = 'project_location'\n[authoring]\nstate = 'ready'\nschema_bundle = 'schema.json'\n",
        );
        assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
        let mut manifest = loaded.manifest.unwrap();
        let digest = ContentDigest::of_bytes(encoded);
        manifest.authoring.state = ProjectAuthoringState::Pending(
            ProjectAuthoringRecovery::new(
                ProjectPath::new(PACKAGE).unwrap(),
                ProjectAuthoringRecoveryDigest::from_parts(digest.length(), *digest.as_bytes()),
            )
            .unwrap(),
        );
        fs::write(
            root.join("nara.toml"),
            manifest.to_canonical_toml_bytes().unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn inspect(
        &self,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryInspectionError> {
        PendingAuthoringRecoveryInspection::inspect(
            crate::test_host_fs::writable_directory(&self.root).unwrap(),
            limits,
        )
    }

    fn bytes(&self) -> [Vec<u8>; 2] {
        ["nara.toml", PACKAGE].map(|path| fs::read(self.root.join(path)).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let (Ok(root), Ok(temporary)) = (
            self.root.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) {
            if root.parent() == Some(temporary.as_path())
                && root
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("nara_pending_inspection_")
            {
                let _ = fs::remove_dir_all(root);
            }
        }
    }
}

fn package(kind: &str) -> Vec<u8> {
    let version = EngineVersion::parse("0.1.0").unwrap();
    serde_json::to_vec(&PersistentFileEnvelope::canonical_v1(
        FormatKind::new(kind).unwrap(),
        version.clone(),
        FormatGenerator::new("nara", version).unwrap(),
        serde_json::json!({ "not_a_concrete_recovery_payload": [true, null, "opaque"] }),
    ))
    .unwrap()
}

fn hostile_headers(text_bytes: usize) -> impl Iterator<Item = Vec<u8>> {
    let cases: [(&[u8], u8, &[u8]); 3] = [
        (
            br#"{"kind":"prefab_extraction_recovery","format_version":""#,
            0x7f,
            br#"","engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":null}"#,
        ),
        (
            br#"{"kind":""#,
            b'a',
            br#"\/","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":null}"#,
        ),
        (
            br#"{""#,
            b'a',
            br#"":null,"kind":"prefab_extraction_recovery","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":null}"#,
        ),
    ];
    cases.into_iter().map(move |(prefix, repeated, suffix)| {
        let mut encoded = Vec::with_capacity(prefix.len() + text_bytes + suffix.len());
        encoded.extend_from_slice(prefix);
        encoded.resize(prefix.len() + text_bytes, repeated);
        encoded.extend_from_slice(suffix);
        encoded
    })
}

#[test]
fn actual_authenticated_headers_dispatch_both_known_kinds_without_decoding_payload_or_writing() {
    for (kind, expected) in [
        (
            "authored_upgrade_recovery",
            PendingAuthoringRecoveryKind::AuthoredSchemaUpgrade,
        ),
        (
            "prefab_extraction_recovery",
            PendingAuthoringRecoveryKind::PrefabExtraction,
        ),
    ] {
        let fixture = Fixture::new(&package(kind));
        let before = fixture.bytes();
        let inspected = fixture
            .inspect(AuthoringSourceOperationLimits::default())
            .unwrap();
        assert_eq!(inspected.kind(), expected);
        assert_eq!(inspected.reference().recovery_path().as_str(), PACKAGE);
        assert!(inspected.usage().host_work > 0);
        assert_eq!(fixture.bytes(), before);
    }
}

#[test]
fn wrong_known_operation_returns_the_same_inspection_and_spent_allowance_before_payload_decode() {
    let fixture = Fixture::new(&package("prefab_extraction_recovery"));
    let before = fixture.bytes();
    let inspected = fixture
        .inspect(AuthoringSourceOperationLimits::default())
        .unwrap();
    let reference = inspected.reference().clone();
    let usage = inspected.usage();
    let failure = AuthoredSchemaUpgradeRecovery::from_inspection(inspected, None).unwrap_err();
    let (Some(inspected), _) = failure.into_parts() else {
        panic!("the mismatched known kind retains its original transport");
    };
    assert_eq!(
        inspected.kind(),
        PendingAuthoringRecoveryKind::PrefabExtraction
    );
    assert_eq!(inspected.reference(), &reference);
    assert_eq!(inspected.usage(), usage);
    assert_eq!(fixture.bytes(), before);
}

#[test]
fn digest_failure_precedes_kind_and_authenticated_duplicate_keys_fail_shape_admission() {
    let fixture = Fixture::new(&package("unknown_recovery"));
    fs::write(
        fixture.root.join(PACKAGE),
        package("prefab_extraction_recovery"),
    )
    .unwrap();
    let error = fixture
        .inspect(AuthoringSourceOperationLimits::default())
        .unwrap_err();
    assert_eq!(
        error.kind(),
        PendingAuthoringRecoveryInspectionErrorKind::Transport
    );
    let duplicated = String::from_utf8(package("prefab_extraction_recovery"))
        .unwrap()
        .replacen('{', "{\"kind\":\"authored_upgrade_recovery\",", 1);
    let fixture = Fixture::new(duplicated.as_bytes());
    let before = fixture.bytes();
    assert_eq!(
        fixture
            .inspect(AuthoringSourceOperationLimits::default())
            .unwrap_err()
            .kind(),
        PendingAuthoringRecoveryInspectionErrorKind::Structure
    );
    assert_eq!(fixture.bytes(), before);
}

#[test]
fn exact_work_and_one_short_use_the_actual_transport_and_never_modify_pending() {
    let fixture = Fixture::new(&package("prefab_extraction_recovery"));
    let before = fixture.bytes();
    let probe = fixture
        .inspect(AuthoringSourceOperationLimits::default())
        .unwrap();
    let required = probe.usage().host_work;
    drop(probe);
    let exact =
        AuthoringSourceOperationLimits::default().with_host_work(ItemLimit::new(required).unwrap());
    assert_eq!(fixture.inspect(exact).unwrap().usage().host_work, required);
    let short = exact.with_host_work(ItemLimit::new(required - 1).unwrap());
    let error = fixture.inspect(short).unwrap_err();
    assert!(error.host_work_limit_exceeded());
    assert!(!error.open_handles_limit_exceeded());
    assert_eq!(fixture.bytes(), before);
}

#[test]
fn hostile_large_headers_refuse_storage_before_owned_header_or_error_allocation() {
    let limits = AuthoringSourceOperationLimits::default()
        .with_host_work(ItemLimit::new(1_000_000_000).unwrap());
    for encoded in hostile_headers(16 * 1024 * 1024) {
        assert!(encoded.len() < PACKAGE_BYTES);
        assert!(
            encoded.len() * HEADER_TEXT_COPIES + HEADER_FIXED_BYTES
                > AuthoringProjectAdmissionLimits::default()
                    .retained_candidate_bytes()
                    .get()
        );
        let fixture = Fixture::new(&encoded);
        let before = fixture.bytes();
        let error = fixture.inspect(limits).unwrap_err();
        assert_eq!(
            error.kind(),
            PendingAuthoringRecoveryInspectionErrorKind::Admission
        );
        assert!(matches!(
            error.detail,
            InspectionFailure::Admission(
                AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
            )
        ));
        assert_eq!(fixture.bytes(), before);
    }
}

#[test]
fn small_hostile_headers_and_invalid_structure_refund_scratch_after_errors_drop() {
    let mut budget =
        AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default());
    budget.charge_source_bytes(0, 0, 37, 321).unwrap();
    budget.retain_open_handles(3).unwrap();
    for encoded in hostile_headers(128) {
        let before = budget.usage();
        let error = classify(&encoded, &mut budget).unwrap_err();
        assert_eq!(
            error.kind(),
            PendingAuthoringRecoveryInspectionErrorKind::Header
        );
        assert_eq!(
            budget.retained_candidate_bytes,
            before.retained_candidate_bytes()
        );
        assert_eq!(budget.retained_open_handles, before.retained_open_handles());
        assert_eq!(
            budget.decode_work,
            before.decode_work() + encoded.len() * (1 + HEADER_TEXT_COPIES)
        );
    }
    let encoded = br#"{"kind":"prefab_extraction_recovery","kind":"authored_upgrade_recovery"}"#;
    let before = budget.usage();
    assert_eq!(
        classify(encoded, &mut budget).unwrap_err().kind(),
        PendingAuthoringRecoveryInspectionErrorKind::Structure
    );
    assert_eq!(
        budget.retained_candidate_bytes,
        before.retained_candidate_bytes()
    );
    assert_eq!(budget.retained_open_handles, before.retained_open_handles());
    assert_eq!(budget.decode_work, before.decode_work() + encoded.len());
}

#[test]
fn each_phase_work_refusal_refunds_only_unallocated_scratch_and_keeps_prefix_work() {
    let encoded = package("prefab_extraction_recovery");
    let shape_work = encoded.len();
    let header_work = encoded.len() * HEADER_TEXT_COPIES;
    for (maximum, charged) in [
        (37 + shape_work - 1, shape_work),
        (37 + shape_work + header_work - 1, shape_work + header_work),
    ] {
        let mut budget =
            AuthoringProjectAdmissionBudget::new(AuthoringProjectAdmissionLimits::default())
                .for_source_operation(
                    ItemLimit::new(maximum).unwrap(),
                    ItemLimit::new(64).unwrap(),
                );
        budget.charge_source_bytes(0, 0, 37, 321).unwrap();
        budget.retain_open_handles(3).unwrap();
        let error = classify(&encoded, &mut budget).unwrap_err();
        assert!(error.host_work_limit_exceeded());
        assert_eq!(budget.retained_candidate_bytes, 321);
        assert_eq!(budget.retained_open_handles, 3);
        assert_eq!(budget.decode_work, 37 + charged);
    }
}

#[test]
fn header_storage_boundary_preserves_incumbents_and_both_phase_work_charges() {
    let mut encoded = package("prefab_extraction_recovery");
    encoded.extend(std::iter::repeat_n(b' ', 4_096));
    let retained = 321 + encoded.len();
    let header_scratch = encoded.len() * HEADER_TEXT_COPIES + HEADER_FIXED_BYTES;
    for exact in [false, true] {
        let maximum = retained + header_scratch - usize::from(!exact);
        let limits = AuthoringProjectAdmissionLimits::default()
            .with_retained_candidate_bytes(ByteLimit::new(maximum).unwrap());
        let mut budget = AuthoringProjectAdmissionBudget::new(limits);
        budget.charge_source_bytes(0, 0, 37, retained).unwrap();
        budget.retain_open_handles(3).unwrap();
        let result = classify(&encoded, &mut budget);
        if exact {
            assert_eq!(
                result.unwrap(),
                PendingAuthoringRecoveryKind::PrefabExtraction
            );
        } else {
            assert!(matches!(
                result.unwrap_err().detail,
                InspectionFailure::Admission(
                    AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded { .. }
                )
            ));
        }
        assert_eq!(budget.retained_candidate_bytes, retained);
        assert_eq!(budget.retained_open_handles, 3);
        assert_eq!(
            budget.decode_work,
            37 + encoded.len() * (1 + HEADER_TEXT_COPIES)
        );
    }
}

#[test]
fn larger_opaque_payload_fits_independent_phase_reservations_without_semantic_decode() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&package("prefab_extraction_recovery")).unwrap();
    value["payload"] = serde_json::Value::String("x".repeat(8 * 1024 * 1024));
    let encoded = serde_json::to_vec(&value).unwrap();
    drop(value);
    let fixture = Fixture::new(&encoded);
    let before = fixture.bytes();
    let limits = AuthoringSourceOperationLimits::default()
        .with_host_work(ItemLimit::new(1_000_000_000).unwrap());
    let inspected = fixture.inspect(limits).unwrap();
    assert_eq!(
        inspected.kind(),
        PendingAuthoringRecoveryKind::PrefabExtraction
    );
    assert_eq!(inspected.limits(), limits);
    let (loaded, budget, _) = inspected.into_parts();
    assert_eq!(loaded.encoded_bytes(), encoded.as_slice());
    // Neither phase's scratch survives the concrete handoff.
    assert!(budget.retained_candidate_bytes < encoded.len() + 64 * 1024);
    assert_eq!(fixture.bytes(), before);
}
