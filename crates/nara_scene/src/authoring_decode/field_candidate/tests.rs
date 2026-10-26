use super::*;
use crate::{PrefabAuthoringSession, SceneAuthoringHistoryLimits, SceneAuthoringSession};
use nara_identity::{
    PersistentRuntimeId, PersistentRuntimeNamespaceId, PersistentRuntimeReference,
};
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentNumericDomain, ComponentSchemaCatalog,
    ComponentValueKind,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.local.Value")
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.local").unwrap()
}
fn asset(value: &str) -> AssetRef {
    AssetRef::path(value).unwrap()
}
fn field(name: &str, shape: impl Into<ComponentValueShape>) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}
fn reference(target: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(target) })
}
fn record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            ("number", ComponentValue::U64(1)),
            (
                "image",
                crate::patch::asset_ref_value(&asset("textures/before.png")),
            ),
            ("values", ComponentValue::List(vec![ComponentValue::U64(2)])),
        ]),
    )
    .with_owner(owner())
}

fn fixture(kind: AuthoringDocumentKind, degraded: bool) -> (SceneDocument, AuthoringDocumentState) {
    let default_link = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("link"),
        "Link",
        ComponentFieldPath::from_fields(["link"]),
        ComponentValueKind::EntityRef,
        reference("target"),
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::Edit,
        ComponentCapability::EntityRef,
    ]);
    let mut default_number = field("reset", ComponentValueKind::U64);
    default_number.default_value = Some(ComponentValue::U64(4));
    let schema = ComponentSchema::new(component(), "Value", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            field("number", ComponentValueKind::U64)
                .with_numeric_domain(ComponentNumericDomain::U8),
            field("image", ComponentValueKind::AssetRef)
                .with_capability(ComponentCapability::AssetRef),
            field(
                "values",
                ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64)),
            ),
            default_number,
            default_link,
        ]);
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
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let basis = Arc::new(AuthoringClassificationBasis::new(&bundle, &[adapter]).unwrap());
    let mut document = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(id("z")).with_component(component(), record()),
        SceneEntityRecord::new(id("a")).with_component(component(), record()),
        SceneEntityRecord::new(id("target")),
    ]);
    if degraded {
        document
            .entities
            .push(SceneEntityRecord::new(id("unknown")).with_component(
                ComponentTypeId::new("test.future.Value"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion(9),
                    ComponentValue::String("preserve".into()),
                ),
            ));
        document.roots.push(id("unknown"));
        let mut anchor = SceneEntityRecord::new(id("anchor"));
        anchor.prefab = Some(crate::PrefabInstance {
            source: asset("prefabs/base.prefab.json"),
            overrides: Some(crate::PrefabOverlay {
                components: vec![crate::PrefabComponentIntent::Set {
                    mode: crate::PrefabComponentSetMode::Add,
                    target: crate::PrefabEntityTarget::source(id("overlay-only")),
                    component: ComponentTypeId::new("test.future.Overlay"),
                    record: SceneComponentRecord::new(
                        ComponentSchemaVersion(7),
                        ComponentValue::Null,
                    ),
                    bindings: Vec::new(),
                }],
                ..crate::PrefabOverlay::new(
                    crate::SourceDocumentKey::named(
                        "e3818e8d-e2d8-4f67-a82f-55074d3614b2".parse().unwrap(),
                        "test.base",
                        crate::SourceDocumentKind::Prefab,
                    )
                    .unwrap(),
                )
            }),
        });
        document.entities.push(anchor);
        document.roots.push(id("anchor"));
        document.canonicalize();
    }
    let source = AuthoringDocumentSource::new(asset(match kind {
        AuthoringDocumentKind::Scene => "scenes/local.scene.json",
        AuthoringDocumentKind::Prefab => "prefabs/local.prefab.json",
    }));
    let state = classify_scene_document(&document, source, kind, basis, SceneFileLimits::default())
        .unwrap();
    (document, state)
}

