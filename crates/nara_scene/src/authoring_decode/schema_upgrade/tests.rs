use super::*;
use crate::{
    PrefabAddition, PrefabComponentIntent, PrefabEntityTarget, PrefabInstanceClaim, PrefabOverlay,
    PrefabPropertyIntent, SourceDocumentKey, SourceDocumentKind,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_reflect::{
    AuthoringSchemaBundleOwner, ComponentFieldSchema, ComponentMemberPath,
    ComponentMemberPathSegment, ComponentPropertyStep, ComponentSchemaCatalog,
    ComponentShapeMember, ComponentValueKind,
};

const COMPONENT: &str = "trial.enemy.EnemyStats";
const OWNER: &str = "trial.enemy";

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(OWNER).unwrap()
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn next_selector() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("health")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("milli")),
    ])
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
    .with_authoring_initial_value(value(next))
}

fn bases() -> (AuthoringSchemaBasis, AuthoringSchemaBasis) {
    bases_with_schemas(schema(false), schema(true))
}

fn bases_with_schemas(
    previous_schema: ComponentSchema,
    successor_schema: ComponentSchema,
) -> (AuthoringSchemaBasis, AuthoringSchemaBasis) {
    let original = ComponentSchemaCatalog {
        components: vec![previous_schema],
        ..Default::default()
    };
    let mut successor = ComponentSchemaCatalog::successor_of(&original).unwrap();
    successor.components.push(successor_schema);
    let admit = |catalog, previous| {
        let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            owner(),
            catalog,
            previous,
        )])
        .unwrap();
        let adapters = bundle
            .owners()
            .iter()
            .map(|owner| {
                AuthoringAdapterReceipt::from_schema_owner(owner, &Default::default()).unwrap()
            })
            .collect::<Vec<_>>();
        AuthoringSchemaBasis::admit(&bundle, &adapters).unwrap()
    };
    (
        admit(original.clone(), None),
        admit(successor, Some(original)),
    )
}

fn upgrade(
    previous: AuthoringSchemaBasis,
    successor: AuthoringSchemaBasis,
) -> AuthoredFieldUpgrade {
    AuthoredFieldUpgrade::prepare(
        previous,
        successor,
        ComponentTypeId::new(COMPONENT),
        ComponentFieldId::new("health"),
        next_selector(),
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
        "421112bd-693a-4e6b-85bc-318d567b2f21".parse().unwrap(),
        "authored-upgrade.base",
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
                    path: if next {
                        next_selector()
                    } else {
                        ComponentPropertyPath::new(ComponentFieldId::new("health"))
                    },
                    intent: PrefabPropertyIntent::Set(ComponentValue::I64(if next {
                        23000
                    } else {
                        23
                    })),
                    bindings: vec![],
                },
                PrefabPropertyClaim {
                    path: ComponentPropertyPath::new(ComponentFieldId::new("armor")),
                    intent: PrefabPropertyIntent::Set(ComponentValue::I64(7)),
                    bindings: vec![],
                },
            ]
        },
    }
}

fn document(next: bool) -> SceneDocument {
    let mut nested = PrefabOverlay::new(base());
    nested.components = vec![properties(next, false), properties(next, true)];
    let nested = PrefabInstance {
        source: AssetRef::path("nested.prefab.json").unwrap(),
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
    let mut root = SceneEntityRecord::new(id("root"))
        .with_component(ComponentTypeId::new(COMPONENT), record(next));
    root.prefab = Some(PrefabInstance {
        source: AssetRef::path("outer.prefab.json").unwrap(),
        overrides: Some(overlay),
    });
    SceneDocument::from_ordered_roots([
        root,
        SceneEntityRecord::new(id("unknown")).with_component(
            ComponentTypeId::new("missing.Opaque"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("keep".into()),
            ),
        ),
    ])
}

fn classify(document: &SceneDocument, basis: &AuthoringSchemaBasis) -> AuthoringDocumentState {
    let published = SceneDocumentCandidate::decode_json_str(&document.to_json_string().unwrap())
        .unwrap()
        .admit_authoring_with_basis(
            AuthoringDocumentSource::new(AssetRef::path("scenes/upgrade.scene.json").unwrap()),
            basis,
        )
        .unwrap()
        .publish();
    Arc::unwrap_or_clone(published.into_parts().1)
}

fn prepare(
    original: &SceneDocument,
    candidate: &SceneDocument,
    upgrade: &AuthoredFieldUpgrade,
) -> Result<AuthoringDocumentState, ScenePatchReport> {
    classify(original, &upgrade.previous).prepare_authored_upgrade(
        original,
        candidate,
        upgrade,
        &mut |_, bytes| bytes.map(|_| ()).ok_or_else(refused),
    )
}

#[test]
fn direct_nested_and_dormant_declarations_upgrade_together_without_interpreting_unknown_data() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    let candidate = document(true);
    let next = prepare(&original, &candidate, &upgrade).unwrap();
    assert_eq!(next.schema_basis(), upgrade.successor);
    assert_eq!(next.readiness(), AuthoringDocumentReadiness::Degraded);
    assert_eq!(next.unavailable_manifest().record_count(), 1);
    assert_eq!(original.entities[1], candidate.entities[1]);
}

