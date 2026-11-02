use super::*;

use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyReadBudget,
    ComponentPropertyStep, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabEntityTarget,
    PrefabInstance, PrefabInstanceClaim, PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent,
    SceneComponentRecord, SceneEntityId, authoring_decode::AuthoringAdapterReceipt,
};

const COMPONENT: &str = "trial.enemy.EnemyStats";

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("trial.enemy").unwrap()
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn selector(next: bool) -> ComponentPropertyPath {
    let path = ComponentPropertyPath::new(ComponentFieldId::new("health"));
    if next {
        path.with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
            "milli",
        ))])
    } else {
        path
    }
}

fn value(next: bool) -> ComponentValue {
    let health = if next {
        ComponentValue::map([("milli", ComponentValue::I64(105000))])
    } else {
        ComponentValue::I64(105)
    };
    ComponentValue::map([("wire_health", health), ("armor", ComponentValue::I64(7))])
}

fn schema(next: bool) -> ComponentSchema {
    let health = if next {
        ComponentValueShape::Record {
            members: vec![
                ComponentShapeMember::new("milli", ComponentValueKind::I64)
                    .with_id(ComponentFieldId::new("milli")),
            ],
        }
    } else {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    };
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Enemy stats",
        ComponentSchemaVersion(if next { 2 } else { 1 }),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("health"),
            "Health",
            ComponentFieldPath::from_fields(["wire_health"]),
            health,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("armor"),
            "Armor",
            ComponentFieldPath::from_fields(["armor"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn upgrade() -> AuthoredFieldUpgrade {
    let old = ComponentSchemaCatalog {
        components: vec![schema(false)],
        ..Default::default()
    };
    let mut next = ComponentSchemaCatalog::successor_of(&old).unwrap();
    next.components.push(schema(true));
    let admit = |catalog, previous| {
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
            catalog,
            previous,
        )])
        .unwrap();
        let adapter =
            AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
                .unwrap();
        AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
    };
    AuthoredFieldUpgrade::prepare(
        admit(old.clone(), None),
        admit(next, Some(old)),
        ComponentTypeId::new(COMPONENT),
        ComponentFieldId::new("health"),
        selector(true),
        &mut ComponentPropertyReadBudget::new(
            ItemLimit::new(4096).unwrap(),
            ByteLimit::new(1024 * 1024).unwrap(),
        ),
    )
    .unwrap()
}

fn record(next: bool) -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion(if next { 2 } else { 1 }),
        value(next),
    )
    .with_owner(owner())
}

fn base() -> SourceDocumentKey {
    SourceDocumentKey::named(
        "d9953522-4412-43b3-9a97-ac63fa5eaef4".parse().unwrap(),
        "recovery.base",
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn properties(next: bool, empty: bool) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id(if empty { "empty" } else { "enemy" })),
        component: ComponentTypeId::new(COMPONENT),
        version: ComponentSchemaVersion(if next { 2 } else { 1 }),
        claims: if empty {
            vec![]
        } else {
            vec![
                PrefabPropertyClaim {
                    path: selector(next),
                    intent: PrefabPropertyIntent::Set(ComponentValue::I64(if next {
                        23000
                    } else {
                        23
                    })),
                },
                PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("armor")),
                    intent: PrefabPropertyIntent::Set(ComponentValue::I64(7)),
                },
            ]
        },
    }
}

fn document(next: bool) -> SceneDocument {
    let mut nested = PrefabOverlay::new(base());
    nested.components = vec![properties(next, false), properties(next, true)];
    let nested = PrefabInstance {
        binding: crate::PrefabInstanceBinding::Forest,
        source: AssetRef::path("missing-nested.prefab.json").unwrap(),
        arguments: None,
        overrides: Some(nested),
    };
    let mut overlay = PrefabOverlay::new(base());
    let mut addition = PrefabAddition::new(id("addition"));
    addition
        .components
        .insert(ComponentTypeId::new(COMPONENT), record(next));
    addition.prefab = Some(nested.clone());
    overlay.additions.push(addition);
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("dormant")),
        value: Some(nested),
    });
    overlay.components.push(PrefabComponentIntent::Set {
        mode: PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("enemy")),
        component: ComponentTypeId::new(COMPONENT),
        record: record(next),
    });
    let mut root = SceneEntityRecord::new(id("root"))
        .with_component(ComponentTypeId::new(COMPONENT), record(next));
    root.prefab = Some(PrefabInstance {
        binding: crate::PrefabInstanceBinding::Forest,
        source: AssetRef::path("missing-outer.prefab.json").unwrap(),
        arguments: None,
        overrides: Some(overlay),
    });
    SceneDocument::from_ordered_roots([
        root,
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("missing.Opaque"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("preserve unavailable bytes".into()),
            ),
        ),
    ])
}

