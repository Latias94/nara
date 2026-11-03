//! Canonical context checks and hidden payload grammar spend the original operation allowance.

use super::*;
use crate::authoring::PrefabExtractionWork;
use crate::{
    PrefabAuthoringSession, PrefabComponentIntent, PrefabComponentSetMode, PrefabInstanceBinding,
    PrefabOverlay, SceneAuthoringSession, SceneCompilationLimits, SourceDocumentKey,
    SourceDocumentKind,
};
use nara_core::ByteLimit;
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaCatalog,
};

const DEFAULT_NODES: usize = 8_192;

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ContextReadmission")
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.context-readmission").unwrap()
}

fn record(contextual: bool) -> SceneComponentRecord {
    let mut record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("references", ComponentValue::List(Vec::new()))]),
    )
    .with_owner(owner());
    if contextual {
        record.contextual_defaults.insert(
            ComponentFieldId::new("references"),
            ComponentValue::List(vec![
                ComponentValue::Null,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("root").unwrap(),
                }),
            ]),
        );
    }
    record
}

struct Fixture {
    kind: AuthoringDocumentKind,
    readmission: AuthoringDocumentReadmission,
    bytes: Vec<u8>,
}

impl Fixture {
    fn new(
        kind: AuthoringDocumentKind,
        overlay: bool,
        default_nodes: usize,
        contextual: bool,
    ) -> Self {
        let field = ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("references"),
            "References",
            ComponentFieldPath::from_fields(["references"]),
            ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::EntityRef,
            )),
            ComponentValue::List(vec![ComponentValue::Null; default_nodes]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef);
        let schema = ComponentSchema::new(component(), "Context", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field]);
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
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
        let mut entity = SceneEntityRecord::new(SceneEntityId::new("root").unwrap());
        if overlay {
            let key = SourceDocumentKey::named(
                "2054a1b9-b0b9-4ad1-9d98-c983089e1011".parse().unwrap(),
                "context-base",
                SourceDocumentKind::Prefab,
            )
            .unwrap();
            let mut overrides = PrefabOverlay::new(key);
            overrides.components.push(PrefabComponentIntent::Set {
                mode: PrefabComponentSetMode::Replace,
                target: PrefabEntityTarget::source(SceneEntityId::new("inherited").unwrap()),
                component: component(),
                record: record(contextual),
            });
            entity.prefab = Some(PrefabInstance {
                source: AssetRef::path("prefabs/context-base.prefab.json").unwrap(),
                binding: PrefabInstanceBinding::Forest,
                arguments: None,
                overrides: Some(overrides),
            });
        } else {
            entity.components.insert(component(), record(contextual));
        }
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
                            AssetRef::path("scenes/context.scene.json").unwrap(),
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
                            AssetRef::path("prefabs/context.prefab.json").unwrap(),
                        ),
                        &basis,
                    )
                    .unwrap()
                    .publish();
                let session = PrefabAuthoringSession::from_published_authoring_document(published);
                (session.authoring_readmission().unwrap(), bytes)
            }
        };
        assert!(bytes.len() < 4_096);
        assert_eq!(
            readmission.state.readiness,
            AuthoringDocumentReadiness::Complete
        );
        assert_eq!(readmission.state.record_facts.len(), 1);
        assert!(readmission.state.unavailable_manifest.entries.is_empty());
        assert!(
            readmission.state.record_facts[0]
                .schema_declared_asset_references
                .is_empty()
        );
        assert_eq!(
            matches!(
                &readmission.state.record_facts[0].location,
                AuthoringRecordLocation::OverlayComponent { .. }
            ),
            overlay,
        );
        Self {
            kind,
            readmission,
            bytes,
        }
    }

    fn preserves(
        &self,
        admit: impl FnMut(Option<usize>) -> Result<(), ScenePatchReport>,
    ) -> Result<bool, ScenePatchReport> {
        match self.kind {
            AuthoringDocumentKind::Scene => {
                self.readmission.preserves_scene_candidate_with_admission(
                    SceneDocumentCandidate::decode_json_bytes(&self.bytes).unwrap(),
                    admit,
                )
            }
            AuthoringDocumentKind::Prefab => {
                self.readmission.preserves_prefab_candidate_with_admission(
                    PrefabDocumentCandidate::decode_json_bytes(&self.bytes).unwrap(),
                    admit,
                )
            }
        }
    }

    fn classify(
        &self,
        admit: &mut reclassification::AdmissionCallback<'_>,
    ) -> Result<AuthoringDocumentState, ScenePatchReport> {
        match self.kind {
            AuthoringDocumentKind::Scene => self.readmission.readmit_scene_with_admission(
                SceneDocumentCandidate::decode_json_bytes(&self.bytes).unwrap(),
                admit,
            ),
            AuthoringDocumentKind::Prefab => self.readmission.readmit_prefab_with_admission(
                PrefabDocumentCandidate::decode_json_bytes(&self.bytes).unwrap(),
                admit,
            ),
        }
    }
}