#[test]
fn unchanged_empty_properties_cannot_hide_the_old_version() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    let mut candidate = document(true);
    let nested = candidate.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
        .instances[0]
        .value
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    nested.components[1] = properties(false, true);
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn armor_unknown_data_and_topology_changes_are_outside_the_upgrade() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    for mutation in 0..3 {
        let mut candidate = document(true);
        match mutation {
            0 => {
                let ComponentValue::Map(value) = &mut candidate.entities[0]
                    .components
                    .get_mut(&ComponentTypeId::new(COMPONENT))
                    .unwrap()
                    .value
                else {
                    unreachable!()
                };
                value.insert("armor".into(), ComponentValue::I64(99));
            }
            1 => {
                candidate.entities[1]
                    .components
                    .get_mut(&ComponentTypeId::new("missing.Opaque"))
                    .unwrap()
                    .value = ComponentValue::String("changed".into())
            }
            _ => candidate.roots.reverse(),
        }
        assert!(prepare(&original, &candidate, &upgrade).is_err());
    }
}

#[test]
fn an_equivalent_old_basis_cannot_replace_the_original_source_admission() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let (equivalent, _) = bases();
    let original = document(false);
    let state = classify(&original, &equivalent);
    assert!(
        state
            .prepare_authored_upgrade(&original, &document(true), &upgrade, &mut |_, _| Ok(()),)
            .is_err()
    );
}

fn overlay_mut(document: &mut SceneDocument) -> &mut PrefabOverlay {
    document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn replace_intent(record: SceneComponentRecord) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode: crate::PrefabComponentSetMode::Replace,
        target: PrefabEntityTarget::source(id("enemy")),
        component: ComponentTypeId::new(COMPONENT),
        record,
        bindings: vec![],
    }
}

#[test]
fn an_addition_version_change_requires_a_valid_successor_payload() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    let mut candidate = document(true);
    overlay_mut(&mut candidate).additions[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value = value(false);
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn a_complete_set_version_change_requires_a_valid_successor_payload() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    overlay_mut(&mut original)
        .components
        .push(replace_intent(record(false)));
    let mut candidate = document(true);
    let mut invalid = record(false);
    invalid.version = ComponentSchemaVersion(2);
    overlay_mut(&mut candidate)
        .components
        .push(replace_intent(invalid));
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn an_invalid_old_addition_cannot_be_admitted_by_a_valid_successor() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let ComponentValue::Map(value) = &mut overlay_mut(&mut original).additions[0]
        .components
        .get_mut(&ComponentTypeId::new(COMPONENT))
        .unwrap()
        .value
    else {
        unreachable!()
    };
    value.insert(
        "wire_health".into(),
        ComponentValue::String("invalid".into()),
    );
    assert!(prepare(&original, &document(true), &upgrade).is_err());
}

