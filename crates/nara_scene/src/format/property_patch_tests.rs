use std::{cell::Cell, slice::from_ref};

use nara_core::SerdeShapeError;
use nara_ecs::Component;
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyStep, ComponentRegistryBuilder, ComponentSchema, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
};

use super::*;
use crate::{
    PrefabAuthoringSession, ScenePatchApplyLimits,
    patch::{
        bounded_prefab_source_cost, bounded_scene_patch_cost, bounded_scene_source_cost,
        scene_patch_cost,
    },
};

const VERSION: ComponentSchemaVersion = ComponentSchemaVersion(2);

#[derive(Component)]
struct Rows(ComponentValue);

fn component() -> ComponentTypeId {
    ComponentTypeId::new("test.rows")
}

fn entity() -> SceneEntityId {
    SceneEntityId::new("root").unwrap()
}

fn field(id: &str) -> ComponentFieldId {
    ComponentFieldId::new(id)
}

fn registry(editable: bool) -> ComponentRegistrySnapshot {
    let row = ComponentValueShape::record_members([
        ComponentShapeMember::new("wire_id", ComponentValueKind::String).with_id(field("id")),
        ComponentShapeMember::new("wire_score", ComponentValueKind::U64).with_id(field("score")),
    ]);
    let mut field = ComponentFieldSchema::required(
        field("rows"),
        "Rows",
        ComponentFieldPath::from_fields(["wire_rows"]),
        ComponentValueShape::keyed_list(row, field("id")),
    )
    .with_capabilities([ComponentCapability::Scene]);
    if editable {
        field.capabilities.insert(ComponentCapability::Edit);
    }
    let schema = ComponentSchema::new(component(), "Rows", VERSION)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field]);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<Rows, _, _>(
            schema,
            |value| {
                let first = value
                    .field("wire_rows")?
                    .as_list()
                    .and_then(|rows| rows.get(0))
                    .ok_or_else(|| ComponentCodecError::missing_field("first row"))?;
                if first.field_str("wire_id")? == "renamed" && first.field_u64("wire_score")? != 9 {
                    return Err(ComponentCodecError::invalid_field(
                        "wire_score",
                        "the renamed row requires score 9",
                    ));
                }
                Ok(Rows(value.to_owned()))
            },
            |rows| Ok(rows.0.clone()),
        )
        .unwrap();
    registry
        .register_component_migration(&component(), ComponentSchemaVersion::ONE, VERSION, Ok)
        .unwrap();
    registry.freeze().unwrap()
}

fn entity_record(key: &str, score: u64) -> SceneEntityRecord {
    let row = |key: &str, score| {
        ComponentValue::map([
            ("wire_id", ComponentValue::String(key.into())),
            ("wire_score", ComponentValue::U64(score)),
        ])
    };
    SceneEntityRecord::new(entity()).with_component(
        component(),
        SceneComponentRecord::new(
            VERSION,
            ComponentValue::map([(
                "wire_rows",
                ComponentValue::List(vec![row(key, score), row("other", 2)]),
            )]),
        ),
    )
}

fn replace(key: &str, member: &str, value: ComponentValue) -> ScenePatchOperation {
    ScenePatchOperation::ReplaceProperty {
        entity: entity(),
        component: component(),
        component_version: VERSION,
        property: ComponentPropertyPath::new(field("rows")).with_steps([
            ComponentPropertyStep::ListItem {
                by: field("id"),
                key: ComponentPropertyKey::String(key.into()),
            },
            ComponentPropertyStep::Member(field(member)),
        ]),
        value,
    }
}

fn encode_patch(
    patch: &ScenePatchDocument,
    encoding: SceneFileEncoding,
    limits: SceneFileLimits,
) -> Result<String, SceneFormatError> {
    match encoding {
        SceneFileEncoding::Json => patch.to_json_string_with_limits(limits),
        SceneFileEncoding::Ron => patch.to_ron_string_with_limits(limits),
    }
}

fn decode_patch(
    encoded: &str,
    encoding: SceneFileEncoding,
    limits: SceneFileLimits,
) -> Result<ScenePatchDocumentCandidate, SceneFormatError> {
    match encoding {
        SceneFileEncoding::Json => {
            ScenePatchDocumentCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), limits)
        }
        SceneFileEncoding::Ron => {
            ScenePatchDocumentCandidate::decode_ron_bytes_with_limits(encoded.as_bytes(), limits)
        }
    }
}

