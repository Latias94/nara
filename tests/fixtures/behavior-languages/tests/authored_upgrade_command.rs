//! Independent, window-free consumer of the project's explicit Rust data proposal command.

#[path = "support/authored_upgrade_command.rs"]
mod authored_upgrade_command;
#[path = "support/authored_upgrade_session.rs"]
mod authored_upgrade_session;

use authored_upgrade_command::{
    COMPONENT, OWNER, UpgradeError, V2, health_property_v2, plan_prefab, plan_scene,
    successor_catalog, v1_catalog,
};
use nara_asset::AssetRef;
use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, ReferenceInputId};
use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, EntityReference, PersistentComponentSchemaOwnerId, resolve_component_property,
    validate_component_value,
};
use nara_scene::{
    PrefabAddition, PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument,
    PrefabDocumentCandidate, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstance,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabPropertyClaim,
    PrefabPropertyIntent, PrefabReferenceArguments, SceneComponentRecord, SceneDocument,
    SceneDocumentCandidate, SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePlacement,
    ScenePlacementParent, SourceDocumentKey, SourceDocumentKind,
};

const UNKNOWN: &str = "unavailable.enemy.Opaque";
const FOREIGN: &str = "unavailable.owner";
const V1: ComponentSchemaVersion = ComponentSchemaVersion::ONE;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component() -> ComponentTypeId {
    ComponentTypeId::new(COMPONENT)
}

fn owner(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn input(value: &str) -> ReferenceInputId {
    ReferenceInputId::new(value).unwrap()
}

fn root(field: &str) -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new(field))
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "f1390a84-11df-49b0-a901-d7a045141039".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}

fn target(name: &str) -> PrefabEntityTarget {
    PrefabEntityTarget::source(id(name))
}

fn deep_target(name: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(id("nested-anchor")),
                expected_base: key("nested"),
            },
            PrefabInstanceHop {
                anchor: PrefabEntityOrigin::InstanceAddition(id("deep-anchor")),
                expected_base: key("deep"),
            },
        ],
        entity: PrefabEntityOrigin::SourceEntity(id(name)),
    }
}

fn instance(name: &str, overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        arguments: Some(PrefabReferenceArguments {
            base: key(name),
            values: [
                (input("opaque-input"), authored_reference()),
                (
                    input("unused-parameter"),
                    EntityReference::Authored {
                        reference: AuthoredEntityReference::input(input("opaque-input")),
                    },
                ),
            ]
            .into(),
        }),
        overrides: overlay,
        ..PrefabInstance::forest(AssetRef::path(format!("{name}.prefab.json")).unwrap())
    }
}

fn authored_reference() -> EntityReference {
    EntityReference::Authored {
        reference: deep_target("opaque-target").into(),
    }
}

fn authored_input() -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: deep_target("unused-endpoint").through,
            target: AuthoredReferenceTarget::Input(input("opaque-input")),
        },
    }
}

fn opaque() -> ComponentValue {
    ComponentValue::map([
        (
            "wire_health",
            ComponentValue::String("not project health".into()),
        ),
        ("wire_armor", ComponentValue::Null),
        (
            "health",
            ComponentValue::map([("milli", ComponentValue::U64(17))]),
        ),
        (
            "opaque",
            ComponentValue::List(vec![
                ComponentValue::Bool(true),
                ComponentValue::String("\0future payload / 未知".into()),
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("opaque-reference"),
                }),
                ComponentValue::EntityReference(authored_reference()),
                ComponentValue::EntityReference(authored_input()),
            ]),
        ),
    ])
}

fn unknown_record() -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion(91), opaque())
        .with_owner(owner(FOREIGN))
        // Explicit values hide this unavailable owner's reference defaults. Neither the
        // matching field IDs nor the inline routes authorize the EnemyStats converter.
        .with_contextual_default(
            ComponentFieldId::new("health"),
            ComponentValue::EntityReference(authored_reference()),
        )
        .with_contextual_default(
            ComponentFieldId::new("armor"),
            ComponentValue::EntityReference(authored_input()),
        )
}

fn stats_value(health: ComponentValue, armor: i64) -> ComponentValue {
    ComponentValue::map([
        ("wire_health", health),
        ("wire_armor", ComponentValue::I64(armor)),
    ])
}