fn set(entity: &str, name: &str, value: ComponentValue) -> ScenePatchOperation {
    ScenePatchOperation::SetField {
        entity: id(entity),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new(name),
        value,
    }
}
fn full(
    document: &SceneDocument,
    state: &AuthoringDocumentState,
) -> Result<AuthoringDocumentState, AuthoringDocumentPublicationError> {
    classify_scene_document(
        document,
        state.source.clone(),
        state.kind,
        Arc::clone(&state.basis),
        state.limits,
    )
}
fn assert_same(actual: &AuthoringDocumentState, expected: &AuthoringDocumentState) {
    assert_eq!(actual.source, expected.source);
    assert_eq!(actual.kind, expected.kind);
    assert_eq!(actual.readiness, expected.readiness);
    assert_eq!(actual.record_facts, expected.record_facts);
    assert_eq!(
        actual.overlay_asset_references,
        expected.overlay_asset_references
    );
    assert_eq!(actual.direct_record_facts, expected.direct_record_facts);
    assert_eq!(actual.unavailable_manifest, expected.unavailable_manifest);
    assert_eq!(actual.diagnostics, expected.diagnostics);
    assert_eq!(actual.limits, expected.limits);
    assert!(Arc::ptr_eq(&actual.basis, &expected.basis));
}

#[test]
fn field_candidate_matches_full_classification_for_both_kinds_and_preservation_states() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        for degraded in [false, true] {
            let (document, state) = fixture(kind, degraded);
            let patch = ScenePatchDocument::new([
                set("z", "number", ComponentValue::U64(9)),
                set(
                    "z",
                    "image",
                    crate::patch::asset_ref_value(&asset("textures/after.png")),
                ),
                set(
                    "a",
                    "values",
                    ComponentValue::List(vec![ComponentValue::U64(3), ComponentValue::U64(4)]),
                ),
            ]);
            let edits = field_edits_from_patch(&patch).unwrap();
            let candidate = prepare(document.clone(), &state, &edits).unwrap();
            assert_eq!(
                candidate.touched.len(),
                2,
                "multiple writes reclassify one complete record"
            );
            let expected = full(candidate.document(), &state).unwrap();
            assert!(
                candidate.try_classify().unwrap().is_none(),
                "reference-bearing fields require the document-wide allowance"
            );
            let actual = candidate.classify().unwrap();
            assert_same(&actual, &expected);
            assert_ne!(
                actual.record_facts, state.record_facts,
                "changed asset references must replace the old fact"
            );
            assert_eq!(actual.unavailable_manifest, state.unavailable_manifest);
            assert!(
                candidate
                    .document()
                    .entities
                    .iter()
                    .filter(|entity| entity.components.contains_key(&component()))
                    .all(|entity| entity.components[&component()].value.get("link").is_none()),
                "defaults stay absent in authored source"
            );
            let (changed, inverse) = candidate.into_parts();
            let inverse = field_edits_from_patch(&inverse).unwrap();
            let restored = prepare(changed, &actual, &inverse).unwrap();
            assert_eq!(restored.document(), &document);
            assert_same(&restored.classify().unwrap(), &state);
        }
    }
}

#[test]
fn scalar_only_candidate_retains_local_classification_without_reference_facts() {
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: vec![
                ComponentSchema::new(component(), "Value", ComponentSchemaVersion::ONE)
                    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                    .with_fields([field("number", ComponentValueKind::U64)]),
            ],
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &BTreeSet::new()).unwrap();
    let basis = Arc::new(AuthoringClassificationBasis::new(&bundle, &[adapter]).unwrap());
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("z"))
        .with_component(
            component(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("number", ComponentValue::U64(1))]),
            )
            .with_owner(owner()),
        )]);
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        let state = classify_scene_document(
            &document,
            AuthoringDocumentSource::new(asset("source.json")),
            kind,
            Arc::clone(&basis),
            SceneFileLimits::default(),
        )
        .unwrap();
        let patch = ScenePatchDocument::new([set("z", "number", ComponentValue::U64(9))]);
        let edits = field_edits_from_patch(&patch).unwrap();
        let candidate = prepare(document.clone(), &state, &edits).unwrap();
        let local = candidate
            .try_classify()
            .unwrap()
            .expect("scalar-only edit retains the proven local path");
        assert_same(&local, &full(candidate.document(), &state).unwrap());
        assert!(local.schema_declared_asset_references().next().is_none());
    }
}