fn validate(
    kind: SourceDocumentKind,
    original: &SceneDocument,
    successor: &SceneDocument,
    upgrade: &AuthoredFieldUpgrade,
    limits: SceneFileLimits,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(), ScenePatchReport> {
    match kind {
        SourceDocumentKind::Scene => crate::__private::validate_authored_upgrade_scene_transition(
            original,
            successor,
            AuthoringDocumentSource::new(AssetRef::path("scenes/recovery.scene.json").unwrap()),
            upgrade,
            limits,
            work,
        ),
        SourceDocumentKind::Prefab => {
            // These decoded input owners belong to the caller, before transition validation.
            let original =
                PrefabDocument::new(original.placements.clone(), original.entities.clone());
            let successor =
                PrefabDocument::new(successor.placements.clone(), successor.entities.clone());
            crate::__private::validate_authored_upgrade_prefab_transition(
                &original,
                &successor,
                AuthoringDocumentSource::new(
                    AssetRef::path("prefabs/recovery.prefab.json").unwrap(),
                ),
                upgrade,
                limits,
                work,
            )
        }
    }
}

fn overlay(document: &mut SceneDocument) -> &mut PrefabOverlay {
    document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn dormant(document: &mut SceneDocument) -> &mut PrefabOverlay {
    overlay(document).instances[0]
        .value
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn field<'a>(record: &'a mut SceneComponentRecord, name: &str) -> &'a mut ComponentValue {
    let ComponentValue::Map(value) = &mut record.value else {
        unreachable!()
    };
    value.get_mut(name).unwrap()
}

fn code(report: &ScenePatchReport) -> &str {
    report.diagnostics.iter().next().unwrap().code().as_str()
}

#[test]
fn stored_scene_and_prefab_pairs_validate_complete_dormant_trees_without_live_owners() {
    let upgrade = upgrade();
    let original = document(false);
    let successor = document(true);
    let original_before = original.clone();
    let successor_before = successor.clone();
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        let mut work = AuthoredSchemaUpgradeWork::default();
        work.retain_preparation_storage(1024).unwrap();
        let entry = work.budget.retention_checkpoint();
        validate(
            kind,
            &original,
            &successor,
            &upgrade,
            SceneFileLimits::default(),
            &mut work,
        )
        .unwrap();
        assert_eq!(work.budget.retention_checkpoint(), entry);
        assert!(work.usage().work_bytes > 1024);
        assert!(work.usage().peak_retained_bytes > 1024);
    }
    assert_eq!(original, original_before);
    assert_eq!(successor, successor_before);
}

#[test]
fn stored_pairs_refuse_unrelated_armor_opaque_payload_and_topology_changes() {
    let upgrade = upgrade();
    let original = document(false);
    for mutation in 0..3 {
        let mut successor = document(true);
        match mutation {
            0 => {
                *field(
                    successor.entities[0]
                        .components
                        .get_mut(&ComponentTypeId::new(COMPONENT))
                        .unwrap(),
                    "armor",
                ) = ComponentValue::I64(99);
            }
            1 => {
                successor.entities[1]
                    .components
                    .get_mut(&ComponentTypeId::new("missing.Opaque"))
                    .unwrap()
                    .value = ComponentValue::String("changed".into());
            }
            _ => successor.placements.reverse(),
        }
        for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
            let report = validate(
                kind,
                &original,
                &successor,
                &upgrade,
                SceneFileLimits::default(),
                &mut AuthoredSchemaUpgradeWork::default(),
            )
            .unwrap_err();
            assert_eq!(code(&report), "scene.authored-schema-upgrade-refused");
        }
    }
}

