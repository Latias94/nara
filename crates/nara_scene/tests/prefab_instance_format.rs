#![cfg(feature = "serde")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
use nara_ecs::Component;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentNumericBound, ComponentNumericInterval, ComponentRegistry,
    ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringRecordLocation, PrefabAuthoringSession, PrefabDocument,
    PrefabDocumentCandidate, PrefabInstance, SceneAuthoringSession, SceneComponentRecord,
    SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneFileBudgetKind,
    SceneFileLimits, SceneFormatError, ScenePatchDocument, ScenePatchDocumentCandidate,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner, UnavailableRecordManifestEntry,
};
use serde::Serialize;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ExactSnapshot")
}

fn instance(patch: ScenePatchDocument) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: patch,
    }
}

fn replace(expected: Option<PrefabInstance>, value: Option<PrefabInstance>) -> ScenePatchOperation {
    ScenePatchOperation::SetPrefabInstance {
        entity: id("target"),
        expected,
        value,
    }
}

fn source_document(patch: ScenePatchDocument) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("source-anchor"));
    anchor.prefab = Some(instance(patch));
    SceneDocument::from_ordered_roots([anchor])
}

#[derive(Clone, Copy, Debug)]
enum Encoding {
    Json,
    Ron,
}

impl Encoding {
    fn patch(self, patch: &ScenePatchDocument) -> Vec<u8> {
        match self {
            Self::Json => patch.to_json_string().unwrap().into_bytes(),
            Self::Ron => patch.to_ron_string().unwrap().into_bytes(),
        }
    }

    fn decode_patch(
        self,
        bytes: &[u8],
        limits: SceneFileLimits,
    ) -> Result<ScenePatchDocumentCandidate, SceneFormatError> {
        match self {
            Self::Json => ScenePatchDocumentCandidate::decode_json_bytes_with_limits(bytes, limits),
            Self::Ron => ScenePatchDocumentCandidate::decode_ron_bytes_with_limits(bytes, limits),
        }
    }
}

#[test]
fn canonical_snapshot_round_trip_preserves_both_sides_and_absence() {
    let populated = instance(ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("nested"),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("number"),
        value: ComponentValue::U64(7),
    }]));
    let patch = ScenePatchDocument::new([
        replace(None, Some(populated.clone())),
        replace(Some(populated.clone()), None),
        replace(Some(populated.clone()), Some(populated)),
        replace(None, None),
    ]);
    for encoding in [Encoding::Json, Encoding::Ron] {
        let encoded = encoding.patch(&patch);
        let decoded = encoding
            .decode_patch(&encoded, SceneFileLimits::default())
            .unwrap();
        let reencoded = match encoding {
            Encoding::Json => decoded.to_json_string().unwrap(),
            Encoding::Ron => decoded.to_ron_string().unwrap(),
        };
        assert_eq!(reencoded.as_bytes(), encoded, "{encoding:?}");
    }
}