fn record(next: bool, whole: i64, milli: i64, armor: i64) -> SceneComponentRecord {
    // Expected milli values are fixture literals, not recomputed by the test's oracle.
    let health = if next {
        ComponentValue::map([("milli", ComponentValue::I64(milli))])
    } else {
        ComponentValue::I64(whole)
    };
    SceneComponentRecord::new(if next { V2 } else { V1 }, stats_value(health, armor))
        .with_owner(owner(OWNER))
}

fn health_pin(next: bool, whole: i64, milli: i64) -> PrefabPropertyClaim {
    PrefabPropertyClaim {
        path: if next {
            root("health").with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
                "milli",
            ))])
        } else {
            root("health")
        },
        intent: PrefabPropertyIntent::Set(ComponentValue::I64(if next { milli } else { whole })),
    }
}

fn armor_pin(value: i64) -> PrefabPropertyClaim {
    PrefabPropertyClaim {
        path: root("armor"),
        intent: PrefabPropertyIntent::Set(ComponentValue::I64(value)),
    }
}

fn properties(
    next: bool,
    target: PrefabEntityTarget,
    claims: Vec<PrefabPropertyClaim>,
) -> PrefabComponentIntent {
    PrefabComponentIntent::Properties {
        target,
        component: component(),
        version: if next { V2 } else { V1 },
        claims,
    }
}

fn set(
    mode: PrefabComponentSetMode,
    target: PrefabEntityTarget,
    record: SceneComponentRecord,
) -> PrefabComponentIntent {
    PrefabComponentIntent::Set {
        mode,
        target,
        component: component(),
        record,
    }
}