#[test]
fn affected_complete_and_selected_property_payloads_are_validated_on_both_sides() {
    let upgrade = upgrade();
    for old in [true, false] {
        for mutation in 0..3 {
            let mut original = document(false);
            let mut successor = document(true);
            let document = if old { &mut original } else { &mut successor };
            match mutation {
                0 => {
                    *field(
                        overlay(document).additions[0]
                            .components
                            .get_mut(&ComponentTypeId::new(COMPONENT))
                            .unwrap(),
                        "wire_health",
                    ) = ComponentValue::String("invalid".into());
                }
                1 => {
                    let PrefabComponentIntent::Set { record, .. } =
                        &mut overlay(document).components[0]
                    else {
                        unreachable!()
                    };
                    *field(record, "wire_health") = ComponentValue::String("invalid".into());
                }
                _ => {
                    let PrefabComponentIntent::Properties { claims, .. } =
                        &mut dormant(document).components[0]
                    else {
                        unreachable!()
                    };
                    claims[0].intent =
                        PrefabPropertyIntent::Set(ComponentValue::String("invalid".into()));
                }
            }
            for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
                assert!(
                    validate(
                        kind,
                        &original,
                        &successor,
                        &upgrade,
                        SceneFileLimits::default(),
                        &mut AuthoredSchemaUpgradeWork::default(),
                    )
                    .is_err(),
                    "old={old}, mutation={mutation}, kind={kind:?}",
                );
            }
        }
    }
}

#[test]
fn malformed_unchanged_claims_cannot_hide_in_affected_dormant_batches() {
    let upgrade = upgrade();
    for mutation in 0..4 {
        let mut original = document(false);
        let mut successor = document(true);
        for document in [&mut original, &mut successor] {
            let PrefabComponentIntent::Properties { claims, .. } =
                &mut dormant(document).components[0]
            else {
                unreachable!()
            };
            match mutation {
                0 => {
                    claims[1].intent =
                        PrefabPropertyIntent::Set(ComponentValue::String("invalid".into()));
                }
                1 => {
                    claims[1].path = ComponentPropertyPath::new(ComponentFieldId::new("missing"));
                }
                2 => claims[1].intent = PrefabPropertyIntent::Absent,
                _ => claims.push(claims[0].clone()),
            }
        }
        for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
            assert!(
                validate(
                    kind,
                    &original,
                    &successor,
                    &upgrade,
                    SceneFileLimits::default(),
                    &mut AuthoredSchemaUpgradeWork::default(),
                )
                .is_err(),
                "mutation={mutation}, kind={kind:?}",
            );
        }
    }
}

#[test]
fn dormant_empty_properties_must_advance_the_affected_version() {
    let upgrade = upgrade();
    let original = document(false);
    let mut successor = document(true);
    dormant(&mut successor).components[1] = properties(false, true);
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        assert!(
            validate(
                kind,
                &original,
                &successor,
                &upgrade,
                SceneFileLimits::default(),
                &mut AuthoredSchemaUpgradeWork::default(),
            )
            .is_err()
        );
    }
}