#[test]
fn persisted_property_patch_replays_on_native_scene_and_prefab_with_exact_inverse() {
    let registry = registry(true);
    let before = entity_record("original", 1);
    let after = entity_record("renamed", 9);
    let forward = ScenePatchDocument::new([
        replace("original", "id", ComponentValue::String("renamed".into())),
        replace("renamed", "score", ComponentValue::U64(9)),
    ]);
    let reverse = ScenePatchDocument::new([
        replace("renamed", "score", ComponentValue::U64(1)),
        replace("renamed", "id", ComponentValue::String("original".into())),
    ]);
    let limits = SceneFileLimits::default();

    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let encoded = encode_patch(&forward, encoding, limits).unwrap();
        let decoded = decode_patch(&encoded, encoding, limits).unwrap();
        let mut scene = SceneDocument::from_ordered_roots([before.clone()]);
        let report = decoded.apply_to_scene(&mut scene, &registry);
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(scene.entities.as_slice(), from_ref(&after));
        let inverse = report.inverse.unwrap();
        assert_eq!(inverse, reverse);

        let encoded = encode_patch(&inverse, encoding, limits).unwrap();
        let decoded_inverse = decode_patch(&encoded, encoding, limits).unwrap();
        let report = decoded_inverse.apply_to_scene(&mut scene, &registry);
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(scene.entities.as_slice(), from_ref(&before));
        assert_eq!(report.inverse, Some(forward.clone()));

        let (patch, changed) = decoded.into_canonical_document(&registry).unwrap();
        assert!(!changed);
        let mut prefab =
            PrefabAuthoringSession::new(PrefabDocument::from_ordered_roots([before.clone()]));
        let report = prefab.apply_patch(&patch, &registry);
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(report.inverse, Some(reverse.clone()));
        assert_eq!(prefab.document().entities.as_slice(), from_ref(&after));
        let report = prefab.undo(&registry);
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(prefab.document().entities.as_slice(), from_ref(&before));
        let report = prefab.redo(&registry);
        assert!(report.applied, "{:#?}", report.diagnostics);
        assert_eq!(prefab.document().entities.as_slice(), from_ref(&after));
    }
}

#[test]
fn property_patch_preserves_exact_admission_and_atomic_failure() {
    let editable = registry(true);
    let read_only = registry(false);
    let before = SceneDocument::from_ordered_roots([entity_record("original", 1)]);
    let root_only = ScenePatchOperation::ReplaceProperty {
        entity: entity(),
        component: component(),
        component_version: VERSION,
        property: ComponentPropertyPath::new(field("rows")),
        value: ComponentValue::List(Vec::new()),
    };
    let mut stale = replace("original", "score", ComponentValue::U64(4));
    let ScenePatchOperation::ReplaceProperty {
        component_version, ..
    } = &mut stale
    else {
        unreachable!();
    };
    *component_version = ComponentSchemaVersion::ONE;

    for (operation, registry, code) in [
        (root_only, &editable, "scene.patch-nested-property-required"),
        (
            replace("missing", "score", ComponentValue::U64(4)),
            &editable,
            "scene.patch-property-replacement-rejected",
        ),
        (
            replace("original", "score", ComponentValue::U64(4)),
            &read_only,
            "scene.patch-field-capability-missing",
        ),
        (
            stale.clone(),
            &editable,
            "scene.patch-stale-component-schema-version",
        ),
    ] {
        let mut scene = before.clone();
        let report = ScenePatchDocument::new([operation]).apply_to_scene(&mut scene, registry);
        assert!(!report.applied);
        assert!(report.inverse.is_none());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|error| error.code().as_str() == code)
        );
        assert_eq!(scene, before);
    }

    let mut scene = before.clone();
    let encoded = ScenePatchDocument::new([stale]).to_json_string().unwrap();
    let report = ScenePatchDocumentCandidate::decode_json_str(&encoded)
        .unwrap()
        .apply_to_scene(&mut scene, &editable);
    assert!(!report.applied);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|error| { error.code().as_str() == "scene.file-field-value-migration-required" })
    );
    assert_eq!(scene, before);

    let mut legacy_record = entity_record("original", 1);
    legacy_record
        .components
        .get_mut(&component())
        .unwrap()
        .version = ComponentSchemaVersion::ONE;
    let legacy = SceneDocument::from_ordered_roots([legacy_record]);
    let mut legacy_candidate = legacy.clone();
    let report = ScenePatchDocument::new([replace("original", "score", ComponentValue::U64(4))])
        .apply_to_scene(&mut legacy_candidate, &editable);
    assert!(!report.applied);
    assert!(
        report.diagnostics.iter().any(|error| {
            error.code().as_str() == "scene.patch-target-component-version-mismatch"
        })
    );
    assert_eq!(legacy_candidate, legacy);

    let rename_only = ScenePatchDocument::new([replace(
        "original",
        "id",
        ComponentValue::String("renamed".into()),
    )]);
    let report = rename_only.apply_to_scene(&mut scene, &editable);
    assert!(!report.applied);
    assert!(report.inverse.is_none());
    assert_eq!(scene, before);
}

