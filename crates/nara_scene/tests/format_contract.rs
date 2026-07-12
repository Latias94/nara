#![cfg(feature = "serde")]

use nara_asset::AssetRef;
use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabDocumentCandidate, PrefabInstance,
    PrefabSourceResolver, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityRecord, SceneFileBudgetKind, SceneFileLimits,
    SceneFormatError, ScenePatchDocument, ScenePatchDocumentCandidate, ScenePatchOperation,
    register_scene_components,
};
use serde::Serialize;

#[derive(Serialize)]
struct TestGenerator<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct TestEnvelope<'a, T> {
    kind: &'a str,
    format_version: u32,
    engine_min_version: &'a str,
    generator: TestGenerator<'a>,
    payload: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Component)]
struct MigratingText(String);

fn envelope<'a, T>(
    kind: &'a str,
    format_version: u32,
    minimum: &'a str,
    payload: T,
) -> TestEnvelope<'a, T> {
    TestEnvelope {
        kind,
        format_version,
        engine_min_version: minimum,
        generator: TestGenerator {
            name: "nara",
            version: "0.1.0",
        },
        payload,
    }
}

fn scene_id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component_id(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn sample_scene() -> SceneDocument {
    SceneDocument::new([
        SceneEntityRecord::new(scene_id("enemy")),
        SceneEntityRecord::new(scene_id("player")).with_component(
            component_id("nara.test.Health"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("current", ComponentValue::I64(10))]),
            ),
        ),
    ])
}

fn sample_prefab() -> PrefabDocument {
    PrefabDocument::new([SceneEntityRecord::new(scene_id("root")).with_component(
        component_id("nara.test.Health"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("current", ComponentValue::I64(10))]),
        ),
    )])
}

fn sample_patch() -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: scene_id("player"),
        component: component_id("nara.test.Health"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("current"),
        value: ComponentValue::I64(9),
    }])
}

#[test]
fn scene_prefab_and_patch_round_trip_through_canonical_json_and_ron_envelopes() {
    let scene = sample_scene();
    let scene_json = scene.to_json_string().unwrap();
    let scene_value = serde_json::from_str::<serde_json::Value>(&scene_json).unwrap();
    assert_eq!(scene_value["kind"], "scene");
    assert_eq!(scene_value["format_version"], 1);
    assert!(scene_value["payload"].get("format_version").is_none());
    assert_eq!(
        SceneDocumentCandidate::decode_json_bytes(scene_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        scene_json
    );
    let scene_ron = scene.to_ron_string().unwrap();
    assert_eq!(
        SceneDocumentCandidate::decode_ron_bytes(scene_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        scene_ron
    );

    let prefab = sample_prefab();
    let prefab_json = prefab.to_json_string().unwrap();
    let prefab_value = serde_json::from_str::<serde_json::Value>(&prefab_json).unwrap();
    assert_eq!(prefab_value["kind"], "prefab");
    assert!(prefab_value["payload"].get("format_version").is_none());
    assert_eq!(
        PrefabDocumentCandidate::decode_json_bytes(prefab_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        prefab_json
    );
    let prefab_ron = prefab.to_ron_string().unwrap();
    assert_eq!(
        PrefabDocumentCandidate::decode_ron_bytes(prefab_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        prefab_ron
    );

    let patch = sample_patch();
    let patch_json = patch.to_json_string().unwrap();
    let patch_value = serde_json::from_str::<serde_json::Value>(&patch_json).unwrap();
    assert_eq!(patch_value["kind"], "scene_patch");
    assert_eq!(patch_value["payload"]["format_version"], 1);
    assert_eq!(
        ScenePatchDocumentCandidate::decode_json_bytes(patch_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        patch_json
    );
    let patch_ron = patch.to_ron_string().unwrap();
    assert_eq!(
        ScenePatchDocumentCandidate::decode_ron_bytes(patch_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        patch_ron
    );
}

#[test]
fn canonical_v1_rejects_non_v1_embedded_patch_records() {
    fn entity_with_future_override() -> SceneEntityRecord {
        let mut entity = SceneEntityRecord::new(scene_id("instance"));
        entity.prefab = Some(PrefabInstance {
            source: AssetRef::path("prefabs/source.ron").unwrap(),
            overrides: ScenePatchDocument {
                format_version: ScenePatchDocument::CURRENT_FORMAT_VERSION + 1,
                operations: Vec::new(),
            },
        });
        entity
    }

    let scene = SceneDocument::new([entity_with_future_override()]);
    let prefab = PrefabDocument::new([entity_with_future_override()]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddEntity {
        entity: entity_with_future_override(),
    }]);
    for error in [
        scene.to_json_string().unwrap_err(),
        scene.to_ron_string().unwrap_err(),
        prefab.to_json_string().unwrap_err(),
        prefab.to_ron_string().unwrap_err(),
        patch.to_json_string().unwrap_err(),
        patch.to_ron_string().unwrap_err(),
    ] {
        assert!(
            error
                .to_string()
                .contains("embedded scene patch format version")
        );
    }

    let valid = SceneDocument::new([prefab_anchor("instance", "prefabs/source.ron")]);
    let mut encoded =
        serde_json::from_str::<serde_json::Value>(&valid.to_json_string().unwrap()).unwrap();
    encoded["payload"]["entities"][0]["prefab"]["overrides"]["format_version"] =
        serde_json::json!(ScenePatchDocument::CURRENT_FORMAT_VERSION + 1);
    let encoded = serde_json::to_vec(&encoded).unwrap();

    let error = SceneDocumentCandidate::decode_json_bytes(&encoded).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("embedded scene patch format version")
    );

    let invalid_json = serde_json::to_vec(&envelope("scene_patch", 1, "0.1.0", &patch)).unwrap();
    let invalid_ron = ron::ser::to_string(&envelope("scene_patch", 1, "0.1.0", &patch)).unwrap();
    for error in [
        ScenePatchDocumentCandidate::decode_json_bytes(&invalid_json).unwrap_err(),
        ScenePatchDocumentCandidate::decode_ron_bytes(invalid_ron.as_bytes()).unwrap_err(),
    ] {
        assert!(
            error
                .to_string()
                .contains("embedded scene patch format version")
        );
    }

    let mut target = SceneDocument::default();
    let before = target.clone();
    let report = patch.apply_to_scene(&mut target, &ComponentRegistry::new());
    assert!(!report.applied);
    assert_eq!(target, before);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.patch-unsupported-format-version"
    }));
}

#[test]
fn every_scene_owned_file_rejects_wrong_kind_version_and_engine_minimum() {
    let scene = sample_scene();
    let wrong_kind = serde_json::to_vec(&envelope("prefab", 1, "0.1.0", &scene)).unwrap();
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes(&wrong_kind),
        Err(SceneFormatError::Contract(_))
    ));

    let wrong_version =
        ron::ser::to_string(&envelope("prefab", 2, "0.1.0", sample_prefab())).unwrap();
    assert!(matches!(
        PrefabDocumentCandidate::decode_ron_bytes(wrong_version.as_bytes()),
        Err(SceneFormatError::Contract(_))
    ));

    let future_engine =
        serde_json::to_vec(&envelope("scene_patch", 1, "999.0.0", sample_patch())).unwrap();
    assert!(matches!(
        ScenePatchDocumentCandidate::decode_json_bytes(&future_engine),
        Err(SceneFormatError::Contract(_))
    ));
}