fn fixture(next: bool) -> SceneDocument {
    let mut beneath_addition_claim = PrefabOverlay::new(key("deep"));
    beneath_addition_claim.components.push(properties(
        next,
        deep_target("deep-pinned"),
        vec![health_pin(next, 14, 14000)],
    ));
    let mut beneath_addition = PrefabOverlay::new(key("nested"));
    beneath_addition.components.push(set(
        PrefabComponentSetMode::Replace,
        target("nested-set"),
        record(next, 9, 9000, 3),
    ));
    beneath_addition.instances.push(PrefabInstanceClaim {
        target: target("deep-anchor"),
        value: Some(instance("deep", Some(beneath_addition_claim))),
    });

    let mut beneath_claim = PrefabOverlay::new(key("nested"));
    let mut nested_addition = PrefabAddition::new(id("claim-addition"));
    nested_addition
        .components
        .insert(component(), record(next, 11, 11000, 2));
    nested_addition.prefab = Some(instance("empty", Some(PrefabOverlay::new(key("empty")))));
    beneath_claim.additions.push(nested_addition);
    beneath_claim.components.push(properties(
        next,
        target("claim-pinned"),
        vec![health_pin(next, 12, 12000)],
    ));

    let mut overlay = PrefabOverlay::new(key("base"));
    let mut addition = PrefabAddition::new(id("added"));
    addition
        .components
        .insert(component(), record(next, -3, -3000, 4));
    addition
        .components
        .insert(ComponentTypeId::new(UNKNOWN), unknown_record());
    addition.prefab = Some(instance("nested", Some(beneath_addition)));
    overlay.additions.push(addition);
    let mut foreign_addition = PrefabAddition::new(id("foreign-added"));
    foreign_addition
        .components
        .insert(component(), unknown_record());
    overlay.additions.push(foreign_addition);
    overlay.instances = vec![
        PrefabInstanceClaim {
            target: deep_target("claim-instance"),
            value: Some(instance("nested", Some(beneath_claim))),
        },
        PrefabInstanceClaim {
            target: target("detached"),
            value: None,
        },
        PrefabInstanceClaim {
            target: target("without-overlay"),
            value: Some(instance("plain", None)),
        },
    ];
    overlay.components = vec![
        set(
            PrefabComponentSetMode::Replace,
            target("set"),
            record(next, 77, 77000, 6),
        ),
        set(
            PrefabComponentSetMode::Add,
            deep_target("set-add"),
            record(next, 8, 8000, 7),
        ),
        // This health pin equals the old creation value. It must remain an explicit pin.
        properties(
            next,
            target("pinned"),
            vec![armor_pin(10), health_pin(next, 100, 100000)],
        ),
        properties(next, target("armor-only"), vec![armor_pin(24)]),
        properties(next, target("empty"), Vec::new()),
        properties(next, target("dormant"), vec![health_pin(next, -9, -9000)]),
        PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: deep_target("unknown-set"),
            component: ComponentTypeId::new(UNKNOWN),
            record: unknown_record(),
        },
        PrefabComponentIntent::Properties {
            target: target("unknown-properties"),
            component: ComponentTypeId::new(UNKNOWN),
            version: ComponentSchemaVersion(123),
            claims: vec![
                PrefabPropertyClaim {
                    path: root("health")
                        .with_steps([ComponentPropertyStep::MapEntry("unavailable-member".into())]),
                    intent: PrefabPropertyIntent::Set(opaque()),
                },
                PrefabPropertyClaim {
                    path: root("future-absence"),
                    intent: PrefabPropertyIntent::Absent,
                },
            ],
        },
        PrefabComponentIntent::Suppress {
            target: target("suppressed-component"),
            component: component(),
        },
        PrefabComponentIntent::Set {
            mode: PrefabComponentSetMode::Replace,
            target: target("foreign-set"),
            component: component(),
            record: unknown_record(),
        },
        set(
            PrefabComponentSetMode::Replace,
            target("dormant-set"),
            record(next, 16, 16000, 1),
        ),
    ];
    overlay.placements.push(SceneEntityPlacement::new(
        PrefabEntityTarget::addition(id("added")),
        ScenePlacement::before(
            ScenePlacementParent::Entity(deep_target("parent")),
            target("dormant"),
        ),
    ));
    // The command must visit declarations that are hidden by this suppression.
    overlay.suppressions.push(target("dormant"));
    overlay.suppressions.push(target("dormant-set"));

    let mut direct = SceneEntityRecord::new(id("direct"))
        .with_component(component(), record(next, 105, 105000, 10))
        .with_component(ComponentTypeId::new(UNKNOWN), unknown_record());
    direct.prefab = Some(instance("base", Some(overlay.clone())));
    let mut repeated = SceneEntityRecord::new(id("repeated"));
    repeated.prefab = Some(instance("base", Some(overlay)));
    // Explicitly different ownership is not this project's EnemyStats, despite its type ID.
    let child = SceneEntityRecord::new(id("child")).with_component(component(), unknown_record());
    SceneDocument::new(
        [
            SceneEntityPlacement::root(id("repeated")),
            SceneEntityPlacement::root(id("direct")),
            SceneEntityPlacement::new(
                target("child"),
                ScenePlacement::last(ScenePlacementParent::Entity(target("direct"))),
            ),
        ],
        [direct, child, repeated],
    )
}

fn prefab(scene: &SceneDocument) -> PrefabDocument {
    PrefabDocument {
        inputs: [input("opaque-input"), input("unused-parameter")].into(),
        placements: scene.placements.clone(),
        entities: scene.entities.clone(),
    }
}

fn entity_mut<'a>(scene: &'a mut SceneDocument, name: &str) -> &'a mut SceneEntityRecord {
    scene
        .entities
        .iter_mut()
        .find(|entity| entity.id == id(name))
        .unwrap()
}