#[test]
fn selector_and_replacement_share_patch_and_file_budgets() {
    let patch = ScenePatchDocument::new([replace("original", "score", ComponentValue::U64(9))]);
    let ScenePatchOperation::ReplaceProperty {
        property, value, ..
    } = &patch.operations[0]
    else {
        unreachable!();
    };
    let value_cost = value.cost();
    let property_bytes = property.logical_bytes().unwrap() + value_cost.logical_bytes();
    let operations = 1 + property.steps.len();
    let replay_bytes = property_bytes + entity().as_str().len() + component().as_str().len();
    let cost = scene_patch_cost(&patch);
    assert_eq!(cost.operation_count(), operations);
    assert_eq!(cost.value_cost(), value_cost);
    assert_eq!(cost.logical_bytes(), replay_bytes);
    let replay_limits = ScenePatchApplyLimits::default()
        .with_validation_work(ItemLimit::new(operations).unwrap())
        .with_validation_value_bytes(ByteLimit::new(replay_bytes).unwrap());
    assert_eq!(bounded_scene_patch_cost(&patch, replay_limits), Some(cost));
    for limits in [
        replay_limits.with_validation_work(ItemLimit::new(operations - 1).unwrap()),
        replay_limits.with_validation_value_bytes(ByteLimit::new(replay_bytes - 1).unwrap()),
    ] {
        assert!(bounded_scene_patch_cost(&patch, limits).is_none());
    }

    let selector = ComponentPropertyPath::new(field("rows")).with_steps(std::iter::repeat_n(
        ComponentPropertyStep::Member(field("id")),
        ComponentPropertyPath::MAX_STEPS,
    ));
    let oversized = ScenePatchDocument::new([ScenePatchOperation::ReplaceProperty {
        entity: entity(),
        component: component(),
        component_version: VERSION,
        property: selector,
        value: ComponentValue::U64(9),
    }]);
    let before = SceneDocument::from_ordered_roots([entity_record("original", 1)]);
    let mut scene = before.clone();
    let registry = registry(true);
    let report = oversized.apply_to_scene_with_limits(
        &mut scene,
        &registry,
        ScenePatchApplyLimits::default()
            .with_validation_work(ItemLimit::new(ComponentPropertyPath::MAX_STEPS).unwrap()),
    );
    assert!(!report.applied);
    assert!(report.diagnostics.iter().any(|error| {
        error.summary().as_str() == "Patch input exceeds the replay data or depth budget"
    }));
    assert_eq!(scene, before);

    let nodes = property.steps.len() + value_cost.nodes();
    let file_limits = SceneFileLimits::default()
        .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
        .with_component_value_bytes(ByteLimit::new(property_bytes).unwrap());
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let encoded = encode_patch(&patch, encoding, file_limits).unwrap();
        let candidate = decode_patch(&encoded, encoding, file_limits).unwrap();
        assert_eq!(
            candidate.into_canonical_document(&registry).unwrap(),
            (patch.clone(), false)
        );
        for (limits, kind) in [
            (
                file_limits.with_component_value_nodes(ItemLimit::new(nodes - 1).unwrap()),
                SceneFileBudgetKind::ComponentValueNodes,
            ),
            (
                file_limits.with_component_value_bytes(ByteLimit::new(property_bytes - 1).unwrap()),
                SceneFileBudgetKind::ComponentValueBytes,
            ),
        ] {
            assert!(matches!(
                encode_patch(&patch, encoding, limits),
                Err(SceneFormatError::Budget(error)) if error.kind == kind
            ));
            assert!(matches!(
                decode_patch(&encoded, encoding, limits),
                Err(SceneFormatError::Budget(error)) if error.kind == kind
            ));
        }
    }
}