#[test]
fn source_local_entity_reference_field_edit_is_atomic_and_round_trips_history_and_reopen() {
    let (document, state) = fixture(AuthoringDocumentKind::Scene, false);
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(PublishedAuthoringSceneDocument {
            document,
            state,
        });
    let original = scene.document().clone();
    let baseline_revision = scene.revision();
    let baseline_history = scene.history_status();

    let missing = AuthoringFieldEdit::single(
        id("z"),
        component(),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("link"),
        reference("missing"),
    )
    .unwrap();
    let missing_report = scene.apply_authoring_field_edit(missing);
    assert!(!missing_report.applied);
    assert!(
        missing_report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "scene.authoring-local-reference-invalid")
    );
    assert_eq!(scene.document(), &original);
    assert_eq!(scene.revision(), baseline_revision);
    assert_eq!(scene.history_status(), baseline_history);

    let persistent = ComponentValue::EntityReference(nara_identity::EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.world").unwrap(),
            PersistentRuntimeId::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        ),
    });
    let persistent_edit = AuthoringFieldEdit::single(
        id("z"),
        component(),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("link"),
        persistent,
    )
    .unwrap();
    let persistent_report = scene.apply_authoring_field_edit(persistent_edit);
    assert!(!persistent_report.applied);
    assert!(persistent_report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authoring-entity-reference-edit-unavailable"
    }));
    assert_eq!(scene.document(), &original);
    assert_eq!(scene.revision(), baseline_revision);
    assert_eq!(scene.history_status(), baseline_history);

    let accepted = AuthoringFieldEdit::single(
        id("z"),
        component(),
        ComponentSchemaVersion::ONE,
        ComponentFieldId::new("link"),
        reference("target"),
    )
    .unwrap();
    let accepted_report = scene.apply_authoring_field_edit(accepted);
    assert!(
        accepted_report.applied,
        "{:#?}",
        accepted_report.diagnostics
    );
    assert_eq!(scene.history_status().undo_depth, 1);
    assert_eq!(
        scene
            .document()
            .entities
            .iter()
            .find(|entity| entity.id == id("z"))
            .unwrap()
            .components[&component()]
            .value
            .get("link")
            .unwrap(),
        &reference("target")
    );
    assert!(scene.undo_authoring().applied);
    assert_eq!(scene.document(), &original);
    assert!(scene.redo_authoring().applied);
    assert_ne!(scene.document(), &original);

    let reopened_candidate = SceneDocumentCandidate::decode_json_bytes(
        scene.document().to_json_string().unwrap().as_bytes(),
    )
    .unwrap();
    let reopened = scene
        .try_reopen_authoring_candidate(reopened_candidate)
        .unwrap();
    assert_eq!(reopened.document(), scene.document());
    assert_eq!(reopened.history_status().undo_depth, 0);
}

#[test]
fn source_local_reference_edit_does_not_publish_a_persistent_inverse() {
    let (mut document, state) = fixture(AuthoringDocumentKind::Scene, false);
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("test.world").unwrap(),
            PersistentRuntimeId::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        ),
    });
    document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("z"))
        .unwrap()
        .components
        .get_mut(&component())
        .unwrap()
        .value
        .set_path(&ComponentFieldPath::from_fields(["link"]), persistent)
        .unwrap();
    let state = full(&document, &state).unwrap();
    let mut scene =
        SceneAuthoringSession::from_published_authoring_document(PublishedAuthoringSceneDocument {
            document: document.clone(),
            state,
        });
    let revision = scene.revision();
    let report = scene.apply_authoring_field_edit(
        AuthoringFieldEdit::single(
            id("z"),
            component(),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("link"),
            reference("target"),
        )
        .unwrap(),
    );
    assert!(!report.applied);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.authoring-entity-reference-edit-unavailable"
    }));
    assert_eq!(scene.document(), &document);
    assert_eq!(scene.revision(), revision);
    assert_eq!(scene.history_status().undo_depth, 0);
}