fn overlay_mut(scene: &mut SceneDocument) -> &mut PrefabOverlay {
    entity_mut(scene, "direct")
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn addition_overlay(overlay: &mut PrefabOverlay) -> &mut PrefabOverlay {
    overlay.additions[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn claim_overlay(overlay: &mut PrefabOverlay) -> &mut PrefabOverlay {
    overlay.instances[0]
        .value
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap()
}

fn set_record(overlay: &mut PrefabOverlay, index: usize) -> &mut SceneComponentRecord {
    let PrefabComponentIntent::Set { record, .. } = &mut overlay.components[index] else {
        panic!("fixture requires a Set record")
    };
    record
}

#[derive(Clone, Copy, Debug)]
enum RecordSite {
    Direct,
    Addition,
    SetReplace,
    SetAdd,
    AdditionNested,
    ClaimNested,
    DormantSet,
}

const RECORD_SITES: [RecordSite; 7] = [
    RecordSite::Direct,
    RecordSite::Addition,
    RecordSite::SetReplace,
    RecordSite::SetAdd,
    RecordSite::AdditionNested,
    RecordSite::ClaimNested,
    RecordSite::DormantSet,
];

fn record_at(scene: &mut SceneDocument, site: RecordSite) -> &mut SceneComponentRecord {
    match site {
        RecordSite::Direct => entity_mut(scene, "direct")
            .components
            .get_mut(&component())
            .unwrap(),
        RecordSite::Addition => overlay_mut(scene).additions[0]
            .components
            .get_mut(&component())
            .unwrap(),
        RecordSite::SetReplace => set_record(overlay_mut(scene), 0),
        RecordSite::SetAdd => set_record(overlay_mut(scene), 1),
        RecordSite::AdditionNested => set_record(addition_overlay(overlay_mut(scene)), 0),
        RecordSite::ClaimNested => claim_overlay(overlay_mut(scene)).additions[0]
            .components
            .get_mut(&component())
            .unwrap(),
        RecordSite::DormantSet => set_record(overlay_mut(scene), 10),
    }
}

#[derive(Clone, Copy, Debug)]
enum BatchSite {
    Both,
    ArmorOnly,
    Empty,
    Dormant,
    AdditionThenClaim,
    Claim,
}

const BATCH_SITES: [BatchSite; 6] = [
    BatchSite::Both,
    BatchSite::ArmorOnly,
    BatchSite::Empty,
    BatchSite::Dormant,
    BatchSite::AdditionThenClaim,
    BatchSite::Claim,
];

fn batch_at(
    scene: &mut SceneDocument,
    site: BatchSite,
) -> (&mut ComponentSchemaVersion, &mut Vec<PrefabPropertyClaim>) {
    let overlay = overlay_mut(scene);
    let (overlay, index) = match site {
        BatchSite::Both => (overlay, 2),
        BatchSite::ArmorOnly => (overlay, 3),
        BatchSite::Empty => (overlay, 4),
        BatchSite::Dormant => (overlay, 5),
        BatchSite::AdditionThenClaim => (claim_overlay(addition_overlay(overlay)), 0),
        BatchSite::Claim => (claim_overlay(overlay), 0),
    };
    let PrefabComponentIntent::Properties {
        version, claims, ..
    } = &mut overlay.components[index]
    else {
        panic!("fixture requires a Properties batch")
    };
    (version, claims)
}

fn assert_refused_unchanged(scene: &SceneDocument, expected: UpgradeError) {
    let before = scene.clone();
    let encoded = serde_json::to_vec(scene).unwrap();
    assert_eq!(plan_scene(scene).unwrap_err(), expected);
    assert_eq!(scene, &before);
    assert_eq!(serde_json::to_vec(scene).unwrap(), encoded);
    let source = prefab(scene);
    let before = source.clone();
    let encoded = serde_json::to_vec(&source).unwrap();
    assert_eq!(plan_prefab(&source).unwrap_err(), expected);
    assert_eq!(source, before);
    assert_eq!(serde_json::to_vec(&source).unwrap(), encoded);
}

#[test]
fn scene_and_prefab_plans_convert_the_complete_stored_tree_and_preserve_every_other_fragment() {
    let mut source = fixture(false);
    let mut expected = fixture(true);
    // Proposal production preserves unselected payload fields too. Their final validity is
    // independently decided by the owning Host/Scene admission, not by this command.
    for document in [&mut source, &mut expected] {
        let ComponentValue::Map(fields) = &mut record_at(document, RecordSite::Direct).value else {
            unreachable!()
        };
        fields.insert("unselected-future-payload".into(), opaque());
    }
    let before = serde_json::to_vec(&source).unwrap();
    let plan = plan_scene(&source).unwrap();
    assert!(std::ptr::eq(plan.original(), &source));
    assert_eq!(plan.converted(), &expected);
    assert_eq!(plan.into_document(), expected);
    assert_eq!(serde_json::to_vec(&source).unwrap(), before);

    let source = prefab(&source);
    let expected = prefab(&expected);
    let before = serde_json::to_vec(&source).unwrap();
    let plan = plan_prefab(&source).unwrap();
    assert!(std::ptr::eq(plan.original(), &source));
    assert_eq!(plan.converted(), &expected);
    assert_eq!(plan.into_document(), expected);
    assert_eq!(serde_json::to_vec(&source).unwrap(), before);
}

#[test]
fn real_json_and_ron_wire_fixtures_preserve_unknown_declarations_through_the_proposal() {
    for (source, expected) in [(fixture(false), fixture(true))] {
        let json = SceneDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let ron = SceneDocumentCandidate::decode_ron_str(&source.to_ron_string().unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        for decoded in [json, ron] {
            assert_eq!(decoded, source);
            let converted = plan_scene(&decoded).unwrap().into_document();
            assert_eq!(converted, expected);
            assert_eq!(
                SceneDocumentCandidate::decode_json_str(&converted.to_json_string().unwrap())
                    .unwrap()
                    .into_composition_source()
                    .unwrap(),
                expected,
            );
        }
        let source = prefab(&source);
        let expected = prefab(&expected);
        let json = PrefabDocumentCandidate::decode_json_str(&source.to_json_string().unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        let ron = PrefabDocumentCandidate::decode_ron_str(&source.to_ron_string().unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
        for decoded in [json, ron] {
            assert_eq!(decoded, source);
            let converted = plan_prefab(&decoded).unwrap().into_document();
            assert_eq!(converted, expected);
            assert_eq!(
                PrefabDocumentCandidate::decode_ron_str(&converted.to_ron_string().unwrap())
                    .unwrap()
                    .into_composition_source()
                    .unwrap(),
                expected,
            );
        }
    }
}

#[test]
fn the_plan_preserves_noncanonical_storage_order_instead_of_rebuilding_topology() {
    let mut source = fixture(false);
    let mut expected = fixture(true);
    source.placements.reverse();
    source.entities.reverse();
    expected.placements.reverse();
    expected.entities.reverse();
    assert_eq!(plan_scene(&source).unwrap().into_document(), expected);
    assert_eq!(
        plan_prefab(&prefab(&source)).unwrap().into_document(),
        prefab(&expected)
    );
}

#[test]
fn catalogs_and_values_use_real_public_schemas_and_the_actual_creation_metadata_predecessor() {
    let mut previous = v1_catalog();
    previous.components[0].authoring_initial_value =
        Some(stats_value(ComponentValue::I64(250), 25));
    let mut other = previous.components[0].clone();
    other.id = ComponentTypeId::new("trial.enemy.Unchanged");
    other.aliases = vec!["Unchanged".into()];
    previous.components.push(other.clone());
    previous
        .type_tombstones
        .push(ComponentTypeId::new("trial.enemy.Retired"));
    previous.canonicalize();
    let before = previous.clone();
    let next = successor_catalog(&previous).unwrap();
    assert_eq!(previous, before);
    assert_eq!(next.generation, previous.generation + 1);
    assert_eq!(next.predecessor, Some(previous.fingerprint()));
    assert_ne!(next.predecessor, Some(v1_catalog().fingerprint()));
    assert_eq!(next.type_tombstones, previous.type_tombstones);
    assert_eq!(
        next.components.iter().find(|schema| schema.id == other.id),
        Some(&other)
    );
    let old_owner =
        ComponentSchemaOwnerSnapshot::admit(owner(OWNER), previous.clone(), None).unwrap();
    let next_owner =
        ComponentSchemaOwnerSnapshot::admit(owner(OWNER), next, Some(previous)).unwrap();
    let old = old_owner.schema(&component()).unwrap();
    let next = next_owner.schema(&component()).unwrap();
    for schema in [old, next] {
        assert!(
            schema
                .fields
                .iter()
                .all(|field| field.required && field.default_value.is_none())
        );
    }
    let old_health = old
        .fields
        .iter()
        .find(|field| field.id.as_str() == "health")
        .unwrap();
    let new_health = next
        .fields
        .iter()
        .find(|field| field.id.as_str() == "health")
        .unwrap();
    assert_eq!(new_health.id, old_health.id);
    assert_eq!(new_health.path, old_health.path);
    assert_eq!(new_health.aliases, old_health.aliases);
    assert_eq!(new_health.capabilities, old_health.capabilities);
    assert_eq!(new_health.member_tombstones, old_health.member_tombstones);
    assert_eq!(
        old.fields.iter().find(|field| field.id.as_str() == "armor"),
        next.fields
            .iter()
            .find(|field| field.id.as_str() == "armor"),
    );
    assert_eq!(
        next.authoring_initial_value,
        Some(stats_value(
            ComponentValue::map([("milli", ComponentValue::I64(250000))]),
            25,
        ))
    );
    validate_component_value(next, next.authoring_initial_value.as_ref().unwrap()).unwrap();
    // A Record is closed and its identified milli member is I64, not an opaque map.
    assert!(
        new_health
            .shape
            .validate_value(&ComponentValue::map([
                ("milli", ComponentValue::I64(1)),
                ("extra", ComponentValue::I64(2)),
            ]))
            .is_err()
    );
    assert!(
        new_health
            .shape
            .validate_value(&ComponentValue::map([("milli", ComponentValue::U64(1)),]))
            .is_err()
    );

    let mut source = fixture(false);
    let mut converted = plan_scene(&source).unwrap().into_document();
    for site in RECORD_SITES {
        let old_record = record_at(&mut source, site);
        let new_record = record_at(&mut converted, site);
        validate_component_value(old, &old_record.value).unwrap();
        validate_component_value(next, &new_record.value).unwrap();
        assert_eq!(
            new_record.value.get("wire_armor"),
            old_record.value.get("wire_armor")
        );
        assert!(resolve_component_property(old, V1, &old_record.value, &root("health"),).is_ok());
        assert!(
            resolve_component_property(next, V2, &new_record.value, &health_property_v2(),).is_ok()
        );
    }
}

#[test]
fn empty_and_armor_only_batches_advance_without_dropping_default_equal_or_dormant_pins() {
    let mut converted = plan_scene(&fixture(false)).unwrap().into_document();
    for site in BATCH_SITES {
        let (version, _) = batch_at(&mut converted, site);
        assert_eq!(*version, V2, "{site:?}");
    }
    let (_, claims) = batch_at(&mut converted, BatchSite::Both);
    assert_eq!(claims, &vec![armor_pin(10), health_pin(true, 100, 100000)]);
    let (_, claims) = batch_at(&mut converted, BatchSite::ArmorOnly);
    assert_eq!(claims, &vec![armor_pin(24)]);
    assert!(batch_at(&mut converted, BatchSite::Empty).1.is_empty());
    assert_eq!(
        batch_at(&mut converted, BatchSite::Dormant).1,
        &vec![health_pin(true, -9, -9000)]
    );
}

#[test]
fn record_versions_and_shapes_are_checked_at_every_location_without_touching_the_input() {
    let mut cases = vec![
        (ComponentValue::Null, UpgradeError::RecordShape),
        (ComponentValue::List(Vec::new()), UpgradeError::RecordShape),
        (
            ComponentValue::map([("wire_armor", ComponentValue::I64(1))]),
            UpgradeError::HealthShape,
        ),
        (
            ComponentValue::map([("wire_health", ComponentValue::I64(1))]),
            UpgradeError::ArmorShape,
        ),
        (
            stats_value(ComponentValue::Null, 1),
            UpgradeError::HealthShape,
        ),
        (
            stats_value(ComponentValue::U64(1), 1),
            UpgradeError::HealthShape,
        ),
        (
            stats_value(
                ComponentValue::map([("milli", ComponentValue::I64(1000))]),
                1,
            ),
            UpgradeError::HealthShape,
        ),
        (
            ComponentValue::map([
                ("wire_health", ComponentValue::I64(1)),
                ("wire_armor", ComponentValue::String("invalid armor".into())),
            ]),
            UpgradeError::ArmorShape,
        ),
        (
            stats_value(ComponentValue::I64(i64::MAX), 1),
            UpgradeError::HealthOverflow(i64::MAX),
        ),
        (
            stats_value(ComponentValue::I64(i64::MIN), 1),
            UpgradeError::HealthOverflow(i64::MIN),
        ),
    ];
    for reference in [authored_reference(), authored_input()] {
        cases.push((
            stats_value(ComponentValue::EntityReference(reference.clone()), 1),
            UpgradeError::HealthShape,
        ));
        cases.push((
            ComponentValue::map([
                ("wire_health", ComponentValue::I64(1)),
                ("wire_armor", ComponentValue::EntityReference(reference)),
            ]),
            UpgradeError::ArmorShape,
        ));
    }
    for site in RECORD_SITES {
        for version in [ComponentSchemaVersion(0), V2, ComponentSchemaVersion(7)] {
            let mut source = fixture(false);
            record_at(&mut source, site).version = version;
            assert_refused_unchanged(&source, UpgradeError::Version(version));
        }
        for (value, error) in &cases {
            let mut source = fixture(false);
            record_at(&mut source, site).value = value.clone();
            assert_refused_unchanged(&source, error.clone());
        }
    }
}

#[test]
fn every_properties_batch_checks_its_version_even_when_no_health_conversion_is_needed() {
    for site in BATCH_SITES {
        for version in [ComponentSchemaVersion(0), V2, ComponentSchemaVersion(19)] {
            let mut source = fixture(false);
            *batch_at(&mut source, site).0 = version;
            assert_refused_unchanged(&source, UpgradeError::Version(version));
        }
    }
    assert_refused_unchanged(&fixture(true), UpgradeError::Version(V2));
}

#[test]
fn invalid_selectors_absence_and_inline_property_values_refuse_including_dormant_claims() {
    let mut cases = vec![
        (
            root("health"),
            PrefabPropertyIntent::Absent,
            UpgradeError::RootAbsent(ComponentFieldId::new("health")),
        ),
        (
            root("armor"),
            PrefabPropertyIntent::Absent,
            UpgradeError::RootAbsent(ComponentFieldId::new("armor")),
        ),
        (
            root("health"),
            PrefabPropertyIntent::Set(ComponentValue::Bool(true)),
            UpgradeError::HealthShape,
        ),
        (
            root("health"),
            PrefabPropertyIntent::Set(ComponentValue::map([("milli", ComponentValue::I64(1000))])),
            UpgradeError::HealthShape,
        ),
        (
            root("armor"),
            PrefabPropertyIntent::Set(ComponentValue::U64(10)),
            UpgradeError::ArmorShape,
        ),
        (
            root("health"),
            PrefabPropertyIntent::Set(ComponentValue::I64(i64::MAX)),
            UpgradeError::HealthOverflow(i64::MAX),
        ),
        (
            root("health"),
            PrefabPropertyIntent::Set(ComponentValue::I64(i64::MIN)),
            UpgradeError::HealthOverflow(i64::MIN),
        ),
    ];
    for path in [
        root("wire_health"),
        root("unknown-field"),
        root("health").with_steps([ComponentPropertyStep::Member(ComponentFieldId::new(
            "milli",
        ))]),
        root("armor").with_steps([ComponentPropertyStep::MapEntry("anything".into())]),
    ] {
        cases.push((
            path.clone(),
            PrefabPropertyIntent::Set(ComponentValue::I64(1)),
            UpgradeError::Selector(path),
        ));
    }
    for site in BATCH_SITES {
        for (path, intent, error) in &cases {
            let mut source = fixture(false);
            *batch_at(&mut source, site).1 = vec![PrefabPropertyClaim {
                path: path.clone(),
                intent: intent.clone(),
            }];
            assert_refused_unchanged(&source, error.clone());
        }
        for field in ["health", "armor"] {
            for reference in [authored_reference(), authored_input()] {
                let mut source = fixture(false);
                *batch_at(&mut source, site).1 = vec![PrefabPropertyClaim {
                    path: root(field),
                    intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(reference)),
                }];
                assert_refused_unchanged(
                    &source,
                    if field == "health" {
                        UpgradeError::HealthShape
                    } else {
                        UpgradeError::ArmorShape
                    },
                );
            }
        }
    }
}

#[test]
fn hidden_contextual_defaults_on_required_fields_refuse_at_every_record_location() {
    for site in RECORD_SITES {
        for field in ["health", "armor"] {
            for value in [
                ComponentValue::I64(7),
                ComponentValue::Null,
                ComponentValue::EntityReference(authored_reference()),
                ComponentValue::EntityReference(authored_input()),
            ] {
                let mut source = fixture(false);
                record_at(&mut source, site)
                    .contextual_defaults
                    .insert(ComponentFieldId::new(field), value);
                assert_refused_unchanged(
                    &source,
                    UpgradeError::ContextualDefault(ComponentFieldId::new(field)),
                );
            }
        }
    }
}

#[test]
fn unknown_owners_keep_inline_values_and_hidden_context_even_for_the_affected_type_id() {
    let source = fixture(false);
    let mut converted = plan_scene(&source).unwrap().into_document();
    let opaque = unknown_record();
    assert!(!opaque.contextual_defaults.is_empty());
    assert_eq!(
        entity_mut(&mut converted, "child").components[&component()],
        opaque,
    );
    let overlay = overlay_mut(&mut converted);
    assert_eq!(overlay.additions[1].components[&component()], opaque);
    assert_eq!(set_record(overlay, 9), &opaque);
    assert_eq!(
        overlay.additions[0].components[&ComponentTypeId::new(UNKNOWN)],
        opaque,
    );
    assert_eq!(set_record(overlay, 6), &opaque);
}

#[test]
fn duplicate_scalar_root_claims_refuse_without_publishing_a_partial_conversion() {
    for site in BATCH_SITES {
        for claim in [health_pin(false, 100, 100000), armor_pin(10)] {
            let mut source = fixture(false);
            *batch_at(&mut source, site).1 = vec![claim.clone(), claim.clone()];
            assert_refused_unchanged(&source, UpgradeError::DuplicateClaim(claim.path.root));
        }
    }
}

#[test]
fn checked_scaling_accepts_signed_boundaries_and_zero_but_refuses_the_next_point() {
    for (whole, expected) in [
        (0, 0),
        (-1, -1000),
        (i64::MAX / 1000, 9_223_372_036_854_775_000),
        (i64::MIN / 1000, -9_223_372_036_854_775_000),
    ] {
        let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("enemy"))
            .with_component(component(), record(false, whole, 0, 5))]);
        let converted = plan_scene(&source).unwrap().into_document();
        assert_eq!(
            converted.entities[0].components[&component()],
            record(true, 0, expected, 5)
        );
    }
    for whole in [i64::MAX / 1000 + 1, i64::MIN / 1000 - 1] {
        let mut source = fixture(false);
        record_at(&mut source, RecordSite::ClaimNested).value =
            stats_value(ComponentValue::I64(whole), 5);
        assert_refused_unchanged(&source, UpgradeError::HealthOverflow(whole));
    }
}

#[test]
fn absent_owner_uses_the_project_basis_and_unaffected_documents_are_exact_noops() {
    let mut source = fixture(false);
    let mut expected = fixture(true);
    record_at(&mut source, RecordSite::Direct).owner = None;
    record_at(&mut expected, RecordSite::Direct).owner = None;
    assert_eq!(plan_scene(&source).unwrap().into_document(), expected);

    for source in [
        SceneDocument::default(),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("unavailable"))
            .with_component(component(), unknown_record())
            .with_component(ComponentTypeId::new(UNKNOWN), unknown_record())]),
    ] {
        assert_eq!(plan_scene(&source).unwrap().into_document(), source);
        let source = prefab(&source);
        assert_eq!(plan_prefab(&source).unwrap().into_document(), source);
    }
}

#[test]
fn catalog_refusal_leaves_the_actual_predecessor_unchanged() {
    let mut wrong_definition = v1_catalog();
    wrong_definition.components[0].fields[0].path = ComponentFieldPath::from_fields(["renamed"]);
    let mut bad_initial_value = v1_catalog();
    bad_initial_value.components[0].authoring_initial_value =
        Some(stats_value(ComponentValue::I64(i64::MAX), 10));
    let mut exhausted = v1_catalog();
    exhausted.generation = u64::MAX;
    let already_v2 = successor_catalog(&v1_catalog()).unwrap();
    for (source, error) in [
        (wrong_definition, UpgradeError::CatalogDefinition),
        (bad_initial_value, UpgradeError::HealthOverflow(i64::MAX)),
        (exhausted, UpgradeError::CatalogGeneration),
        (already_v2, UpgradeError::CatalogDefinition),
        (
            ComponentSchemaCatalog::default(),
            UpgradeError::CatalogDefinition,
        ),
    ] {
        let before = source.clone();
        assert_eq!(successor_catalog(&source).unwrap_err(), error);
        assert_eq!(source, before);
    }
}