#[test]
fn patch_encoders_preflight_single_and_total_string_bytes() {
    let default = SceneFileLimits::default();
    let shape = default.shape();
    let key_bytes = 64;
    let single = default.with_shape(SerdeShapeLimits::new(
        shape.depth(),
        shape.nodes(),
        shape.container_items(),
        ByteLimit::new(key_bytes).unwrap(),
        shape.total_string_bytes(),
    ));
    let total_bytes = 4 * key_bytes;
    let total = default.with_shape(SerdeShapeLimits::new(
        shape.depth(),
        shape.nodes(),
        shape.container_items(),
        ByteLimit::new(key_bytes).unwrap(),
        ByteLimit::new(total_bytes).unwrap(),
    ));
    let patch_with_key_bytes = |bytes| {
        ScenePatchDocument::new([replace(&"x".repeat(bytes), "score", ComponentValue::U64(9))])
    };
    let exact = patch_with_key_bytes(key_bytes);
    let one_over = patch_with_key_bytes(key_bytes + 1);
    let aggregate_over = ScenePatchDocument::new(exact.operations.iter().cloned().cycle().take(5));

    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let encoded = encode_patch(&exact, encoding, single).unwrap();
        decode_patch(&encoded, encoding, single).unwrap();
        for (patch, limits, expected) in [
            (
                &one_over,
                single,
                SerdeShapeError::StringLimitExceeded { maximum: key_bytes },
            ),
            (
                &aggregate_over,
                total,
                SerdeShapeError::TotalStringLimitExceeded {
                    maximum: total_bytes,
                },
            ),
        ] {
            let error = encode_patch(patch, encoding, limits).unwrap_err();
            assert_eq!(error.shape_error(), Some(&expected));
            assert!(matches!(
                error,
                SceneFormatError::Shape { encoding: actual, .. } if actual == encoding
            ));
            let encoded = encode_patch(patch, encoding, default).unwrap();
            let error = decode_patch(&encoded, encoding, limits).unwrap_err();
            assert_eq!(error.shape_error(), Some(&expected));
        }
    }
}

#[test]
fn native_property_patch_uses_scene_limits_for_large_components() {
    let registry = registry(true);
    let mut entity = entity_record("original", 1);
    let record = entity.components.get_mut(&component()).unwrap();
    let sibling = ComponentFieldPath::new([
        ComponentFieldPathSegment::field("wire_rows"),
        ComponentFieldPathSegment::index(1),
        ComponentFieldPathSegment::field("wire_id"),
    ]);
    record
        .value_mut()
        .set_path(
            &sibling,
            ComponentValue::String("x".repeat(4 * 1024 * 1024 + 1)),
        )
        .unwrap();
    let bytes = 2 * record.value().cost().logical_bytes();
    let limits = ScenePatchApplyLimits::default()
        .with_validation_value_bytes(ByteLimit::new(bytes).unwrap());
    let before = SceneDocument::from_ordered_roots([entity]);
    let mut scene = before.clone();
    let patch = ScenePatchDocument::new([replace("original", "score", ComponentValue::U64(9))]);

    let report = patch.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(report.applied, "{:#?}", report.diagnostics);
    let inverse = report.inverse.unwrap();
    assert_eq!(
        inverse.operations,
        [replace("original", "score", ComponentValue::U64(1))]
    );
    let undone = inverse.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(undone.applied, "{:#?}", undone.diagnostics);
    assert_eq!(scene, before);
}

#[test]
fn native_property_patch_admits_the_complete_inverse_before_publication() {
    let registry = registry(true);
    let mut record = entity_record("original", 1);
    record.id = SceneEntityId::new("root".repeat(32)).unwrap();
    let mut edit = replace("original", "score", ComponentValue::U64(9));
    let ScenePatchOperation::ReplaceProperty { entity, .. } = &mut edit else {
        unreachable!();
    };
    *entity = record.id.clone();
    let patch = ScenePatchDocument::new([
        edit,
        ScenePatchOperation::RemoveComponent {
            entity: record.id.clone(),
            component: component(),
        },
    ]);
    let before = SceneDocument::from_ordered_roots([record]);
    let mut after = before.clone();
    let report = patch.apply_to_scene(&mut after, &registry);
    assert!(report.applied, "{:#?}", report.diagnostics);
    let inverse = report.inverse.unwrap();
    let inverse_bytes = scene_patch_cost(&inverse).logical_bytes();
    assert!(scene_patch_cost(&patch).logical_bytes() < inverse_bytes);
    let limits = ScenePatchApplyLimits::default()
        .with_validation_value_bytes(ByteLimit::new(inverse_bytes).unwrap());
    let short = limits.with_validation_value_bytes(ByteLimit::new(inverse_bytes - 1).unwrap());

    let mut scene = before.clone();
    let rejected = patch.apply_to_scene_with_limits(&mut scene, &registry, short);
    assert!(!rejected.applied);
    assert!(rejected.inverse.is_none());
    assert!(rejected.diagnostics.iter().any(|error| {
        error.summary().as_str() == "Patch inverse exceeds the replay data or depth budget"
    }));
    assert_eq!(scene, before);

    let accepted = patch.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(accepted.applied, "{:#?}", accepted.diagnostics);
    assert_eq!(accepted.inverse.as_ref(), Some(&inverse));
    assert_eq!(scene, after);
    let rejected = inverse.apply_to_scene_with_limits(&mut scene, &registry, short);
    assert!(!rejected.applied);
    assert_eq!(scene, after);
    let undone = inverse.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(undone.applied, "{:#?}", undone.diagnostics);
    assert_eq!(undone.inverse, Some(patch));
    assert_eq!(scene, before);
}