#[test]
fn both_snapshot_trees_share_inclusive_domain_budgets() {
    let mut added = SceneEntityRecord::new(id("added")).with_component(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7)),
    );
    added.prefab = Some(instance(ScenePatchDocument::default()));
    let snapshot = instance(ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: added,
        placement: ScenePlacement::last(ScenePlacementOwner::Root),
    }]));
    let patch = ScenePatchDocument::new([replace(Some(snapshot.clone()), Some(snapshot))]);
    // One outer operation, two retained snapshots, and two added records with nested instances.
    // Diagnostic sources: 1 outer op + 2 snapshots + 2 ops + 2 entities + 2 records + 2 instances.
    let inclusive = SceneFileLimits::default()
        .with_entities(ItemLimit::new(2).unwrap())
        .with_placements(ItemLimit::new(2).unwrap())
        .with_components(ItemLimit::new(2).unwrap())
        .with_prefab_instances(ItemLimit::new(4).unwrap())
        .with_patch_operations(ItemLimit::new(3).unwrap())
        .with_diagnostic_sources(ItemLimit::new(11).unwrap())
        .with_component_value_nodes(ItemLimit::new(2).unwrap())
        .with_component_value_bytes(ByteLimit::new(16).unwrap());
    for encoding in [Encoding::Json, Encoding::Ron] {
        let bytes = encoding.patch(&patch);
        encoding.decode_patch(&bytes, inclusive).unwrap();
        for (limits, kind, observed, maximum) in [
            (
                inclusive.with_entities(ItemLimit::ONE),
                SceneFileBudgetKind::Entities,
                2,
                1,
            ),
            (
                inclusive.with_placements(ItemLimit::ONE),
                SceneFileBudgetKind::Placements,
                2,
                1,
            ),
            (
                inclusive.with_components(ItemLimit::ONE),
                SceneFileBudgetKind::Components,
                2,
                1,
            ),
            (
                inclusive.with_prefab_instances(ItemLimit::new(3).unwrap()),
                SceneFileBudgetKind::PrefabInstances,
                4,
                3,
            ),
            (
                inclusive.with_patch_operations(ItemLimit::new(2).unwrap()),
                SceneFileBudgetKind::PatchOperations,
                3,
                2,
            ),
            (
                inclusive.with_diagnostic_sources(ItemLimit::new(10).unwrap()),
                SceneFileBudgetKind::DiagnosticSources,
                11,
                10,
            ),
            (
                inclusive.with_component_value_nodes(ItemLimit::ONE),
                SceneFileBudgetKind::ComponentValueNodes,
                2,
                1,
            ),
            (
                inclusive.with_component_value_bytes(ByteLimit::new(15).unwrap()),
                SceneFileBudgetKind::ComponentValueBytes,
                16,
                15,
            ),
        ] {
            let SceneFormatError::Budget(error) =
                encoding.decode_patch(&bytes, limits).unwrap_err()
            else {
                panic!("expected {kind:?} domain budget failure for {encoding:?}");
            };
            assert_eq!(
                (error.kind, error.observed, error.maximum),
                (kind, observed, maximum)
            );
        }
    }
}

#[derive(Serialize)]
struct Envelope<'a, T> {
    kind: &'a str,
    format_version: u32,
    engine_min_version: &'a str,
    generator: Generator,
    payload: T,
}

#[derive(Serialize)]
struct Generator {
    name: &'static str,
    version: &'static str,
}

fn raw_envelope<T: Serialize>(kind: &str, payload: &T) -> (Vec<u8>, Vec<u8>) {
    let envelope = Envelope {
        kind,
        format_version: 1,
        engine_min_version: "0.1.0",
        generator: Generator {
            name: "nara",
            version: "0.1.0",
        },
        payload,
    };
    (
        serde_json::to_vec(&envelope).unwrap(),
        ron::ser::to_string(&envelope).unwrap().into_bytes(),
    )
}

#[test]
fn noncurrent_embedded_patch_versions_are_rejected_on_either_snapshot_side() {
    for side in [0, 1] {
        for nested in [false, true] {
            let mut invalid = instance(ScenePatchDocument {
                format_version: ScenePatchDocument::CURRENT_FORMAT_VERSION + 1,
                operations: Vec::new(),
            });
            if nested {
                let mut added = SceneEntityRecord::new(id("nested"));
                added.prefab = Some(invalid);
                invalid = instance(ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: added,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]));
            }
            let (expected, value) = if side == 0 {
                (Some(invalid), None)
            } else {
                (None, Some(invalid))
            };
            let patch = ScenePatchDocument::new([replace(expected, value)]);
            let scene = source_document(patch.clone());
            let prefab = PrefabDocument::from_ordered_roots(scene.entities.clone());
            let (patch_json, patch_ron) = raw_envelope("scene_patch", &patch);
            let (scene_json, scene_ron) = raw_envelope("scene", &scene);
            let (prefab_json, prefab_ron) = raw_envelope("prefab", &prefab);
            for error in [
                ScenePatchDocumentCandidate::decode_json_bytes(&patch_json).unwrap_err(),
                ScenePatchDocumentCandidate::decode_ron_bytes(&patch_ron).unwrap_err(),
                SceneDocumentCandidate::decode_json_bytes(&scene_json).unwrap_err(),
                SceneDocumentCandidate::decode_ron_bytes(&scene_ron).unwrap_err(),
                PrefabDocumentCandidate::decode_json_bytes(&prefab_json).unwrap_err(),
                PrefabDocumentCandidate::decode_ron_bytes(&prefab_ron).unwrap_err(),
            ] {
                assert!(
                    matches!(error, SceneFormatError::EmbeddedPatchFormatVersion { .. }),
                    "side {side}, nested {nested}: {error}"
                );
            }
        }
    }
}