#[test]
fn a_remapped_property_requires_a_valid_selected_payload() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    let mut candidate = document(true);
    let mut intent = properties(true, false);
    let PrefabComponentIntent::Properties { claims, .. } = &mut intent else {
        unreachable!()
    };
    claims[0].intent = PrefabPropertyIntent::Set(ComponentValue::String("invalid".into()));
    overlay_mut(&mut candidate).components.push(intent);
    let mut original = original;
    overlay_mut(&mut original)
        .components
        .push(properties(false, false));
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn unchanged_property_payloads_in_an_affected_batch_are_validated_on_both_sides() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let mut candidate = document(true);
    for (next, document) in [(false, &mut original), (true, &mut candidate)] {
        let mut intent = properties(next, false);
        let PrefabComponentIntent::Properties { claims, .. } = &mut intent else {
            unreachable!()
        };
        claims[1].intent = PrefabPropertyIntent::Set(ComponentValue::String("invalid".into()));
        overlay_mut(document).components.push(intent);
    }
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn an_unchanged_unknown_selector_cannot_hide_in_an_upgraded_batch() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let mut candidate = document(true);
    for (next, document) in [(false, &mut original), (true, &mut candidate)] {
        let mut intent = properties(next, false);
        let PrefabComponentIntent::Properties { claims, .. } = &mut intent else {
            unreachable!()
        };
        claims.push(PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("missing")),
            intent: PrefabPropertyIntent::Set(ComponentValue::I64(1)),
            bindings: vec![],
        });
        overlay_mut(document).components.push(intent);
    }
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn a_valid_complete_set_upgrades_under_its_own_schema() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let mut candidate = document(true);
    overlay_mut(&mut original)
        .components
        .push(replace_intent(record(false)));
    overlay_mut(&mut candidate)
        .components
        .push(replace_intent(record(true)));
    assert!(prepare(&original, &candidate, &upgrade).is_ok());
}

#[test]
fn affected_claims_cannot_remove_a_required_field_without_a_default() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let mut candidate = document(true);
    for (next, document) in [(false, &mut original), (true, &mut candidate)] {
        let mut intent = properties(next, false);
        let PrefabComponentIntent::Properties { claims, .. } = &mut intent else {
            unreachable!()
        };
        claims[1].intent = PrefabPropertyIntent::Absent;
        overlay_mut(document).components.push(intent);
    }
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}

#[test]
fn validation_returns_the_original_owner_admission_refusal() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let original = document(false);
    let candidate = document(true);
    let state = classify(&original, &upgrade.previous);
    for refused_stage in [
        "authored-upgrade-record-validation",
        "authored-upgrade-property-paths",
        "authored-upgrade-property-path-index",
        "authored-upgrade-property-selector",
        "authored-upgrade-property-value",
        "publication-buffer",
    ] {
        let report = state
            .prepare_authored_upgrade(&original, &candidate, &upgrade, &mut |stage, bytes| {
                if stage == refused_stage {
                    let mut diagnostics = DiagnosticReport::default();
                    diagnostics.push(diagnostic_error(
                        "trial.authored-upgrade-work-refused",
                        "The project owner refused further upgrade validation work",
                    ));
                    Err(ScenePatchReport {
                        applied: false,
                        inverse: None,
                        diagnostics,
                    })
                } else {
                    bytes.map(|_| ()).ok_or_else(refused)
                }
            })
            .unwrap_err();
        assert_eq!(
            report.diagnostics.iter().next().unwrap().code().as_str(),
            "trial.authored-upgrade-work-refused",
            "stage={refused_stage}",
        );
    }
}

#[test]
fn selected_field_member_retirement_is_outside_the_value_upgrade_footprint() {
    let mut old = schema(false);
    let mut next = schema(true);
    let member = |id| ComponentMemberPathSegment::Member(ComponentFieldId::new(id));
    old.fields[0].member_tombstones = vec![ComponentMemberPath::new([
        member("retired"),
        member("child"),
    ])];
    next.fields[0].member_tombstones = vec![ComponentMemberPath::new([member("retired")])];
    // Ordinary owner lineage admits the broader tombstone. This one-field value operation
    // has no authority to widen permanent member retirement at the same time.
    let (previous, successor) = bases_with_schemas(old, next);
    assert!(
        AuthoredFieldUpgrade::prepare(
            previous,
            successor,
            ComponentTypeId::new(COMPONENT),
            ComponentFieldId::new("health"),
            next_selector(),
            &mut ComponentPropertyReadBudget::new(
                ItemLimit::new(4096).unwrap(),
                ByteLimit::new(1024 * 1024).unwrap(),
            ),
        )
        .is_err()
    );
}

#[test]
fn duplicated_property_paths_remain_invalid_on_both_sides_of_an_upgrade() {
    let (previous, successor) = bases();
    let upgrade = upgrade(previous, successor);
    let mut original = document(false);
    let mut candidate = document(true);
    for (next, document) in [(false, &mut original), (true, &mut candidate)] {
        let mut intent = properties(next, false);
        let PrefabComponentIntent::Properties { claims, .. } = &mut intent else {
            unreachable!()
        };
        claims.push(claims[0].clone());
        overlay_mut(document).components.push(intent);
    }
    assert!(prepare(&original, &candidate, &upgrade).is_err());
}