#[test]
fn field_candidate_keeps_full_classifier_error_order() {
    for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
        let (document, state) = fixture(kind, false);
        let patch = ScenePatchDocument::new([
            set("z", "number", ComponentValue::U64(256)),
            set("a", "number", ComponentValue::String("wrong kind".into())),
        ]);
        let candidate = prepare(
            document.clone(),
            &state,
            &field_edits_from_patch(&patch).unwrap(),
        )
        .unwrap();
        assert_eq!(
            candidate.classify().unwrap_err().diagnostics(),
            full(candidate.document(), &state)
                .unwrap_err()
                .diagnostics()
        );
    }
}

#[test]
fn field_candidate_uses_full_fallback_when_record_authority_cannot_be_proven() {
    let (document, state) = fixture(AuthoringDocumentKind::Scene, true);
    let patch = ScenePatchDocument::new([set("a", "number", ComponentValue::U64(3))]);
    for version in [ComponentSchemaVersion(2), ComponentSchemaVersion::ONE] {
        let mut candidate = prepare(
            document.clone(),
            &state,
            &field_edits_from_patch(&patch).unwrap(),
        )
        .unwrap();
        let record = candidate
            .document
            .entities
            .iter_mut()
            .find(|entity| entity.id == id("a"))
            .unwrap()
            .components
            .get_mut(&component())
            .unwrap();
        record.version = version;
        if version == ComponentSchemaVersion::ONE {
            *record = record
                .clone()
                .with_owner(PersistentComponentSchemaOwnerId::try_new("test.other").unwrap());
        }
        assert!(candidate.try_classify().unwrap().is_none());
        assert_same(
            &candidate.classify().unwrap(),
            &full(candidate.document(), &state).unwrap(),
        );
    }
}