#[test]
fn encoded_and_shape_budgets_reject_json_and_ron_before_domain_publication() {
    let scene = sample_scene();
    let json = scene.to_json_string().unwrap();
    let byte_limits =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(json.len() - 1).unwrap());
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes_with_limits(json.as_bytes(), byte_limits),
        Err(SceneFormatError::EncodedBytesExceeded { .. })
    ));

    let ron = scene.to_ron_string().unwrap();
    let byte_limits =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(ron.len() - 1).unwrap());
    assert!(matches!(
        SceneDocumentCandidate::decode_ron_bytes_with_limits(ron.as_bytes(), byte_limits),
        Err(SceneFormatError::EncodedBytesExceeded { .. })
    ));

    let defaults = SceneFileLimits::default();
    let shallow = defaults.with_shape(SerdeShapeLimits::new(
        DepthLimit::ONE,
        defaults.shape().nodes(),
        defaults.shape().container_items(),
        defaults.shape().string_bytes(),
        defaults.shape().total_string_bytes(),
    ));
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes_with_limits(json.as_bytes(), shallow),
        Err(SceneFormatError::Shape { .. })
    ));
    assert!(matches!(
        SceneDocumentCandidate::decode_ron_bytes_with_limits(ron.as_bytes(), shallow),
        Err(SceneFormatError::Shape { .. })
    ));
}

#[test]
fn entity_component_operation_and_diagnostic_budgets_are_independent() {
    let scene = sample_scene();
    let encoded = scene.to_json_string().unwrap();

    let entity_limits = SceneFileLimits::default().with_entities(ItemLimit::ONE);
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), entity_limits),
        Err(SceneFormatError::Budget(error)) if error.kind == SceneFileBudgetKind::Entities
    ));

    let component_scene = SceneDocument::new([SceneEntityRecord::new(scene_id("player"))
        .with_component(
            component_id("nara.test.Health"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )
        .with_component(
            component_id("nara.test.Speed"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Null),
        )]);
    let encoded = component_scene.to_json_string().unwrap();
    let component_limits = SceneFileLimits::default().with_components(ItemLimit::ONE);
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes_with_limits(
            encoded.as_bytes(),
            component_limits,
        ),
        Err(SceneFormatError::Budget(error)) if error.kind == SceneFileBudgetKind::Components
    ));

    let prefab_scene = SceneDocument::new([
        prefab_anchor("first", "prefabs/unit.ron"),
        prefab_anchor("second", "prefabs/unit.ron"),
    ]);
    let encoded = prefab_scene.to_json_string().unwrap();
    let prefab_limits = SceneFileLimits::default().with_prefab_instances(ItemLimit::ONE);
    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), prefab_limits),
        Err(SceneFormatError::Budget(error))
            if error.kind == SceneFileBudgetKind::PrefabInstances
    ));

    let patch = ScenePatchDocument::new([
        ScenePatchOperation::RemoveEntity {
            entity: scene_id("first"),
        },
        ScenePatchOperation::RemoveEntity {
            entity: scene_id("second"),
        },
    ]);
    let encoded = patch.to_ron_string().unwrap();
    let operation_limits = SceneFileLimits::default().with_patch_operations(ItemLimit::ONE);
    assert!(matches!(
        ScenePatchDocumentCandidate::decode_ron_bytes_with_limits(
            encoded.as_bytes(),
            operation_limits,
        ),
        Err(SceneFormatError::Budget(error))
            if error.kind == SceneFileBudgetKind::PatchOperations
    ));

    let diagnostic_limits = SceneFileLimits::default().with_diagnostic_sources(ItemLimit::ONE);
    let encoded = sample_prefab().to_json_string().unwrap();
    assert!(matches!(
        PrefabDocumentCandidate::decode_json_bytes_with_limits(
            encoded.as_bytes(),
            diagnostic_limits,
        ),
        Err(SceneFormatError::Budget(error))
            if error.kind == SceneFileBudgetKind::DiagnosticSources
    ));
}

