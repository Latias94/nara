#![cfg(feature = "serde")]

#[path = "support/prefab_source.rs"]
mod prefab_source;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
use nara_ecs::Component;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentNumericBound, ComponentNumericInterval, ComponentPropertyPath,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueShape, PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    AuthoringDocumentSource, AuthoringOverlayPathSegment, AuthoringOverlayRecordSlot,
    AuthoringRecordLocation, PrefabAddition, PrefabAuthoringSession, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabDocument, PrefabDocumentCandidate, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceClaim, PrefabOverlay, PrefabPlacement, PrefabPlacementParent,
    PrefabPlacementSlot, PrefabPropertyClaim, PrefabPropertyIntent, SceneAuthoringSession,
    SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId, SceneEntityRecord,
    SceneFileBudgetKind, SceneFileLimits, SceneFormatError, ScenePatchDocument,
    ScenePatchDocumentCandidate, ScenePatchOperation, UnavailableRecordManifestEntry,
};
use serde::Serialize;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.ExactSnapshot")
}

fn instance(overlay: PrefabOverlay) -> PrefabInstance {
    PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: Some(overlay),
    }
}

fn replace(expected: Option<PrefabInstance>, value: Option<PrefabInstance>) -> ScenePatchOperation {
    ScenePatchOperation::SetPrefabInstance {
        entity: id("source-anchor"),
        expected,
        value,
    }
}

fn source_document(declaration: Option<PrefabInstance>) -> SceneDocument {
    let mut anchor = SceneEntityRecord::new(id("source-anchor"));
    anchor.prefab = declaration;
    SceneDocument::from_ordered_roots([anchor])
}

fn overlay() -> PrefabOverlay {
    PrefabOverlay::new(prefab_source::key("snapshot.base"))
}