#[test]
fn scene_and_prefab_field_publication_keep_history_and_quota_failure_atomic() {
    for degraded in [false, true] {
        let (document, state) = fixture(AuthoringDocumentKind::Scene, degraded);
        let mut scene = SceneAuthoringSession::from_published_authoring_document(
            PublishedAuthoringSceneDocument {
                document: document.clone(),
                state,
            },
        );
        let (prefab_document, state) = fixture(AuthoringDocumentKind::Prefab, degraded);
        let prefab_document = PrefabDocument {
            roots: prefab_document.roots,
            entities: prefab_document.entities,
        };
        let mut prefab = PrefabAuthoringSession::from_published_authoring_document(
            PublishedAuthoringPrefabDocument {
                document: prefab_document.clone(),
                state,
            },
        );
        let patch = ScenePatchDocument::new([
            set("a", "reset", ComponentValue::U64(11)),
            ScenePatchOperation::SetAssetRefField {
                entity: id("a"),
                component: component(),
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("image"),
                asset_ref: asset("textures/after.png"),
            },
        ]);
        assert!(scene.apply_authoring_patch(&patch).applied);
        assert!(prefab.apply_authoring_patch(&patch).applied);
        for facts in [
            scene.authoring_record_facts().unwrap(),
            prefab.authoring_record_facts().unwrap(),
        ] {
            let fact = facts
                .iter()
                .find(|fact| {
                    matches!(&fact.location,
                AuthoringRecordLocation::EntityComponent { entity } if entity == &id("a"))
                        && fact.component == component()
                })
                .unwrap();
            assert_eq!(fact.schema_declared_asset_references.len(), 1);
            assert_eq!(
                fact.schema_declared_asset_references[0].asset_ref(),
                &asset("textures/after.png")
            );
        }
        assert!(scene.undo_authoring().applied);
        assert!(prefab.undo_authoring().applied);
        assert_eq!(scene.document(), &document);
        assert_eq!(prefab.document(), &prefab_document);
        assert!(scene.redo_authoring().applied);
        assert!(prefab.redo_authoring().applied);
        let rejected = ScenePatchDocument::new([
            set("z", "number", ComponentValue::U64(7)),
            set("a", "number", ComponentValue::U64(256)),
        ]);
        let scene_before = (
            scene.document().clone(),
            scene.revision(),
            scene.content_state(),
            scene.history_status(),
            scene.history_usage(),
        );
        let prefab_before = (
            prefab.document().clone(),
            prefab.revision(),
            prefab.content_state(),
            prefab.history_status(),
            prefab.history_usage(),
        );
        assert!(!scene.apply_authoring_patch(&rejected).applied);
        assert!(!prefab.apply_authoring_patch(&rejected).applied);
        assert_eq!(
            (
                scene.document(),
                scene.revision(),
                scene.content_state(),
                scene.history_status(),
                scene.history_usage()
            ),
            (
                &scene_before.0,
                scene_before.1,
                scene_before.2,
                scene_before.3,
                scene_before.4
            )
        );
        assert_eq!(
            (
                prefab.document(),
                prefab.revision(),
                prefab.content_state(),
                prefab.history_status(),
                prefab.history_usage()
            ),
            (
                &prefab_before.0,
                prefab_before.1,
                prefab_before.2,
                prefab_before.3,
                prefab_before.4
            )
        );
    }
    let limits = SceneAuthoringHistoryLimits::new(8, 16, 128, 4096, 16, 128, 1).unwrap();
    let (document, state) = fixture(AuthoringDocumentKind::Scene, true);
    let mut scene = SceneAuthoringSession::from_published_authoring_document_with_history_limits(
        PublishedAuthoringSceneDocument {
            document: document.clone(),
            state,
        },
        limits,
    );
    let (document_prefab, state) = fixture(AuthoringDocumentKind::Prefab, true);
    let document_prefab = PrefabDocument {
        roots: document_prefab.roots,
        entities: document_prefab.entities,
    };
    let mut prefab = PrefabAuthoringSession::from_published_authoring_document_with_history_limits(
        PublishedAuthoringPrefabDocument {
            document: document_prefab.clone(),
            state,
        },
        limits,
    );
    let patch = ScenePatchDocument::new([set("a", "number", ComponentValue::U64(7))]);
    let scene_before = (
        scene.revision(),
        scene.content_state(),
        scene.history_usage(),
    );
    let prefab_before = (
        prefab.revision(),
        prefab.content_state(),
        prefab.history_usage(),
    );
    let scene_facts = scene.authoring_record_facts().unwrap().to_vec();
    let prefab_facts = prefab.authoring_record_facts().unwrap().to_vec();
    for report in [
        scene.apply_authoring_patch(&patch),
        prefab.apply_authoring_patch(&patch),
    ] {
        assert!(!report.applied);
        assert_eq!(
            report.diagnostics.iter().next().unwrap().code().as_str(),
            "scene.authoring-history-entry-limit-exceeded"
        );
    }
    assert_eq!(scene.document(), &document);
    assert_eq!(prefab.document(), &document_prefab);
    assert_eq!(
        (
            scene.revision(),
            scene.content_state(),
            scene.history_usage()
        ),
        scene_before
    );
    assert_eq!(
        (
            prefab.revision(),
            prefab.content_state(),
            prefab.history_usage()
        ),
        prefab_before
    );
    assert_eq!(scene.authoring_record_facts().unwrap(), scene_facts);
    assert_eq!(prefab.authoring_record_facts().unwrap(), prefab_facts);
    assert!(
        scene
            .authoring_component_schema(&id("a"), &component())
            .is_some()
    );
    assert!(
        prefab
            .authoring_component_schema(&id("a"), &component())
            .is_some()
    );
}