#[test]
fn nested_unknown_fields_are_rejected_and_existing_document_is_unchanged() {
    let existing = sample_scene();
    let before = existing.clone();
    let mut encoded =
        serde_json::from_str::<serde_json::Value>(&existing.to_json_string().unwrap()).unwrap();
    encoded["payload"]["entities"][1]["components"]["nara.test.Health"]["value"]
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::Value::Bool(true));
    let bytes = serde_json::to_vec(&encoded).unwrap();

    assert!(matches!(
        SceneDocumentCandidate::decode_json_bytes(&bytes),
        Err(SceneFormatError::Payload { .. })
    ));
    assert_eq!(existing, before);
}

#[test]
fn semantic_scene_publication_requires_frozen_registry_and_is_atomic() {
    let empty_json = SceneDocument::default().to_json_string().unwrap();
    let building_registry = ComponentRegistry::new();
    let error = SceneAuthoringSession::try_from_file_candidate(
        SceneDocumentCandidate::decode_json_bytes(empty_json.as_bytes()).unwrap(),
        &building_registry,
    )
    .unwrap_err();
    assert!(
        error.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.component-registry-not-frozen"
        })
    );

    let registry = frozen_scene_registry();
    let player = scene_id("player");
    let existing = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        component_id("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("before".to_owned()),
        ),
    )]);
    let mut session = SceneAuthoringSession::new(existing);
    let edit = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: component_id("nara.scene.Name"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("after".to_owned()),
    }]);
    let edit_report = session.apply_patch(&edit, &registry);
    assert!(edit_report.applied, "{:#?}", edit_report.diagnostics);

    let before_document = session.document().clone();
    let before_revision = session.revision();
    let before_history = session.history_status();
    let before_live_dirty = session.is_live_dirty();
    let invalid_json = sample_scene().to_json_string().unwrap();
    let error = session
        .try_replace_file_candidate(
            SceneDocumentCandidate::decode_json_bytes(invalid_json.as_bytes()).unwrap(),
            &registry,
        )
        .unwrap_err();

    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.unknown-component")
    );
    assert_eq!(session.document(), &before_document);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.history_status(), before_history);
    assert_eq!(session.is_live_dirty(), before_live_dirty);
}

#[test]
fn scene_candidate_migrates_to_the_current_schema_and_requires_source_save() {
    let registry = migrating_text_registry(0);
    let player = scene_id("player");
    let legacy = legacy_text_record("before");
    let scene = SceneDocument::new([
        SceneEntityRecord::new(player.clone()).with_component(migrating_text_id(), legacy)
    ]);
    let encoded = scene.to_json_string().unwrap();

    let mut session = SceneAuthoringSession::try_from_file_candidate(
        SceneDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
        &registry,
    )
    .unwrap();

    let component = session.document().entities[0]
        .components
        .get(&migrating_text_id())
        .unwrap();
    assert_eq!(component.version, ComponentSchemaVersion(2));
    assert_eq!(component.value.field_str("current").unwrap(), "before");
    assert!(session.source_upgrade_required());
    session.acknowledge_source_saved();
    assert!(!session.source_upgrade_required());
}

#[test]
fn migration_value_growth_respects_candidate_limits_and_is_failure_atomic() {
    let registry = migrating_text_registry(64);
    let incoming = SceneDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let encoded = incoming.to_json_string().unwrap();
    let limits = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(4).unwrap())
        .with_component_value_bytes(ByteLimit::new(16).unwrap());
    let candidate =
        SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits).unwrap();
    let before = SceneDocument::new([SceneEntityRecord::new(scene_id("existing"))]);
    let mut session = SceneAuthoringSession::new(before.clone());
    let before_revision = session.revision();

    let error = session
        .try_replace_file_candidate(candidate, &registry)
        .unwrap_err();

    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.file-component-value-budget-exceeded"
    }));
    assert_eq!(session.document(), &before);
    assert_eq!(session.revision(), before_revision);
    assert!(!session.source_upgrade_required());
}