#[test]
fn exact_work_and_aggregate_retention_admit_while_one_short_refuses_and_drops_scratch() {
    let upgrade = upgrade();
    let original = document(false);
    let successor = document(true);
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        let mut measured = AuthoredSchemaUpgradeWork::default();
        measured.retain_preparation_storage(1024).unwrap();
        validate(
            kind,
            &original,
            &successor,
            &upgrade,
            SceneFileLimits::default(),
            &mut measured,
        )
        .unwrap();
        let required = measured.usage();
        for (work_bytes, retained_bytes, succeeds) in [
            (required.work_bytes, required.peak_retained_bytes, true),
            (required.work_bytes - 1, required.peak_retained_bytes, false),
            (required.work_bytes, required.peak_retained_bytes - 1, false),
        ] {
            let mut work = AuthoredSchemaUpgradeWork::new(AuthoredSchemaUpgradeLimits::new(
                ByteLimit::new(work_bytes).unwrap(),
                ByteLimit::new(retained_bytes).unwrap(),
            ));
            work.retain_preparation_storage(1024).unwrap();
            let entry = work.budget.retention_checkpoint();
            let result = validate(
                kind,
                &original,
                &successor,
                &upgrade,
                SceneFileLimits::default(),
                &mut work,
            );
            assert_eq!(result.is_ok(), succeeds, "kind={kind:?}");
            if let Err(report) = result {
                assert_eq!(
                    code(&report),
                    "scene.authored-schema-upgrade-limit-exceeded"
                );
            }
            assert_eq!(work.budget.retention_checkpoint(), entry);
        }
    }
}

#[test]
fn refusals_release_only_scratch_and_keep_the_same_allowance_cumulative_for_retry() {
    let upgrade = upgrade();
    let original = document(false);
    let successor = document(true);
    let mut invalid = successor.clone();
    *field(
        overlay(&mut invalid).additions[0]
            .components
            .get_mut(&ComponentTypeId::new(COMPONENT))
            .unwrap(),
        "wire_health",
    ) = ComponentValue::String("invalid".into());
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        let mut work = AuthoredSchemaUpgradeWork::default();
        work.retain_preparation_storage(1024).unwrap();
        let entry = work.budget.retention_checkpoint();
        assert!(
            validate(
                kind,
                &original,
                &invalid,
                &upgrade,
                SceneFileLimits::default(),
                &mut work,
            )
            .is_err()
        );
        assert_eq!(work.budget.retention_checkpoint(), entry);
        let refused = work.usage();
        assert!(refused.work_bytes > 1024);
        validate(
            kind,
            &original,
            &successor,
            &upgrade,
            SceneFileLimits::default(),
            &mut work,
        )
        .unwrap();
        let completed = work.usage();
        assert!(completed.work_bytes > refused.work_bytes);
        assert!(completed.peak_retained_bytes >= refused.peak_retained_bytes);
        assert_eq!(work.budget.retention_checkpoint(), entry);
        validate(
            kind,
            &original,
            &successor,
            &upgrade,
            SceneFileLimits::default(),
            &mut work,
        )
        .unwrap();
        assert!(work.usage().work_bytes > completed.work_bytes);
        assert_eq!(
            work.usage().peak_retained_bytes,
            completed.peak_retained_bytes
        );
        assert_eq!(work.budget.retention_checkpoint(), entry);
    }
}

#[test]
fn decoded_successors_cannot_bypass_original_publication_limits() {
    let upgrade = upgrade();
    // Isolate the direct value: the original contains three nodes; the new Record adds one.
    let original = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("enemy"))
        .with_component(ComponentTypeId::new(COMPONENT), record(false))]);
    let successor = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("enemy"))
        .with_component(ComponentTypeId::new(COMPONENT), record(true))]);
    let old_prefab = PrefabDocument::new(original.placements.clone(), original.entities.clone());
    let next_prefab = PrefabDocument::new(successor.placements.clone(), successor.entities.clone());
    let old_bytes = original
        .to_json_string()
        .unwrap()
        .len()
        .max(old_prefab.to_json_string().unwrap().len());
    assert!(successor.to_json_string().unwrap().len() > old_bytes);
    assert!(next_prefab.to_json_string().unwrap().len() > old_bytes);
    let encoded = SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(old_bytes).unwrap());
    let values = SceneFileLimits::default().with_component_value_nodes(ItemLimit::new(3).unwrap());
    for kind in [SourceDocumentKind::Scene, SourceDocumentKind::Prefab] {
        for limits in [encoded, values] {
            let mut work = AuthoredSchemaUpgradeWork::default();
            assert!(
                validate(kind, &original, &successor, &upgrade, limits, &mut work).is_err(),
                "kind={kind:?}, limits={limits:?}",
            );
            assert_eq!(work.budget.retention_checkpoint(), (0, 0));
        }
    }
}