#[test]
fn ordinary_field_edits_reject_selected_wire_limits_without_publishing_either_kind() {
    use nara_core::{ByteLimit, ItemLimit, SerdeShapeLimits};

    // The value is schema-valid and fits the field transaction's 256-node limit. Only the
    // selected whole-file boundary refuses its publication, before document/history mutation.
    let patch = ScenePatchDocument::new([set(
        "a",
        "values",
        ComponentValue::List((0..100).map(ComponentValue::U64).collect()),
    )]);
    for shape_axis in [false, true] {
        for kind in [AuthoringDocumentKind::Scene, AuthoringDocumentKind::Prefab] {
            let (document, mut state) = fixture(kind, true);
            let prefab_document = PrefabDocument {
                roots: document.roots.clone(),
                entities: document.entities.clone(),
            };
            let limits = if shape_axis {
                let shape = state.limits.shape();
                state.limits.with_shape(SerdeShapeLimits::new(
                    shape.depth(),
                    shape.nodes(),
                    ItemLimit::new(20).unwrap(),
                    shape.string_bytes(),
                    shape.total_string_bytes(),
                ))
            } else {
                let base_bytes = [
                    document.to_json_string().unwrap().len(),
                    document.to_ron_string().unwrap().len(),
                    prefab_document.to_json_string().unwrap().len(),
                    prefab_document.to_ron_string().unwrap().len(),
                ]
                .into_iter()
                .max()
                .unwrap();
                state
                    .limits
                    .with_encoded_bytes(ByteLimit::new(base_bytes + 32).unwrap())
            };
            crate::format::validate_scene_publication_shape(&document, limits).unwrap();
            crate::format::validate_prefab_publication_shape(&prefab_document, limits).unwrap();
            state.limits = limits;
            let candidate = prepare(
                document.clone(),
                &state,
                &field_edits_from_patch(&patch).unwrap(),
            )
            .unwrap();
            assert!(
                full(candidate.document(), &state).is_ok(),
                "the final value itself remains semantically valid"
            );
            assert!(
                crate::format::validate_scene_publication_shape(candidate.document(), limits)
                    .is_err()
            );
            drop(candidate);

            macro_rules! assert_refused {
                ($session:ident, $original:expr) => {{
                    let before = (
                        $session.revision(),
                        $session.content_state(),
                        $session.history_status(),
                        $session.history_usage(),
                    );
                    let facts = $session.authoring_record_facts().unwrap().to_vec();
                    let manifest = $session.unavailable_record_manifest().unwrap().clone();
                    let authority = $session.authoring_readmission().unwrap();
                    let report = $session.apply_authoring_patch(&patch);
                    assert!(!report.applied);
                    assert!(report.inverse.is_none());
                    assert_eq!(
                        report.diagnostics.iter().next().unwrap().code().as_str(),
                        "scene.authoring-field-edit-format-invalid"
                    );
                    assert_eq!($session.document(), $original);
                    assert_eq!(
                        (
                            $session.revision(),
                            $session.content_state(),
                            $session.history_status(),
                            $session.history_usage()
                        ),
                        before
                    );
                    assert_eq!($session.authoring_record_facts().unwrap(), facts);
                    assert_eq!($session.unavailable_record_manifest(), Some(&manifest));
                    let after = $session.authoring_readmission().unwrap();
                    assert_eq!(after.source, authority.source);
                    assert_eq!(after.kind, authority.kind);
                    assert_eq!(after.expected_readiness, authority.expected_readiness);
                    assert!(Arc::ptr_eq(&after.basis, &authority.basis));
                }};
            }
            match kind {
                AuthoringDocumentKind::Scene => {
                    let mut session = SceneAuthoringSession::from_published_authoring_document(
                        PublishedAuthoringSceneDocument {
                            document: document.clone(),
                            state,
                        },
                    );
                    assert_refused!(session, &document);
                }
                AuthoringDocumentKind::Prefab => {
                    let mut session = PrefabAuthoringSession::from_published_authoring_document(
                        PublishedAuthoringPrefabDocument {
                            document: prefab_document.clone(),
                            state,
                        },
                    );
                    assert_refused!(session, &prefab_document);
                }
            }
        }
    }
}