#[test]
fn prefab_and_patch_migration_growth_respects_limits_and_is_failure_atomic() {
    let registry = migrating_text_registry(64);
    let limits = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(4).unwrap())
        .with_component_value_bytes(ByteLimit::new(16).unwrap());
    let source = AssetRef::path("prefabs/migrating.ron").unwrap();
    let existing = PrefabDocument::new([SceneEntityRecord::new(scene_id("existing"))]);
    let incoming = PrefabDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let encoded = incoming.to_json_string().unwrap();
    let mut resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), existing.clone());

    let error = resolver
        .insert_file_candidate(
            source.clone(),
            PrefabDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits)
                .unwrap(),
            &registry,
        )
        .unwrap_err();

    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.file-component-value-budget-exceeded"
    }));
    assert_eq!(resolver.resolve_prefab(&source), Some(&existing));

    let target = SceneDocument::new([SceneEntityRecord::new(scene_id("target"))]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: scene_id("target"),
        component: migrating_text_id(),
        value: legacy_text_record("x"),
    }]);
    let encoded = patch.to_ron_string().unwrap();
    let candidate =
        ScenePatchDocumentCandidate::decode_ron_bytes_with_limits(encoded.as_bytes(), limits)
            .unwrap();
    let mut session = SceneAuthoringSession::new(target.clone());
    let before_revision = session.revision();
    let before_history = session.history_status();

    let report = session.apply_file_patch_candidate(candidate, &registry);

    assert!(!report.applied);
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.file-component-value-budget-exceeded"
    }));
    assert_eq!(session.document(), &target);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.history_status(), before_history);
}

#[test]
fn migration_results_reapply_shape_limits_for_scene_prefab_and_patch() {
    let mut deeply_nested = ComponentValue::String("leaf".to_owned());
    for _ in 0..20 {
        deeply_nested = ComponentValue::List(vec![deeply_nested]);
    }
    let mut serialized_depth = ComponentValue::String("leaf".to_owned());
    for _ in 0..8 {
        serialized_depth = ComponentValue::List(vec![serialized_depth]);
    }
    let cases = [
        (
            "depth",
            deeply_nested,
            migration_shape_limits(16, 128, 256, 4_096),
            "scene.file-component-value-shape-budget-exceeded",
        ),
        (
            "serialized-depth",
            serialized_depth,
            migration_shape_limits(16, 128, 256, 4_096),
            "scene.file-post-migration-format-invalid",
        ),
        (
            "encoded-bytes",
            ComponentValue::String("x".repeat(2_048)),
            migration_shape_limits(64, 128, 4_096, 8_192)
                .with_encoded_bytes(ByteLimit::new(1_024).unwrap()),
            "scene.file-post-migration-format-invalid",
        ),
        (
            "container-items",
            ComponentValue::List(vec![ComponentValue::Null; 17]),
            migration_shape_limits(64, 16, 256, 4_096),
            "scene.file-component-value-shape-budget-exceeded",
        ),
        (
            "string-bytes",
            ComponentValue::String("x".repeat(65)),
            migration_shape_limits(64, 128, 64, 4_096),
            "scene.file-component-value-shape-budget-exceeded",
        ),
        (
            "total-string-bytes",
            ComponentValue::List(
                (0..40)
                    .map(|_| ComponentValue::String("x".repeat(32)))
                    .collect(),
            ),
            migration_shape_limits(64, 128, 64, 1_024),
            "scene.file-component-value-shape-budget-exceeded",
        ),
    ];

    for (case, migrated_value, limits, expected_code) in cases {
        let registry = migrating_text_registry_with_value(migrated_value);
        assert_scene_migration_shape_rejected(case, expected_code, &registry, limits);
        assert_prefab_migration_shape_rejected(case, expected_code, &registry, limits);
        assert_patch_migration_shape_rejected(case, expected_code, &registry, limits);
    }
}

#[test]
fn migration_results_validate_json_and_ron_encoded_limits() {
    let registry = migrating_text_registry(0);

    let legacy_scene = SceneDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let current_scene = SceneDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), current_text_record("x"))]);
    let scene_limits = ron_only_encoded_limits(
        &legacy_scene.to_json_string().unwrap(),
        &current_scene.to_json_string().unwrap(),
        &current_scene.to_ron_string().unwrap(),
    );
    assert_scene_migration_shape_rejected(
        "ron-encoded-bytes",
        "scene.file-post-migration-format-invalid",
        &registry,
        scene_limits,
    );

    let legacy_prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let current_prefab = PrefabDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), current_text_record("x"))]);
    let prefab_limits = ron_only_encoded_limits(
        &legacy_prefab.to_json_string().unwrap(),
        &current_prefab.to_json_string().unwrap(),
        &current_prefab.to_ron_string().unwrap(),
    );
    assert_prefab_migration_shape_rejected(
        "ron-encoded-bytes",
        "scene.file-post-migration-format-invalid",
        &registry,
        prefab_limits,
    );

    let legacy_patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: scene_id("target"),
        component: migrating_text_id(),
        value: legacy_text_record("x"),
    }]);
    let current_patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: scene_id("target"),
        component: migrating_text_id(),
        value: current_text_record("x"),
    }]);
    let patch_limits = ron_only_encoded_limits(
        &legacy_patch.to_json_string().unwrap(),
        &current_patch.to_json_string().unwrap(),
        &current_patch.to_ron_string().unwrap(),
    );
    assert_patch_migration_shape_rejected(
        "ron-encoded-bytes",
        "scene.file-post-migration-format-invalid",
        &registry,
        patch_limits,
    );
}