#[test]
fn explicit_empty_reference_lists_charge_unselected_defaults_to_the_original_work_and_retries() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        for overlay in [false, true] {
            let empty = Fixture::new(kind, overlay, 0, false);
            let large = Fixture::new(kind, overlay, DEFAULT_NODES, false);
            assert_eq!(empty.bytes, large.bytes);
            let original = Arc::clone(&large.readmission.state);
            let mut small_work =
                PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap();
            assert!(
                empty
                    .preserves(|bytes| small_work.admit_capture_storage(bytes))
                    .unwrap()
            );
            let mut measured =
                PrefabExtractionWork::new(SceneCompilationLimits::default()).unwrap();
            let initial = measured.usage().work_bytes;
            assert!(
                large
                    .preserves(|bytes| measured.admit_capture_storage(bytes))
                    .unwrap()
            );
            let required = measured.usage().work_bytes;
            assert!(required >= small_work.usage().work_bytes + DEFAULT_NODES);

            for deficit in [0, 1] {
                let limits = SceneCompilationLimits::default()
                    .with_working_bytes(ByteLimit::new(required - deficit).unwrap());
                let mut work = PrefabExtractionWork::new(limits).unwrap();
                let result = large.preserves(|bytes| work.admit_capture_storage(bytes));
                if deficit == 0 {
                    assert!(result.unwrap());
                    assert_eq!(work.usage().work_bytes, required);
                } else {
                    let report = result.unwrap_err();
                    assert!(report.diagnostics.iter().any(|entry| {
                        entry.code().as_str() == "scene.prefab-extraction-capacity"
                    }));
                }
                let spent = work.usage();
                assert!(
                    large
                        .preserves(|bytes| work.admit_capture_storage(bytes))
                        .is_err()
                );
                assert!(work.usage().work_bytes >= spent.work_bytes);
                assert!(Arc::ptr_eq(&original, &large.readmission.state));
            }
            let spent = measured.usage().work_bytes;
            assert!(
                large
                    .preserves(|bytes| measured.admit_capture_storage(bytes))
                    .unwrap()
            );
            assert_eq!(measured.usage().work_bytes - spent, required - initial);
        }
    }
}

#[test]
fn canonical_context_scratch_refusal_stops_direct_and_actual_overlay_classification() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        for overlay in [false, true] {
            let fixture = Fixture::new(kind, overlay, DEFAULT_NODES, false);
            let basis = fixture.readmission.state.schema_basis();
            let schema = basis.schema(&component()).unwrap();
            let mut observations = Vec::new();
            crate::document::validate_contextual_defaults_with_admission(
                schema,
                &record(false),
                SceneFileLimits::default().component_materialization_limits(),
                |nodes, bytes, allocated| {
                    observations.push((nodes, bytes, allocated));
                    Ok::<(), std::convert::Infallible>(())
                },
            )
            .unwrap();
            let scratch: Vec<_> = observations
                .iter()
                .enumerate()
                .filter_map(|(index, (_, _, allocated))| (*allocated > 0).then_some(index))
                .collect();
            assert!(
                scratch.len() >= 2,
                "exercise shape and canonical-value iterator scratch"
            );
            let original = Arc::clone(&fixture.readmission.state);
            for denied in scratch {
                let refusal = authoring_structure_rejection("canonical context scratch refused");
                let mut phase_calls = 0;
                let mut refused = false;
                let result = fixture.classify(&mut |stage, _| {
                    assert!(
                        !refused,
                        "classification continued after its original refusal"
                    );
                    if stage == "classification-contextual-validation" {
                        let current = phase_calls;
                        phase_calls += 1;
                        if current == denied {
                            refused = true;
                            return Err(refusal.clone());
                        }
                    }
                    Ok(())
                });
                assert_eq!(result.unwrap_err(), refusal);
                assert_eq!(phase_calls, denied + 1);
                assert!(refused);
                assert!(Arc::ptr_eq(&original, &fixture.readmission.state));
            }
        }
    }
}

#[test]
fn hidden_contextual_reference_planning_propagates_the_original_refusal_without_downgrading() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        for overlay in [false, true] {
            let fixture = Fixture::new(kind, overlay, DEFAULT_NODES, true);
            let original = Arc::clone(&fixture.readmission.state);
            for denied in [
                "classification-contextual-reference-field",
                "classification-contextual-references",
            ] {
                let mut refusal =
                    authoring_structure_rejection("hidden reference planning refused");
                refusal.inverse = Some(ScenePatchDocument::new([]));
                let mut refused = false;
                let result = fixture.classify(&mut |stage, _| {
                    assert!(
                        !refused,
                        "admission refusal was treated as dormant overlay meaning"
                    );
                    if stage == denied {
                        refused = true;
                        return Err(refusal.clone());
                    }
                    Ok(())
                });
                assert_eq!(result.unwrap_err(), refusal);
                assert!(refused);
                assert!(Arc::ptr_eq(&original, &fixture.readmission.state));
            }
            assert!(fixture.preserves(|_| Ok(())).unwrap());
        }
    }
}
