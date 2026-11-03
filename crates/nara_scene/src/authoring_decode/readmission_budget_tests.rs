//! Readmission charges selected Schema defaults even when no asset facts retain them.

use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
    ComponentValueKind,
};

use super::*;
use crate::{PrefabAuthoringSession, SceneAuthoringSession, SceneFileEncoding};

const DEFAULT_BYTES: usize = 64 * 1024;

fn fixture(kind: AuthoringDocumentKind) -> (AuthoringDocumentReadmission, Vec<u8>) {
    let component = ComponentTypeId::new("test.ReadmissionScalarDefault");
    let owner = PersistentComponentSchemaOwnerId::try_new("test.readmission-budget").unwrap();
    let schema = ComponentSchema::new(
        component.clone(),
        "Scalar Default",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValueKind::String,
        ComponentValue::String("x".repeat(DEFAULT_BYTES)),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner.clone(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    let basis = AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap();
    let entity = SceneEntityRecord::new(SceneEntityId::new("root").unwrap()).with_component(
        component,
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::Map(BTreeMap::new()),
        )
        .with_owner(owner),
    );
    let (readmission, bytes) = match kind {
        AuthoringDocumentKind::Scene => {
            let bytes = SceneDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap()
                .into_bytes();
            let published = SceneDocumentCandidate::decode_json_bytes(&bytes)
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("scenes/readmission.scene.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish();
            let session = SceneAuthoringSession::from_published_authoring_document(published);
            (session.authoring_readmission().unwrap(), bytes)
        }
        AuthoringDocumentKind::Prefab => {
            let bytes = PrefabDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap()
                .into_bytes();
            let published = PrefabDocumentCandidate::decode_json_bytes(&bytes)
                .unwrap()
                .admit_authoring_with_basis(
                    AuthoringDocumentSource::new(
                        AssetRef::path("prefabs/readmission.prefab.json").unwrap(),
                    ),
                    &basis,
                )
                .unwrap()
                .publish();
            let session = PrefabAuthoringSession::from_published_authoring_document(published);
            (session.authoring_readmission().unwrap(), bytes)
        }
    };
    assert!(bytes.len() < 4096);
    assert_eq!(
        readmission.state.readiness,
        AuthoringDocumentReadiness::Complete
    );
    assert_eq!(readmission.state.record_facts.len(), 1);
    let fact = &readmission.state.record_facts[0];
    assert_eq!(fact.readiness, AuthoringRecordReadiness::Available);
    assert!(fact.schema_declared_asset_references.is_empty());
    assert!(readmission.state.unavailable_manifest.entries.is_empty());
    (readmission, bytes)
}

fn preserves(
    readmission: &AuthoringDocumentReadmission,
    kind: AuthoringDocumentKind,
    bytes: &[u8],
    admit: &mut dyn FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
) -> Result<bool, ScenePatchReport> {
    match kind {
        AuthoringDocumentKind::Scene => readmission.preserves_scene_candidate_with_admission(
            SceneDocumentCandidate::decode_json_bytes(bytes).unwrap(),
            admit,
        ),
        AuthoringDocumentKind::Prefab => readmission.preserves_prefab_candidate_with_admission(
            PrefabDocumentCandidate::decode_json_bytes(bytes).unwrap(),
            admit,
        ),
    }
}

fn assert_default_admission(kind: AuthoringDocumentKind) {
    let (readmission, bytes) = fixture(kind);
    let original = Arc::clone(&readmission.state);
    let refusal = authoring_structure_rejection("Selected default exceeds the original allowance");
    let mut spent = 17_usize;
    let mut refused_stage = None;
    let mut calls_after_default = false;
    let mut admission = |stage, bytes: Option<usize>| {
        assert_ne!(
            stage, "publication-buffer",
            "decoded output must not be re-encoded"
        );
        calls_after_default |= refused_stage.is_some();
        let bytes = bytes.expect("fixture costs must be representable");
        if stage == "classification-defaults" && bytes >= DEFAULT_BYTES {
            refused_stage = Some(stage);
            return Err(refusal.clone());
        }
        spent = spent.checked_add(bytes).unwrap();
        Ok(())
    };
    // Inspect the owning phase at the same private boundary used by the public callbacks.
    // Its refusal stops before the selected default's copy or subsequent value validation.
    let result = match kind {
        AuthoringDocumentKind::Scene => readmission.readmit_scene_with_admission(
            SceneDocumentCandidate::decode_json_bytes(&bytes).unwrap(),
            &mut admission,
        ),
        AuthoringDocumentKind::Prefab => readmission.readmit_prefab_with_admission(
            PrefabDocumentCandidate::decode_json_bytes(&bytes).unwrap(),
            &mut admission,
        ),
    };
    assert_eq!(result.unwrap_err(), refusal);
    assert_eq!(refused_stage, Some("classification-defaults"));
    assert!(!calls_after_default);
    assert!(spent > 17);

    // The byte-only Host interface forwards the same refusal and keeps the caller's original
    // cumulative work. Retrying the same sparse output cannot obtain a fresh allowance.
    for _ in 0..2 {
        let before = spent;
        let mut refused = false;
        let result = preserves(&readmission, kind, &bytes, &mut |bytes| {
            assert!(!refused, "classification continued after admission refusal");
            let bytes = bytes.expect("fixture costs must be representable");
            if bytes >= DEFAULT_BYTES {
                refused = true;
                return Err(refusal.clone());
            }
            spent = spent.checked_add(bytes).unwrap();
            Ok(())
        });
        assert_eq!(result.unwrap_err(), refusal);
        assert!(refused);
        assert!(spent > before);
        assert!(Arc::ptr_eq(&original, &readmission.state));
    }

    let before = spent;
    assert!(
        preserves(&readmission, kind, &bytes, &mut |bytes| {
            spent = spent
                .checked_add(bytes.expect("fixture costs must be representable"))
                .unwrap();
            Ok(())
        })
        .unwrap()
    );
    assert!(spent >= before + DEFAULT_BYTES);
    assert!(Arc::ptr_eq(&original, &readmission.state));
    assert_eq!(
        readmission.state.readiness,
        AuthoringDocumentReadiness::Complete
    );
    match kind {
        AuthoringDocumentKind::Scene => assert!(
            readmission
                .preserves_scene_candidate(
                    SceneDocumentCandidate::decode_json_bytes(&bytes).unwrap(),
                )
                .unwrap()
        ),
        AuthoringDocumentKind::Prefab => assert!(
            readmission
                .preserves_prefab_candidate(
                    PrefabDocumentCandidate::decode_json_bytes(&bytes).unwrap(),
                )
                .unwrap()
        ),
    }
}

#[test]
fn scene_sparse_scalar_default_uses_cumulative_readmission_admission() {
    assert_default_admission(AuthoringDocumentKind::Scene);
}

#[test]
fn prefab_sparse_scalar_default_uses_cumulative_readmission_admission() {
    assert_default_admission(AuthoringDocumentKind::Prefab);
}

#[test]
fn decoded_output_at_its_exact_file_limit_does_not_require_the_other_wire_format() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        let (original, json) = fixture(kind);
        let ron = match kind {
            AuthoringDocumentKind::Scene => SceneDocumentCandidate::decode_json_bytes(&json)
                .unwrap()
                .into_authoring_parts()
                .0
                .to_ron_string()
                .unwrap(),
            AuthoringDocumentKind::Prefab => PrefabDocumentCandidate::decode_json_bytes(&json)
                .unwrap()
                .into_authoring_parts()
                .0
                .to_ron_string()
                .unwrap(),
        }
        .into_bytes();
        assert_ne!(json.len(), ron.len());
        let (bytes, encoding, other_len) = if json.len() < ron.len() {
            (&json, SceneFileEncoding::Json, ron.len())
        } else {
            (&ron, SceneFileEncoding::Ron, json.len())
        };
        let limits =
            SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(bytes.len()).unwrap());
        assert!(other_len > limits.encoded_bytes().get());
        match kind {
            AuthoringDocumentKind::Scene => {
                let candidate = match encoding {
                    SceneFileEncoding::Json => {
                        SceneDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
                    }
                    SceneFileEncoding::Ron => {
                        SceneDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
                    }
                }
                .unwrap();
                let admitted = candidate
                    .clone()
                    .admit_authoring_with_basis(
                        original.state.source.clone(),
                        &original.state.schema_basis(),
                    )
                    .unwrap();
                let readmission = Arc::new(admitted.state).readmission(original.history_limits);
                assert!(
                    readmission
                        .preserves_scene_candidate_with_admission(candidate, |_| Ok(()))
                        .unwrap()
                );
            }
            AuthoringDocumentKind::Prefab => {
                let candidate = match encoding {
                    SceneFileEncoding::Json => {
                        PrefabDocumentCandidate::decode_json_bytes_with_limits(bytes, limits)
                    }
                    SceneFileEncoding::Ron => {
                        PrefabDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits)
                    }
                }
                .unwrap();
                let admitted = candidate
                    .clone()
                    .admit_authoring_with_basis(
                        original.state.source.clone(),
                        &original.state.schema_basis(),
                    )
                    .unwrap();
                let readmission = Arc::new(admitted.state).readmission(original.history_limits);
                assert!(
                    readmission
                        .preserves_prefab_candidate_with_admission(candidate, |_| Ok(()))
                        .unwrap()
                );
            }
        }
    }
}