#[test]
fn patch_candidate_migrates_whole_component_operations() {
    let registry = migrating_text_registry(0);
    let add_target = scene_id("add-target");
    let replace_target = scene_id("replace-target");
    let current_record = SceneComponentRecord::new(
        ComponentSchemaVersion(2),
        ComponentValue::map([(
            "current",
            ComponentValue::String("current-before".to_owned()),
        )]),
    );
    let mut document = SceneDocument::new([
        SceneEntityRecord::new(add_target.clone()),
        SceneEntityRecord::new(replace_target.clone())
            .with_component(migrating_text_id(), current_record),
    ]);
    let patch = ScenePatchDocument::new([
        ScenePatchOperation::AddComponent {
            entity: add_target,
            component: migrating_text_id(),
            value: legacy_text_record("added"),
        },
        ScenePatchOperation::ReplaceComponent {
            entity: replace_target,
            component: migrating_text_id(),
            value: legacy_text_record("replaced"),
        },
    ]);
    let encoded = patch.to_json_string().unwrap();

    let report = ScenePatchDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .apply_to_scene(&mut document, &registry);

    assert!(report.applied, "{:#?}", report.diagnostics);
    let values = document
        .entities
        .iter()
        .map(|entity| {
            let component = entity.components.get(&migrating_text_id()).unwrap();
            assert_eq!(component.version, ComponentSchemaVersion(2));
            component.value.field_str("current").unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(values, ["added", "replaced"]);
}

#[test]
fn patch_candidate_resolves_stable_fields_against_the_current_schema_version() {
    let registry = migrating_text_registry(0);
    let player = scene_id("player");
    let mut document = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        migrating_text_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(2),
            ComponentValue::map([("current", ComponentValue::String("before".to_owned()))]),
        ),
    )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: migrating_text_id(),
        component_version: ComponentSchemaVersion(2),
        field: ComponentFieldId::new("text"),
        value: ComponentValue::String("after".to_owned()),
    }]);
    let encoded = patch.to_ron_string().unwrap();

    let report = ScenePatchDocumentCandidate::decode_ron_bytes(encoded.as_bytes())
        .unwrap()
        .apply_to_scene(&mut document, &registry);

    assert!(report.applied, "{:#?}", report.diagnostics);
    let value = &document.entities[0]
        .components
        .get(&migrating_text_id())
        .unwrap()
        .value;
    assert_eq!(value.field_str("current").unwrap(), "after");
}

#[test]
fn patch_candidate_rejects_legacy_field_writes_without_a_value_migration_contract() {
    let registry = migrating_text_registry(0);
    let player = scene_id("player");
    let before = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        migrating_text_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(2),
            ComponentValue::map([("current", ComponentValue::String("before".to_owned()))]),
        ),
    )]);
    let operations = [
        ScenePatchOperation::SetField {
            entity: player.clone(),
            component: migrating_text_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("text"),
            value: ComponentValue::String("after".to_owned()),
        },
        ScenePatchOperation::SetAssetRefField {
            entity: player,
            component: migrating_text_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("text"),
            asset_ref: AssetRef::path("textures/after.png").unwrap(),
        },
    ];

    for operation in operations {
        let encoded = ScenePatchDocument::new([operation])
            .to_json_string()
            .unwrap();
        let mut document = before.clone();

        let report = ScenePatchDocumentCandidate::decode_json_bytes(encoded.as_bytes())
            .unwrap()
            .apply_to_scene(&mut document, &registry);

        assert!(!report.applied);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.file-field-value-migration-required"
        }));
        assert_eq!(document, before);
    }
}

#[test]
fn patch_candidate_upgrades_legacy_remove_field_by_stable_identity() {
    let registry = migrating_text_registry(0);
    let player = scene_id("player");
    let mut document = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        migrating_text_id(),
        SceneComponentRecord::new(
            ComponentSchemaVersion(2),
            ComponentValue::map([
                ("current", ComponentValue::String("before".to_owned())),
                ("note", ComponentValue::String("remove-me".to_owned())),
            ]),
        ),
    )]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: player,
        component: migrating_text_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("note"),
    }]);
    let encoded = patch.to_ron_string().unwrap();

    let report = ScenePatchDocumentCandidate::decode_ron_bytes(encoded.as_bytes())
        .unwrap()
        .apply_to_scene(&mut document, &registry);

    assert!(report.applied, "{:#?}", report.diagnostics);
    let component = document.entities[0]
        .components
        .get(&migrating_text_id())
        .unwrap();
    assert_eq!(component.version, ComponentSchemaVersion(2));
    assert!(component.value.field("note").is_err());
    let inverse = report.inverse.unwrap();
    let ScenePatchOperation::SetField {
        component_version,
        field,
        value,
        ..
    } = &inverse.operations[0]
    else {
        panic!("field removal must produce a field-write inverse");
    };
    assert_eq!(*component_version, ComponentSchemaVersion(2));
    assert_eq!(field.as_str(), "note");
    assert_eq!(value, &ComponentValue::String("remove-me".to_owned()));
}