#[test]
fn snapshot_payloads_remain_subject_to_serde_shape_preflight() {
    let snapshot = instance(ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("nested"),
        component: component(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("number"),
        value: ComponentValue::List(vec![ComponentValue::List(vec![ComponentValue::U64(7)])]),
    }]));
    let limits = SceneFileLimits::default().with_shape(SerdeShapeLimits::new(
        DepthLimit::new(8).unwrap(),
        ItemLimit::new(10_000).unwrap(),
        ItemLimit::new(1_000).unwrap(),
        ByteLimit::new(1_000).unwrap(),
        ByteLimit::new(10_000).unwrap(),
    ));
    for side in [0, 1] {
        let patch = if side == 0 {
            ScenePatchDocument::new([replace(Some(snapshot.clone()), None)])
        } else {
            ScenePatchDocument::new([replace(None, Some(snapshot.clone()))])
        };
        for encoding in [Encoding::Json, Encoding::Ron] {
            let bytes = encoding.patch(&patch);
            let error = encoding.decode_patch(&bytes, limits).unwrap_err();
            assert!(
                matches!(error, SceneFormatError::Shape { .. }),
                "{encoding:?}: {error}"
            );
        }
    }
}

#[derive(Component)]
struct SnapshotComponent;

fn registry(migrations: Arc<AtomicUsize>, native_calls: Arc<AtomicUsize>) -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    let decode_calls = Arc::clone(&native_calls);
    registry
        .register_persistent_component_with_codec::<SnapshotComponent, _, _>(
            ComponentSchema::new(component(), "Exact snapshot", ComponentSchemaVersion(2))
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                .with_fields([ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("number"),
                    "Number",
                    ComponentFieldPath::from_fields(["number"]),
                    ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
                        ComponentNumericInterval::U64 {
                            min: None,
                            max: Some(ComponentNumericBound::Included(10)),
                        },
                    ),
                    ComponentValue::U64(7),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING)]),
            move |_| {
                decode_calls.fetch_add(1, Ordering::SeqCst);
                Ok(SnapshotComponent)
            },
            move |_| {
                native_calls.fetch_add(1, Ordering::SeqCst);
                Ok(ComponentValue::map([("number", ComponentValue::U64(7))]))
            },
        )
        .unwrap();
    registry
        .register_component_migration(
            &component(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            move |_| {
                migrations.fetch_add(1, Ordering::SeqCst);
                Ok(ComponentValue::map([("number", ComponentValue::U64(99))]))
            },
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn record_snapshot(version: ComponentSchemaVersion, value: ComponentValue) -> PrefabInstance {
    instance(ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: id("nested"),
            component: component(),
            value: SceneComponentRecord::new(version, value).with_owner(
                PersistentComponentSchemaOwnerId::try_new("test.snapshot.owner").unwrap(),
            ),
        },
    ]))
}