fn property(
    version: ComponentSchemaVersion,
    field: &str,
    intent: PrefabPropertyIntent,
) -> PrefabOverlay {
    let mut overlay = overlay();
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("nested")),
        component: component(),
        version,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new(field)),
            intent,
            bindings: Vec::new(),
        }],
    });
    overlay
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
    let populated = instance(property(
        ComponentSchemaVersion::ONE,
        "number",
        PrefabPropertyIntent::Set(ComponentValue::U64(7)),
    ));
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
    let mut added = PrefabAddition::new(id("added"));
    added.components.insert(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(7)),
    );
    added.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        overrides: None,
    });
    let mut declarations = overlay();
    declarations.additions.push(added);
    declarations.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added")),
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    });
    let snapshot = instance(declarations);
    let patch = ScenePatchDocument::new([replace(Some(snapshot.clone()), Some(snapshot))]);
    // The exact operation retains both complete declaration trees. Each side contains one
    // overlay, one addition, one placement, one component and two instance declarations.
    // Each source claim retains the AssetRef map keys/kind (13) and path. A Named base
    // retains kind/origin tags and UUID (18), plus its name. The addition retains its ID,
    // component ID, record version/owner flag (5), U64 (8), and placement target/tags (3).
    let source_bytes = 13 + "prefabs/base.prefab.json".len();
    let snapshot_bytes = 2 * source_bytes
        + (18 + "snapshot.base".len())
        + "added".len()
        + ("test.ExactSnapshot".len() + 5 + 8)
        + ("added".len() + 3);
    let complete_bytes = 2 * snapshot_bytes;
    let inclusive = SceneFileLimits::default()
        .with_entities(ItemLimit::new(2).unwrap())
        .with_placements(ItemLimit::new(2).unwrap())
        .with_components(ItemLimit::new(2).unwrap())
        .with_prefab_instances(ItemLimit::new(4).unwrap())
        .with_patch_operations(ItemLimit::ONE)
        .with_overlay_declarations(ItemLimit::new(6).unwrap())
        .with_diagnostic_sources(ItemLimit::new(13).unwrap())
        .with_component_value_nodes(ItemLimit::new(16).unwrap())
        .with_component_value_bytes(ByteLimit::new(complete_bytes).unwrap());
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
                inclusive.with_overlay_declarations(ItemLimit::new(5).unwrap()),
                SceneFileBudgetKind::OverlayDeclarations,
                6,
                5,
            ),
            (
                inclusive.with_diagnostic_sources(ItemLimit::new(12).unwrap()),
                SceneFileBudgetKind::DiagnosticSources,
                13,
                12,
            ),
            (
                inclusive.with_component_value_nodes(ItemLimit::new(15).unwrap()),
                SceneFileBudgetKind::ComponentValueNodes,
                16,
                15,
            ),
            (
                inclusive.with_component_value_bytes(ByteLimit::new(complete_bytes - 1).unwrap()),
                SceneFileBudgetKind::ComponentValueBytes,
                complete_bytes,
                complete_bytes - 1,
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
fn embedded_operation_patches_have_no_reader_on_either_exact_snapshot_side() {
    for side in [0, 1] {
        for nested in [false, true] {
            let marker = PrefabOverlay::new(prefab_source::key("legacy-marker"));
            let mut invalid = instance(marker.clone());
            if nested {
                let mut added = PrefabAddition::new(id("nested"));
                added.prefab = Some(invalid);
                let mut containing = overlay();
                containing.additions.push(added);
                invalid = instance(containing);
            }
            let scene = source_document(Some(invalid.clone()));
            let (expected, value) = if side == 0 {
                (Some(invalid), None)
            } else {
                (None, Some(invalid))
            };
            let patch = ScenePatchDocument::new([replace(expected, value)]);
            let prefab = PrefabDocument::from_ordered_roots(scene.entities.clone());
            let (patch_json, patch_ron) = raw_envelope("scene_patch", &patch);
            let (scene_json, scene_ron) = raw_envelope("scene", &scene);
            let (prefab_json, prefab_ron) = raw_envelope("prefab", &prefab);
            for format_version in [1, 2] {
                let legacy = ScenePatchDocument {
                    format_version,
                    operations: Vec::new(),
                };
                let json = |bytes: &[u8]| {
                    let encoded = String::from_utf8(bytes.to_vec()).unwrap();
                    let needle = serde_json::to_string(&Some(marker.clone())).unwrap();
                    assert!(encoded.contains(&needle));
                    encoded.replace(&needle, &serde_json::to_string(&legacy).unwrap())
                };
                let ron = |bytes: &[u8]| {
                    let encoded = String::from_utf8(bytes.to_vec()).unwrap();
                    let needle = ron::ser::to_string(&Some(marker.clone())).unwrap();
                    assert!(encoded.contains(&needle));
                    encoded.replace(&needle, &ron::ser::to_string(&legacy).unwrap())
                };
                for error in [
                    ScenePatchDocumentCandidate::decode_json_bytes(json(&patch_json).as_bytes())
                        .unwrap_err(),
                    ScenePatchDocumentCandidate::decode_ron_bytes(ron(&patch_ron).as_bytes())
                        .unwrap_err(),
                    SceneDocumentCandidate::decode_json_bytes(json(&scene_json).as_bytes())
                        .unwrap_err(),
                    SceneDocumentCandidate::decode_ron_bytes(ron(&scene_ron).as_bytes())
                        .unwrap_err(),
                    PrefabDocumentCandidate::decode_json_bytes(json(&prefab_json).as_bytes())
                        .unwrap_err(),
                    PrefabDocumentCandidate::decode_ron_bytes(ron(&prefab_ron).as_bytes())
                        .unwrap_err(),
                ] {
                    assert!(
                        matches!(error, SceneFormatError::Payload { .. }),
                        "side {side}, nested {nested}, version {format_version}: {error}"
                    );
                }
            }
        }
    }
}

#[test]
fn snapshot_payloads_remain_subject_to_serde_shape_preflight() {
    let snapshot = instance(property(
        ComponentSchemaVersion::ONE,
        "number",
        PrefabPropertyIntent::Set(ComponentValue::List(vec![ComponentValue::List(vec![
            ComponentValue::U64(7),
        ])])),
    ));
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
    let mut declarations = overlay();
    declarations.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Add,
        target: PrefabEntityTarget::source(id("nested")),
        component: component(),
        record: SceneComponentRecord::new(version, value)
            .with_owner(PersistentComponentSchemaOwnerId::try_new("test.snapshot.owner").unwrap()),
        bindings: Vec::new(),
    });
    instance(declarations)
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
    let document = source_document(Some(snapshot.clone()));
    let patch = ScenePatchDocument::new([replace(Some(snapshot.clone()), Some(snapshot))]);
    for candidate in [
        ScenePatchDocumentCandidate::decode_json_bytes(patch.to_json_string().unwrap().as_bytes())
            .unwrap(),
        ScenePatchDocumentCandidate::decode_ron_bytes(patch.to_ron_string().unwrap().as_bytes())
            .unwrap(),
    ] {
        let mut target = document.clone();
        let report = candidate.apply_to_scene(&mut target, &registry);
        assert!(report.applied, "{:?}", report.diagnostics);
        // The default is validated in an effective view but never inserted into an exact snapshot.
        assert_eq!(target, document);
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
                replace(Some(snapshot.clone()), None)
            } else {
                replace(None, Some(snapshot.clone()))
            };
            let document = source_document(if side == 0 {
                Some(snapshot.clone())
            } else {
                None
            });
            let patch = ScenePatchDocument::new([operation]);
            for candidate in [
                ScenePatchDocumentCandidate::decode_json_bytes(
                    patch.to_json_string().unwrap().as_bytes(),
                )
                .unwrap(),
                ScenePatchDocumentCandidate::decode_ron_bytes(
                    patch.to_ron_string().unwrap().as_bytes(),
                )
                .unwrap(),
            ] {
                let mut target = document.clone();
                let error = candidate.apply_to_scene(&mut target, &registry);
                assert!(!error.applied);
                assert_eq!(target, document);
                assert!(
                    error
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str() == code),
                    "side {side}: {:?}",
                    error.diagnostics
                );
            }
        }
    }
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn declarative_property_constraints_are_preserved_until_activation_without_native_calls() {
    let migrations = Arc::new(AtomicUsize::new(0));
    let native_calls = Arc::new(AtomicUsize::new(0));
    let registry = registry(Arc::clone(&migrations), Arc::clone(&native_calls));
    for (version, field, value) in [
        (
            ComponentSchemaVersion::ONE,
            "number",
            ComponentValue::U64(7),
        ),
        (ComponentSchemaVersion(2), "removed", ComponentValue::U64(7)),
        (ComponentSchemaVersion(2), "number", ComponentValue::U64(11)),
    ] {
        let snapshot = instance(property(version, field, PrefabPropertyIntent::Set(value)));
        let document = source_document(Some(snapshot));
        let canonical = SceneDocumentCandidate::decode_json_bytes(
            document.to_json_string().unwrap().as_bytes(),
        )
        .unwrap()
        .canonicalize(&registry)
        .unwrap();
        assert_eq!(canonical.document(), &document);
        if version == ComponentSchemaVersion::ONE {
            // Exact source transactions must retain the authored version on either
            // snapshot side; they cannot migrate their equality evidence in place.
            for prior in [false, true] {
                let mut target = if prior {
                    document.clone()
                } else {
                    source_document(None)
                };
                let before = target.clone();
                let patch = ScenePatchDocument::new([replace(
                    if prior {
                        document.entities[0].prefab.clone()
                    } else {
                        None
                    },
                    if prior {
                        None
                    } else {
                        document.entities[0].prefab.clone()
                    },
                )]);
                let report = ScenePatchDocumentCandidate::decode_json_bytes(
                    patch.to_json_string().unwrap().as_bytes(),
                )
                .unwrap()
                .apply_to_scene(&mut target, &registry);
                assert!(!report.applied);
                assert!(
                    report
                        .diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code().as_str()
                            == "scene.file-exact-snapshot-version-mismatch")
                );
                assert_eq!(target, before);
            }
        }
        let resolver = nara_scene::InMemoryPrefabSourceResolver::new().with_prefab(
            AssetRef::path("prefabs/base.prefab.json").unwrap(),
            prefab_source::key("snapshot.base"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("nested"))
                .with_component(
                    component(),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion(2),
                        ComponentValue::map([("number", ComponentValue::U64(7))]),
                    ),
                )]),
        );
        let error = document.expand_prefabs(&registry, &resolver);
        assert!(error.candidate.is_none());
        assert!(
            error
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-compilation-failed"),
            "{:?}",
            error.diagnostics
        );
    }
    assert_eq!(migrations.load(Ordering::SeqCst), 0);
    assert_eq!(native_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn unavailable_nested_declarations_keep_distinct_paths_and_survive_reopen() {
    let owner = PersistentComponentSchemaOwnerId::try_new("test.snapshot.owner").unwrap();
    let mut declarations = property(
        ComponentSchemaVersion::ONE,
        "number",
        PrefabPropertyIntent::Absent,
    );
    let mut addition = PrefabAddition::new(id("same"));
    addition.components.insert(
        component(),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(3))
            .with_owner(owner.clone()),
    );
    declarations.additions.push(addition);
    let snapshot = instance(declarations);
    let mut containing = overlay();
    let mut addition = PrefabAddition::new(id("nested"));
    addition.prefab = Some(snapshot.clone());
    containing.additions.push(addition);
    containing.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("nested")),
        value: Some(snapshot),
    });
    let document = source_document(Some(instance(containing)));
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
    assert_eq!(manifest.overlay_target_count(), 2);
    let mut record_paths = Vec::new();
    let mut target_paths = Vec::new();
    for entry in manifest.entries() {
        match entry {
            UnavailableRecordManifestEntry::Record(record) => {
                let AuthoringRecordLocation::OverlayComponent {
                    anchor,
                    overlays,
                    slot,
                    target,
                } = record.location()
                else {
                    panic!("a snapshot must not acquire a direct source record owner");
                };
                assert_eq!(anchor, &id("source-anchor"));
                assert_eq!(target, &PrefabEntityTarget::addition(id("same")));
                assert_eq!(slot, &AuthoringOverlayRecordSlot::Addition { index: 0 });
                assert_eq!(record.value(), &ComponentValue::U64(3));
                record_paths.push(overlays.clone());
            }
            UnavailableRecordManifestEntry::OverlayTarget(target) => {
                assert_eq!(target.anchor(), &id("source-anchor"));
                assert_eq!(target.target(), &PrefabEntityTarget::source(id("nested")));
                let AuthoringRecordLocation::OverlayComponent { overlays, slot, .. } =
                    target.location()
                else {
                    panic!("a property target belongs to its declaring overlay");
                };
                assert_eq!(
                    slot,
                    &AuthoringOverlayRecordSlot::Property {
                        component_index: 0,
                        claim_index: 0
                    }
                );
                target_paths.push(overlays.clone());
            }
        }
    }
    let root = AuthoringOverlayPathSegment::Root {
        source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
        base: prefab_source::key("snapshot.base"),
    };
    let paths = [
        vec![
            root.clone(),
            AuthoringOverlayPathSegment::Addition {
                index: 0,
                entity: id("nested"),
                source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
                base: prefab_source::key("snapshot.base"),
            },
        ],
        vec![
            root,
            AuthoringOverlayPathSegment::InstanceClaim {
                index: 0,
                target: PrefabEntityTarget::source(id("nested")),
                source: AssetRef::path("prefabs/base.prefab.json").unwrap(),
                base: prefab_source::key("snapshot.base"),
            },
        ],
    ];
    assert_eq!(record_paths, paths);
    assert_eq!(target_paths, paths);
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
        let overlay = drifted.entities[0]
            .prefab
            .as_mut()
            .unwrap()
            .overrides
            .as_mut()
            .unwrap();
        let instance = if side == 0 {
            overlay.additions[0].prefab.as_mut().unwrap()
        } else {
            overlay.instances[0].value.as_mut().unwrap()
        };
        instance.overrides.as_mut().unwrap().additions[0]
            .components
            .get_mut(&component())
            .unwrap()
            .value = ComponentValue::U64(4);
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
fn unavailable_nested_property_repair_is_exact_and_readmission_still_rejects_drift() {
    let bundle = AuthoringSchemaBundle::new([]).unwrap();
    for asset in [false, true] {
        for side in [0, 1] {
            let make_document = |changed| {
                let value = if asset {
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".into())),
                        (
                            "value",
                            ComponentValue::String(
                                if changed {
                                    "changed.png"
                                } else {
                                    "preserve.png"
                                }
                                .into(),
                            ),
                        ),
                    ])
                } else {
                    ComponentValue::U64(if changed { 8 } else { 7 })
                };
                let declaration = instance(property(
                    ComponentSchemaVersion::ONE,
                    "field",
                    PrefabPropertyIntent::Set(value),
                ));
                let mut declarations = overlay();
                if side == 0 {
                    let mut addition = PrefabAddition::new(id("nested"));
                    addition.prefab = Some(declaration);
                    declarations.additions.push(addition);
                } else {
                    declarations.instances.push(PrefabInstanceClaim {
                        target: PrefabEntityTarget::source(id("nested")),
                        value: Some(declaration),
                    });
                }
                let mut document = source_document(Some(instance(declarations)));
                document.entities[0].components.insert(
                    ComponentTypeId::new("missing.Direct"),
                    SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::U64(42)),
                );
                let mut other = SceneEntityRecord::new(id("untouched-anchor"));
                other.prefab = Some(instance(property(
                    ComponentSchemaVersion::ONE,
                    "field",
                    PrefabPropertyIntent::Set(ComponentValue::U64(99)),
                )));
                document.roots.push(other.id.clone());
                document.entities.push(other);
                document
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
            let mut session = SceneAuthoringSession::from_published_authoring_document(published);
            let readmission = session.authoring_readmission().unwrap();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            assert_eq!(manifest.record_count(), 1);
            assert_eq!(manifest.overlay_target_count(), 2);
            let protected = manifest
                .entries()
                .iter()
                .filter(|entry| match entry {
                    UnavailableRecordManifestEntry::Record(_) => true,
                    UnavailableRecordManifestEntry::OverlayTarget(target) => {
                        target.anchor() != &id("source-anchor")
                    }
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(protected.len(), 2);
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
            let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
            assert_eq!(prepared.document(), &drifted);
            let inverse = ScenePatchDocument::new([replace(
                drifted.entities[0].prefab.clone(),
                document.entities[0].prefab.clone(),
            )]);
            assert_eq!(prepared.inverse(), &inverse);
            assert_eq!(session.document(), &document);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert!(session.commit_authoring_edit(prepared).applied);
            assert_eq!(session.document(), &drifted);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(
                session
                    .unavailable_record_manifest()
                    .unwrap()
                    .entries()
                    .iter()
                    .filter(|entry| match entry {
                        UnavailableRecordManifestEntry::Record(_) => true,
                        UnavailableRecordManifestEntry::OverlayTarget(target) => {
                            target.anchor() != &id("source-anchor")
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
                protected
            );
            let repaired_readmission = session.authoring_readmission().unwrap();
            let reopened = repaired_readmission
                .reopen_scene_candidate(
                    SceneDocumentCandidate::decode_ron_bytes(
                        session.document().to_ron_string().unwrap().as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(repaired_readmission.preserves_scene(&reopened));
            assert!(!readmission.preserves_scene(&reopened));
            assert_eq!(reopened.document(), &drifted);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &document);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert!(readmission.preserves_scene(&session));
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &drifted);
            assert!(repaired_readmission.preserves_scene(&session));

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
            let mut session = PrefabAuthoringSession::from_published_authoring_document(published);
            let readmission = session.authoring_readmission().unwrap();
            let manifest = session.unavailable_record_manifest().unwrap().clone();
            let protected = manifest
                .entries()
                .iter()
                .filter(|entry| match entry {
                    UnavailableRecordManifestEntry::Record(_) => true,
                    UnavailableRecordManifestEntry::OverlayTarget(target) => {
                        target.anchor() != &id("source-anchor")
                    }
                })
                .cloned()
                .collect::<Vec<_>>();
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
            let prepared = session.prepare_authoring_edit(revision, &patch).unwrap();
            assert_eq!(prepared.document(), &drifted);
            assert_eq!(prepared.inverse(), &inverse);
            assert_eq!(session.document(), &prefab);
            assert_eq!(session.revision(), revision);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.history_status(), history);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert!(session.commit_authoring_edit(prepared).applied);
            assert_eq!(session.document(), &drifted);
            assert_eq!(session.history_status().undo_depth, 1);
            assert_eq!(
                session
                    .unavailable_record_manifest()
                    .unwrap()
                    .entries()
                    .iter()
                    .filter(|entry| match entry {
                        UnavailableRecordManifestEntry::Record(_) => true,
                        UnavailableRecordManifestEntry::OverlayTarget(target) => {
                            target.anchor() != &id("source-anchor")
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
                protected
            );
            let repaired_readmission = session.authoring_readmission().unwrap();
            let reopened = repaired_readmission
                .reopen_prefab_candidate(
                    PrefabDocumentCandidate::decode_json_bytes(
                        session.document().to_json_string().unwrap().as_bytes(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(repaired_readmission.preserves_prefab(&reopened));
            assert!(!readmission.preserves_prefab(&reopened));
            assert_eq!(reopened.document(), &drifted);
            assert!(session.undo_authoring().applied);
            assert_eq!(session.document(), &prefab);
            assert_eq!(session.content_state(), content);
            assert_eq!(session.unavailable_record_manifest(), Some(&manifest));
            assert!(readmission.preserves_prefab(&session));
            assert!(session.redo_authoring().applied);
            assert_eq!(session.document(), &drifted);
            assert!(repaired_readmission.preserves_prefab(&session));
        }
    }
}