#[test]
fn patch_candidate_rejects_legacy_remove_of_tombstoned_field() {
    let registry = migrating_text_registry(0);
    let player = scene_id("player");
    let before = SceneDocument::new([SceneEntityRecord::new(player.clone())
        .with_component(migrating_text_id(), current_text_record("before"))]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::RemoveField {
        entity: player,
        component: migrating_text_id(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("retired-note"),
    }]);
    let encoded = patch.to_json_string().unwrap();
    let mut document = before.clone();

    let report = ScenePatchDocumentCandidate::decode_json_bytes(encoded.as_bytes())
        .unwrap()
        .apply_to_scene(&mut document, &registry);

    assert!(!report.applied);
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.file-unknown-component-field"
        })
    );
    assert_eq!(document, before);
}

#[test]
fn prefab_candidate_upgrades_nested_legacy_remove_field() {
    let registry = migrating_text_registry(0);
    let mut root = SceneEntityRecord::new(scene_id("root"));
    root.prefab = Some(PrefabInstance {
        source: AssetRef::path("prefabs/nested.ron").unwrap(),
        overrides: ScenePatchDocument::new([ScenePatchOperation::RemoveField {
            entity: scene_id("nested-target"),
            component: migrating_text_id(),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("note"),
        }]),
    });
    let incoming = PrefabDocument::new([root]);
    let encoded = incoming.to_json_string().unwrap();
    let published_source = AssetRef::path("prefabs/root.ron").unwrap();
    let mut resolver = InMemoryPrefabSourceResolver::new();

    resolver
        .insert_file_candidate(
            published_source.clone(),
            PrefabDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
            &registry,
        )
        .unwrap();

    let published = resolver.resolve_prefab(&published_source).unwrap();
    let operations = &published.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .operations;
    let ScenePatchOperation::RemoveField {
        component_version,
        field,
        ..
    } = &operations[0]
    else {
        panic!("nested override must remain a field removal");
    };
    assert_eq!(*component_version, ComponentSchemaVersion(2));
    assert_eq!(field.as_str(), "note");
}

#[test]
fn prefab_candidate_canonicalizes_components_and_nested_override_operations() {
    let registry = migrating_text_registry(0);
    let nested_entity = scene_id("nested-added");
    let nested_source = AssetRef::path("prefabs/nested.ron").unwrap();
    let mut root = SceneEntityRecord::new(scene_id("root"))
        .with_component(migrating_text_id(), legacy_text_record("root-before"));
    root.prefab = Some(PrefabInstance {
        source: nested_source,
        overrides: ScenePatchDocument::new([
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(nested_entity.clone())
                    .with_component(migrating_text_id(), legacy_text_record("nested-before")),
            },
            ScenePatchOperation::SetField {
                entity: nested_entity,
                component: migrating_text_id(),
                component_version: ComponentSchemaVersion(2),
                field: ComponentFieldId::new("text"),
                value: ComponentValue::String("nested-after".to_owned()),
            },
        ]),
    });
    let encoded = PrefabDocument::new([root]).to_json_string().unwrap();
    let published_source = AssetRef::path("prefabs/root.ron").unwrap();
    let mut resolver = InMemoryPrefabSourceResolver::new();

    resolver
        .insert_file_candidate(
            published_source.clone(),
            PrefabDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
            &registry,
        )
        .unwrap();

    let published = resolver.resolve_prefab(&published_source).unwrap();
    let root = &published.entities[0];
    let component = root.components.get(&migrating_text_id()).unwrap();
    assert_eq!(component.version, ComponentSchemaVersion(2));
    assert_eq!(component.value.field_str("current").unwrap(), "root-before");
    let operations = &root.prefab.as_ref().unwrap().overrides.operations;
    let ScenePatchOperation::AddEntity { entity } = &operations[0] else {
        panic!("first override must add the nested entity");
    };
    let component = entity.components.get(&migrating_text_id()).unwrap();
    assert_eq!(component.version, ComponentSchemaVersion(2));
    assert_eq!(
        component.value.field_str("current").unwrap(),
        "nested-before"
    );
    let ScenePatchOperation::SetField {
        component_version, ..
    } = &operations[1]
    else {
        panic!("second override must edit the nested entity");
    };
    assert_eq!(*component_version, ComponentSchemaVersion(2));
}

#[test]
fn prefab_and_patch_candidates_publish_only_through_their_owning_transactions() {
    let registry = frozen_scene_registry();
    let source = AssetRef::path("prefabs/player.ron").unwrap();
    let mut resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), PrefabDocument::default());
    let invalid_prefab_json = sample_prefab().to_json_string().unwrap();
    let error = resolver
        .insert_file_candidate(
            source.clone(),
            PrefabDocumentCandidate::decode_json_bytes(invalid_prefab_json.as_bytes()).unwrap(),
            &registry,
        )
        .unwrap_err();
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.unknown-component")
    );
    assert_eq!(
        resolver.resolve_prefab(&source),
        Some(&PrefabDocument::default())
    );

    let mut target = SceneDocument::new([SceneEntityRecord::new(scene_id("player"))]);
    let before = target.clone();
    let patch_json = sample_patch().to_json_string().unwrap();
    let report = ScenePatchDocumentCandidate::decode_json_bytes(patch_json.as_bytes())
        .unwrap()
        .apply_to_scene(&mut target, &registry);
    assert!(!report.applied);
    assert_eq!(target, before);
}