#[test]
fn canonicalization_validates_current_snapshots_without_materializing_or_rewriting_them() {
    let migrations = Arc::new(AtomicUsize::new(0));
    let native_calls = Arc::new(AtomicUsize::new(0));
    let registry = registry(Arc::clone(&migrations), Arc::clone(&native_calls));
    let snapshot = record_snapshot(
        ComponentSchemaVersion(2),
        ComponentValue::map([] as [(&str, ComponentValue); 0]),
    );
    let document = source_document(ScenePatchDocument::new([replace(
        Some(snapshot.clone()),
        Some(snapshot),
    )]));
    for candidate in [
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap(),
        SceneDocumentCandidate::decode_ron_bytes(document.to_ron_string().unwrap().as_bytes())
            .unwrap(),
    ] {
        let canonical = candidate.canonicalize(&registry).unwrap();
        // The default is validated in an effective view but never inserted into an exact snapshot.
        assert_eq!(canonical.document(), &document);
    }
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn canonicalization_rejects_legacy_future_and_malformed_snapshots_on_both_sides() {
    let migrations = Arc::new(AtomicUsize::new(0));
    let native_calls = Arc::new(AtomicUsize::new(0));
    let registry = registry(Arc::clone(&migrations), Arc::clone(&native_calls));
    for side in [0, 1] {
        for (version, payload, code) in [
            (
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("number", ComponentValue::U64(1))]),
                "scene.file-exact-snapshot-version-mismatch",
            ),
            (
                ComponentSchemaVersion(3),
                ComponentValue::map([("number", ComponentValue::U64(1))]),
                "scene.file-exact-snapshot-version-mismatch",
            ),
            (
                ComponentSchemaVersion(2),
                ComponentValue::map([("number", ComponentValue::Bool(true))]),
                "scene.file-exact-snapshot-value-invalid",
            ),
            (
                ComponentSchemaVersion(2),
                ComponentValue::map([("number", ComponentValue::U64(11))]),
                "nara.reflect.numeric-range",
            ),
        ] {
            let snapshot = record_snapshot(version, payload);
            let operation = if side == 0 {
                replace(Some(snapshot), None)
            } else {
                replace(None, Some(snapshot))
            };
            let document = source_document(ScenePatchDocument::new([operation]));
            for candidate in [
                SceneDocumentCandidate::decode_json_bytes(
                    document.to_json_string().unwrap().as_bytes(),
                )
                .unwrap(),
                SceneDocumentCandidate::decode_ron_bytes(
                    document.to_ron_string().unwrap().as_bytes(),
                )
                .unwrap(),
            ] {
                let error = candidate.canonicalize(&registry).unwrap_err();
                assert!(
                    error
                        .diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str() == code),
                    "side {side}: {:?}",
                    error.diagnostics()
                );
            }
        }
    }
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn exact_snapshot_field_writes_preserve_current_field_constraints_without_native_calls() {
    let migrations = Arc::new(AtomicUsize::new(0));
    let native_calls = Arc::new(AtomicUsize::new(0));
    let registry = registry(Arc::clone(&migrations), Arc::clone(&native_calls));
    for side in [0, 1] {
        for (version, field, value, code) in [
            (
                ComponentSchemaVersion::ONE,
                "number",
                ComponentValue::U64(7),
                "scene.file-exact-snapshot-version-mismatch",
            ),
            (
                ComponentSchemaVersion(2),
                "removed",
                ComponentValue::U64(7),
                "scene.file-unknown-component-field",
            ),
            (
                ComponentSchemaVersion(2),
                "number",
                ComponentValue::U64(11),
                "scene.file-exact-snapshot-value-invalid",
            ),
        ] {
            let snapshot = instance(ScenePatchDocument::new([ScenePatchOperation::SetField {
                entity: id("nested"),
                component: component(),
                component_version: version,
                field: ComponentFieldId::new(field),
                value,
            }]));
            let operation = if side == 0 {
                replace(Some(snapshot), None)
            } else {
                replace(None, Some(snapshot))
            };
            let document = source_document(ScenePatchDocument::new([operation]));
            let error = SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .canonicalize(&registry)
            .unwrap_err();
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == code),
                "side {side}: {:?}",
                error.diagnostics()
            );
        }
    }
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unavailable_snapshot_records_and_targets_keep_distinct_paths_and_survive_reopen() {
    let owner = PersistentComponentSchemaOwnerId::try_new("test.snapshot.owner").unwrap();
    let snapshot = instance(ScenePatchDocument::new([
        ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(id("same")).with_component(
                component(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(3))
                    .with_owner(owner.clone()),
            ),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        },
        ScenePatchOperation::RemoveField {
            entity: id("same"),
            component: component(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("number"),
        },
    ]));
    let document = source_document(ScenePatchDocument::new([replace(
        Some(snapshot.clone()),
        Some(snapshot),
    )]));
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    let published =
        SceneDocumentCandidate::decode_json_bytes(document.to_json_string().unwrap().as_bytes())
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("scenes/snapshots.scene.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
    let manifest = published.unavailable_manifest().clone();
    assert_eq!(manifest.record_count(), 2);
    assert_eq!(manifest.patch_target_count(), 2);
    let mut record_paths = Vec::new();
    let mut target_paths = Vec::new();
    for entry in manifest.entries() {
        match entry {
            UnavailableRecordManifestEntry::Record(record) => {
                let AuthoringRecordLocation::PatchComponent {
                    anchor,
                    operation_path,
                    entity,
                    ..
                } = record.location()
                else {
                    panic!("a snapshot must not acquire a direct source record owner");
                };
                assert_eq!(anchor, &id("source-anchor"));
                assert_eq!(entity, &id("same"));
                assert_eq!(record.value(), &ComponentValue::U64(3));
                record_paths.push(operation_path.clone());
            }
            UnavailableRecordManifestEntry::PatchTarget(target) => {
                assert_eq!(target.anchor(), &id("source-anchor"));
                target_paths.push(target.operation_path().to_vec());
            }
        }
    }
    assert_eq!(record_paths, [vec![0, 0, 0], vec![0, 1, 0]]);
    assert_eq!(target_paths, [vec![0, 0, 1], vec![0, 1, 1]]);
    let session = SceneAuthoringSession::from_published_authoring_document(published);
    let readmission = session.authoring_readmission().unwrap();
    let reopened = readmission
        .reopen_scene_candidate(
            SceneDocumentCandidate::decode_ron_bytes(
                session.document().to_ron_string().unwrap().as_bytes(),
            )
            .unwrap(),
        )
        .unwrap();
    assert!(readmission.preserves_scene(&reopened));
    assert_eq!(reopened.document(), &document);
    for side in [0, 1] {
        let mut drifted = document.clone();
        let ScenePatchOperation::SetPrefabInstance {
            expected, value, ..
        } = &mut drifted.entities[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .operations[0]
        else {
            unreachable!()
        };
        let instance = if side == 0 {
            expected.as_mut().unwrap()
        } else {
            value.as_mut().unwrap()
        };
        let ScenePatchOperation::AddEntity { entity, .. } = &mut instance.overrides.operations[0]
        else {
            unreachable!()
        };
        entity.components.get_mut(&component()).unwrap().value = ComponentValue::U64(4);
        let drifted = readmission
            .reopen_scene_candidate(
                SceneDocumentCandidate::decode_json_bytes(
                    drifted.to_json_string().unwrap().as_bytes(),
                )
                .unwrap(),
            )
            .unwrap();
        assert!(!readmission.preserves_scene(&drifted), "side {side}");
    }
}

#[test]
fn unavailable_snapshot_field_payload_drift_is_rejected_by_source_edits_and_readmission() {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    for asset in [false, true] {
        for side in [0, 1] {
            let make_document = |changed| {
                let operation = if asset {
                    ScenePatchOperation::SetAssetRefField {
                        entity: id("nested"),
                        component: component(),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("field"),
                        asset_ref: AssetRef::path(if changed {
                            "changed.png"
                        } else {
                            "preserve.png"
                        })
                        .unwrap(),
                    }
                } else {
                    ScenePatchOperation::SetField {
                        entity: id("nested"),
                        component: component(),
                        component_version: ComponentSchemaVersion::ONE,
                        field: ComponentFieldId::new("field"),
                        value: ComponentValue::U64(if changed { 8 } else { 7 }),
                    }
                };
                let declaration = Some(instance(ScenePatchDocument::new([operation])));
                source_document(ScenePatchDocument::new([if side == 0 {
                    replace(declaration, None)
                } else {
                    replace(None, declaration)
                }]))
            };
            let document = make_document(false);
            let drifted = make_document(true);
            let patch = ScenePatchDocument::new([ScenePatchOperation::SetPrefabInstance {
                entity: id("source-anchor"),
                expected: document.entities[0].prefab.clone(),
                value: drifted.entities[0].prefab.clone(),
            }]);
            let published = SceneDocumentCandidate::decode_json_bytes(
                document.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(AssetRef::path("scenes/payload.scene.json").unwrap()),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
            let session = SceneAuthoringSession::from_published_authoring_document(published);
            let readmission = session.authoring_readmission().unwrap();
            let reopened = readmission
                .reopen_scene_candidate(
                    SceneDocumentCandidate::decode_json_bytes(
                        drifted.to_json_string().unwrap().as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(
                !readmission.preserves_scene(&reopened),
                "asset={asset}, side={side}"
            );
            let revision = session.revision();
            let content = session.content_state();
            let history = session.history_status();
            assert!(session.prepare_authoring_edit(revision, &patch).is_err());
            assert_eq!(session.document(), &document);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);

            let prefab = PrefabDocument::new(document.roots.clone(), document.entities.clone());
            let drifted = PrefabDocument::new(drifted.roots, drifted.entities);
            let published = PrefabDocumentCandidate::decode_json_bytes(
                prefab.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .admit_authoring(
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/payload.prefab.json").unwrap(),
                ),
                &bundle,
                &[],
            )
            .unwrap()
            .publish();
            let session = PrefabAuthoringSession::from_published_authoring_document(published);
            let readmission = session.authoring_readmission().unwrap();
            let reopened = readmission
                .reopen_prefab_candidate(
                    PrefabDocumentCandidate::decode_ron_bytes(
                        drifted.to_ron_string().unwrap().as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(
                !readmission.preserves_prefab(&reopened),
                "asset={asset}, side={side}"
            );
            let revision = session.revision();
            let content = session.content_state();
            let history = session.history_status();
            assert!(session.prepare_authoring_edit(revision, &patch).is_err());
            assert_eq!(session.document(), &prefab);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
        }
    }
}