#[test]
fn native_property_patch_admits_the_final_source_for_inverse_replay() {
    let registry = registry(true);
    let entity = SceneEntityId::new("root".repeat(32)).unwrap();
    let new_key = "x".repeat(1024);
    let mut old_record = entity_record("original", 1);
    old_record.id = entity.clone();
    let mut new_record = entity_record(&new_key, 1);
    new_record.id = entity.clone();
    let before = SceneDocument::from_ordered_roots([old_record.clone()]);
    let after = SceneDocument::from_ordered_roots([new_record.clone()]);
    let prefab_before = PrefabDocument::from_ordered_roots([old_record]);
    let prefab_after = PrefabDocument::from_ordered_roots([new_record]);

    let mut rename = replace("original", "id", ComponentValue::String(new_key.clone()));
    let mut restore = replace(&new_key, "id", ComponentValue::String("original".into()));
    for operation in [&mut rename, &mut restore] {
        let ScenePatchOperation::ReplaceProperty { entity: target, .. } = operation else {
            unreachable!();
        };
        *target = entity.clone();
    }
    let patch = ScenePatchDocument::new([rename]);
    let inverse = ScenePatchDocument::new([restore]);
    let default = ScenePatchApplyLimits::default();
    let final_bytes = bounded_scene_source_cost(&after, default)
        .unwrap()
        .logical_bytes();
    let limits = default.with_validation_value_bytes(ByteLimit::new(final_bytes).unwrap());
    let short = limits.with_validation_value_bytes(ByteLimit::new(final_bytes - 1).unwrap());
    assert!(bounded_scene_patch_cost(&patch, short).is_some());
    assert!(bounded_scene_patch_cost(&inverse, short).is_some());
    assert!(bounded_scene_source_cost(&before, short).is_some());
    assert!(bounded_scene_source_cost(&after, short).is_none());
    assert!(bounded_prefab_source_cost(&prefab_before, short).is_some());
    assert!(bounded_prefab_source_cost(&prefab_after, short).is_none());

    let mut scene = before.clone();
    let rejected = patch.apply_to_scene_with_limits(&mut scene, &registry, short);
    assert_eq!(scene, before);
    let prefab_final_checked = Cell::new(false);
    let validate_prefab =
        |document: &PrefabDocument, registry: &ComponentRegistrySnapshot, index| {
            let scene = document.instantiate();
            match index {
                Some(index) => {
                    prefab_final_checked.set(true);
                    scene.validate_authoring_for_patch(registry, index)
                }
                None => scene.validate_authoring(registry),
            }
        };
    let prefab_rejected = match patch.prepare_to_prefab_with_validator(
        &prefab_before,
        &registry,
        short,
        validate_prefab,
    ) {
        Ok(_) => panic!("Prefab with an over-budget reverse source was prepared"),
        Err(report) => report,
    };
    assert!(prefab_final_checked.get());
    for report in [rejected, prefab_rejected] {
        assert!(!report.applied);
        assert!(report.inverse.is_none());
        assert!(report.diagnostics.iter().any(|error| {
            error.summary().as_str()
                == "Patched source declarations exceed the inverse replay data or depth budget"
        }));
    }

    let accepted = patch.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(accepted.applied, "{:#?}", accepted.diagnostics);
    assert_eq!(accepted.inverse.as_ref(), Some(&inverse));
    assert_eq!(scene, after);
    let undone = inverse.apply_to_scene_with_limits(&mut scene, &registry, limits);
    assert!(undone.applied, "{:#?}", undone.diagnostics);
    assert_eq!(scene, before);

    let (prefab, accepted) = patch
        .prepare_to_prefab_with_validator(&prefab_before, &registry, limits, validate_prefab)
        .unwrap()
        .into_parts();
    assert_eq!(prefab, prefab_after);
    assert_eq!(accepted.inverse.as_ref(), Some(&inverse));
    let (restored, undone) = inverse
        .prepare_to_prefab_with_validator(&prefab, &registry, limits, validate_prefab)
        .unwrap()
        .into_parts();
    assert!(undone.applied);
    assert_eq!(restored, prefab_before);
}