#[test]
fn patch_candidate_participates_in_authoring_revision_and_history_atomically() {
    let registry = frozen_scene_registry();
    let player = scene_id("player");
    let before = SceneDocument::new([SceneEntityRecord::new(player.clone()).with_component(
        component_id("nara.scene.Name"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::String("before".to_owned()),
        ),
    )]);
    let mut session = SceneAuthoringSession::new(before.clone());
    let initial_revision = session.revision();
    let valid_patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: player,
        component: component_id("nara.scene.Name"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("value"),
        value: ComponentValue::String("after".to_owned()),
    }]);
    let encoded = valid_patch.to_json_string().unwrap();

    let report = session.apply_file_patch_candidate(
        ScenePatchDocumentCandidate::decode_json_bytes(encoded.as_bytes()).unwrap(),
        &registry,
    );

    assert!(report.applied, "{:#?}", report.diagnostics);
    assert_eq!(
        session.revision().generation(),
        initial_revision.generation() + 1
    );
    assert_eq!(session.revision().source_id(), initial_revision.source_id());
    assert_eq!(session.history_status().undo_depth, 1);
    assert!(session.undo(&registry).applied);
    assert_eq!(session.document(), &before);

    let before_document = session.document().clone();
    let before_revision = session.revision();
    let before_history = session.history_status();
    let invalid_patch = sample_patch().to_ron_string().unwrap();
    let report = session.apply_file_patch_candidate(
        ScenePatchDocumentCandidate::decode_ron_bytes(invalid_patch.as_bytes()).unwrap(),
        &registry,
    );

    assert!(!report.applied);
    assert_eq!(session.document(), &before_document);
    assert_eq!(session.revision(), before_revision);
    assert_eq!(session.history_status(), before_history);
}

#[test]
fn canonical_scene_prefab_and_patch_v1_fixtures_reserialize_exactly() {
    let scene_json = include_str!("../../../tests/fixtures/formats/v1/scene.json");
    let scene_ron = include_str!("../../../tests/fixtures/formats/v1/scene.ron");
    let prefab_json = include_str!("../../../tests/fixtures/formats/v1/prefab.json");
    let prefab_ron = include_str!("../../../tests/fixtures/formats/v1/prefab.ron");
    let patch_json = include_str!("../../../tests/fixtures/formats/v1/scene_patch.json");
    let patch_ron = include_str!("../../../tests/fixtures/formats/v1/scene_patch.ron");

    assert_golden(
        &SceneDocumentCandidate::decode_json_bytes(scene_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        scene_json,
    );
    assert_golden(
        &SceneDocumentCandidate::decode_ron_bytes(scene_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        scene_ron,
    );
    assert_golden(
        &PrefabDocumentCandidate::decode_json_bytes(prefab_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        prefab_json,
    );
    assert_golden(
        &PrefabDocumentCandidate::decode_ron_bytes(prefab_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        prefab_ron,
    );
    assert_golden(
        &ScenePatchDocumentCandidate::decode_json_bytes(patch_json.as_bytes())
            .unwrap()
            .to_json_string()
            .unwrap(),
        patch_json,
    );
    assert_golden(
        &ScenePatchDocumentCandidate::decode_ron_bytes(patch_ron.as_bytes())
            .unwrap()
            .to_ron_string()
            .unwrap(),
        patch_ron,
    );
}

fn frozen_scene_registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry).unwrap();
    registry.freeze().unwrap();
    registry
}

fn migrating_text_registry(migrated_len: usize) -> ComponentRegistry {
    migrating_text_registry_with_migration(move |value| {
        let ComponentValue::Map(mut fields) = value else {
            return Err(ComponentCodecError::invalid_field("<root>", "map"));
        };
        let legacy = fields
            .remove("legacy")
            .ok_or_else(|| ComponentCodecError::missing_field("legacy"))?;
        let ComponentValue::String(legacy) = legacy else {
            return Err(ComponentCodecError::invalid_field("legacy", "string"));
        };
        let current = if migrated_len == 0 {
            legacy
        } else {
            "x".repeat(migrated_len)
        };
        Ok(ComponentValue::map([(
            "current",
            ComponentValue::String(current),
        )]))
    })
}

fn migrating_text_registry_with_value(value: ComponentValue) -> ComponentRegistry {
    migrating_text_registry_with_migration(move |_| Ok(value.clone()))
}

fn migrating_text_registry_with_migration<Migrate>(migrate: Migrate) -> ComponentRegistry
where
    Migrate:
        Fn(ComponentValue) -> Result<ComponentValue, ComponentCodecError> + Send + Sync + 'static,
{
    let mut registry = ComponentRegistry::new();
    registry
        .register_persistent_component_with_codec::<MigratingText, _, _>(
            ComponentSchema::new(
                migrating_text_id(),
                "Migrating text",
                ComponentSchemaVersion(2),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([
                ComponentFieldSchema::required(
                    ComponentFieldId::new("text"),
                    "Text",
                    ComponentFieldPath::from_fields(["current"]),
                    ComponentValueKind::String,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
                ComponentFieldSchema::optional(
                    ComponentFieldId::new("note"),
                    "Note",
                    ComponentFieldPath::from_fields(["note"]),
                    ComponentValueKind::String,
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            ])
            .with_field_tombstones([ComponentFieldId::new("retired-note")]),
            |value| Ok(MigratingText(value.field_str("current")?.to_owned())),
            |value| {
                Ok(ComponentValue::map([(
                    "current",
                    ComponentValue::String(value.0.clone()),
                )]))
            },
        )
        .unwrap();
    registry
        .register_component_migration(
            &migrating_text_id(),
            ComponentSchemaVersion::ONE,
            ComponentSchemaVersion(2),
            migrate,
        )
        .unwrap();
    registry.freeze().unwrap();
    registry
}

fn migration_shape_limits(
    depth: usize,
    container_items: usize,
    string_bytes: usize,
    total_string_bytes: usize,
) -> SceneFileLimits {
    SceneFileLimits::default().with_shape(SerdeShapeLimits::new(
        DepthLimit::new(depth).unwrap(),
        ItemLimit::new(10_000).unwrap(),
        ItemLimit::new(container_items).unwrap(),
        ByteLimit::new(string_bytes).unwrap(),
        ByteLimit::new(total_string_bytes).unwrap(),
    ))
}

fn assert_scene_migration_shape_rejected(
    case: &str,
    expected_code: &str,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) {
    let incoming = SceneDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let encoded = incoming.to_json_string().unwrap();
    let candidate =
        SceneDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits).unwrap();
    let before = SceneDocument::new([SceneEntityRecord::new(scene_id("existing"))]);
    let mut session = SceneAuthoringSession::new(before.clone());

    let error = session
        .try_replace_file_candidate(candidate, registry)
        .unwrap_err();

    assert_shape_budget_error(case, expected_code, error.diagnostics());
    assert_eq!(session.document(), &before, "scene case {case}");
}

fn assert_prefab_migration_shape_rejected(
    case: &str,
    expected_code: &str,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) {
    let source = AssetRef::path(format!("prefabs/{case}.ron")).unwrap();
    let existing = PrefabDocument::new([SceneEntityRecord::new(scene_id("existing"))]);
    let incoming = PrefabDocument::new([SceneEntityRecord::new(scene_id("incoming"))
        .with_component(migrating_text_id(), legacy_text_record("x"))]);
    let encoded = incoming.to_json_string().unwrap();
    let candidate =
        PrefabDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits).unwrap();
    let mut resolver =
        InMemoryPrefabSourceResolver::new().with_prefab(source.clone(), existing.clone());

    let error = resolver
        .insert_file_candidate(source.clone(), candidate, registry)
        .unwrap_err();

    assert_shape_budget_error(case, expected_code, error.diagnostics());
    assert_eq!(resolver.resolve_prefab(&source), Some(&existing));
}

fn assert_patch_migration_shape_rejected(
    case: &str,
    expected_code: &str,
    registry: &ComponentRegistry,
    limits: SceneFileLimits,
) {
    let before = SceneDocument::new([SceneEntityRecord::new(scene_id("target"))]);
    let patch = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: scene_id("target"),
        component: migrating_text_id(),
        value: legacy_text_record("x"),
    }]);
    let encoded = patch.to_json_string().unwrap();
    let candidate =
        ScenePatchDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits)
            .unwrap();
    let mut document = before.clone();

    let report = candidate.apply_to_scene(&mut document, registry);

    assert!(!report.applied, "patch case {case}");
    assert_shape_budget_error(case, expected_code, &report.diagnostics);
    assert_eq!(document, before, "patch case {case}");
}

fn assert_shape_budget_error(
    case: &str,
    expected_code: &str,
    diagnostics: &nara_diagnostic::DiagnosticReport,
) {
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == expected_code),
        "case {case}: {diagnostics:#?}"
    );
}

fn migrating_text_id() -> ComponentTypeId {
    component_id("nara.test.MigratingText")
}

fn legacy_text_record(value: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([("legacy", ComponentValue::String(value.to_owned()))]),
    )
}

fn current_text_record(value: &str) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(2),
        ComponentValue::map([("current", ComponentValue::String(value.to_owned()))]),
    )
}

fn ron_only_encoded_limits(
    legacy_json: &str,
    current_json: &str,
    current_ron: &str,
) -> SceneFileLimits {
    let maximum = legacy_json.len().max(current_json.len());
    assert!(
        maximum < current_ron.len(),
        "test payload must fit canonical JSON but exceed the same RON byte limit: legacy JSON {}, current JSON {}, current RON {}",
        legacy_json.len(),
        current_json.len(),
        current_ron.len()
    );
    SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(maximum).unwrap())
}

fn prefab_anchor(id: &str, source: &str) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(scene_id(id));
    entity.prefab = Some(PrefabInstance {
        source: AssetRef::path(source).unwrap(),
        overrides: ScenePatchDocument::default(),
    });
    entity
}

fn assert_golden(encoded: &str, fixture: &str) {
    assert!(!fixture.contains('\r'), "canonical fixtures must use LF");
    assert_eq!(format!("{encoded}\n"), fixture);
}
