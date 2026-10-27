//! Code-authored data and native records share the public production Prefab compiler.

#[path = "support/prefab_source.rs"]
mod prefab_source;

use std::{collections::BTreeSet, convert::Infallible};

use nara_asset::AssetRef;
use nara_core::ItemLimit;
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentPropertyPath, ComponentRegistry, ComponentRegistrySnapshot, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaOwnerSnapshot,
    ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    InMemoryPrefabSourceResolver, PrefabComponentIntent, PrefabComponentSetMode, PrefabDocument,
    PrefabEntityOrigin, PrefabEntityTarget, PrefabExpansionReport, PrefabInstance,
    PrefabInstanceClaim, PrefabInstanceHop, PrefabOverlay, PrefabProjectionStep,
    PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceBinding, PrefabSourceResolver,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindingError,
    SceneComponentBindingLimits, SceneComponentBindings, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityRecord, SourceDocumentKey,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}

fn owner_id(value: &str) -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new(value).unwrap()
}

fn source(value: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/component-bindings/{value}.ron")).unwrap()
}

fn key(value: &str) -> SourceDocumentKey {
    prefab_source::key(&format!("component-bindings.{value}"))
}

fn empty() -> ComponentValue {
    ComponentValue::map([] as [(&str, ComponentValue); 0])
}

fn local(value: &str) -> ComponentValue {
    ComponentValue::EntityReference(EntityReference::SceneLocal { entity: id(value) })
}

fn mover_schema() -> ComponentSchema {
    ComponentSchema::new(
        component("game.Mover"),
        "Mover",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
            local("socket"),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("speed"),
            "Speed",
            ComponentFieldPath::from_fields(["speed"]),
            ComponentValueKind::U64,
            ComponentValue::U64(7),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn sensor_schema() -> ComponentSchema {
    ComponentSchema::new(
        component("game.Sensor"),
        "Sensor",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("target"),
            "Target",
            ComponentFieldPath::from_fields(["target"]),
            ComponentValueKind::EntityRef,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::EntityRef),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("enabled"),
            "Enabled",
            ComponentFieldPath::from_fields(["enabled"]),
            ComponentValueKind::Bool,
            ComponentValue::Bool(true),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn catalog(
    schemas: impl IntoIterator<Item = ComponentSchema>,
    tombstones: &[&str],
) -> ComponentSchemaCatalog {
    ComponentSchemaCatalog {
        components: schemas.into_iter().collect(),
        type_tombstones: tombstones.iter().map(|value| component(value)).collect(),
        ..Default::default()
    }
}

fn admit(
    owner: &str,
    current: ComponentSchemaCatalog,
    predecessor: Option<ComponentSchemaCatalog>,
) -> ComponentSchemaOwnerSnapshot {
    ComponentSchemaOwnerSnapshot::admit(owner_id(owner), current, predecessor).unwrap()
}

fn data_owners() -> [ComponentSchemaOwnerSnapshot; 2] {
    [
        admit("game.motion", catalog([mover_schema()], &[]), None),
        admit("game.sensing", catalog([sensor_schema()], &[]), None),
    ]
}

fn native_registry() -> ComponentRegistry {
    let mut native = ComponentRegistry::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut native)
        .unwrap();
    native.freeze().unwrap();
    native
}

fn empty_native() -> ComponentRegistrySnapshot {
    let mut native = ComponentRegistry::new();
    native.freeze().unwrap();
    native.snapshot().unwrap()
}

fn bindings() -> SceneComponentBindings {
    SceneComponentBindings::new(native_registry().snapshot().unwrap(), data_owners()).unwrap()
}

fn mover(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
        .with_owner(owner_id("game.motion"))
}

fn data_node(name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(id(name))
        .with_component(component("game.Mover"), mover(empty()))
        .with_component(
            component("game.Sensor"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([("target", local("socket"))]),
            )
            .with_owner(owner_id("game.sensing")),
        )
        .with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(format!("native {name}")),
            ),
        )
}

fn anchor(name: &str, prefab: &str, overrides: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    entity.prefab = Some(PrefabInstance {
        source: source(prefab),
        overrides,
    });
    entity
}

fn leaf_prefab() -> PrefabDocument {
    PrefabDocument::from_ordered_roots([data_node("node"), SceneEntityRecord::new(id("socket"))])
}

fn outer_prefab() -> PrefabDocument {
    PrefabDocument::new(
        [id("node")],
        [
            data_node("node").with_children([id("socket"), id("nested")]),
            SceneEntityRecord::new(id("socket")),
            anchor("nested", "leaf", None),
        ],
    )
}

fn sources() -> InMemoryPrefabSourceResolver {
    InMemoryPrefabSourceResolver::new()
        .with_prefab(source("outer"), key("outer"), outer_prefab())
        .with_prefab(source("leaf"), key("leaf"), leaf_prefab())
}

fn mixed_scene() -> SceneDocument {
    let mut overlay = PrefabOverlay::new(key("outer"));
    overlay.components.push(PrefabComponentIntent::Properties {
        target: PrefabEntityTarget::source(id("node")),
        component: component("game.Mover"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![
            PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("speed")),
                intent: PrefabPropertyIntent::Set(ComponentValue::U64(42)),
                bindings: Vec::new(),
            },
            PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
                intent: PrefabPropertyIntent::Set(local("socket")),
                bindings: vec![PrefabReferenceBinding {
                    slot: ComponentFieldPath::empty(),
                    expected: id("socket"),
                    target: PrefabEntityTarget {
                        through: vec![PrefabInstanceHop {
                            anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
                            expected_base: key("leaf"),
                        }],
                        entity: PrefabEntityOrigin::SourceEntity(id("socket")),
                    },
                }],
            },
        ],
    });
    SceneDocument::from_ordered_roots([
        anchor("left", "outer", Some(overlay)),
        anchor("right", "outer", None),
        SceneEntityRecord::new(id("socket")),
    ])
}

fn expanded(report: &PrefabExpansionReport) -> &SceneDocument {
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report
        .document()
        .expect("expected a complete production candidate")
}

fn rejected(report: &PrefabExpansionReport) {
    assert!(report.diagnostics.has_errors(), "{report:?}");
    assert!(
        report.candidate.is_none(),
        "no partial candidate may escape"
    );
    assert!(report.document().is_none());
    assert!(report.provenance().is_none());
}

fn projected(report: &PrefabExpansionReport, root: &str, steps: &[(&str, &str)]) -> SceneEntityId {
    let steps: Vec<_> = steps
        .iter()
        .map(|(prefab, entity)| PrefabProjectionStep::SourceEntity {
            source_document: source(prefab),
            canonical_source: key(prefab),
            entity: id(entity),
        })
        .collect();
    let mut matches = report.provenance().unwrap().iter().filter(|(_, origin)| {
        origin
            .projection_path()
            .is_some_and(|path| path.root_anchor() == &id(root) && path.steps() == steps.as_slice())
    });
    let (entity, origin) = matches.next().expect("expected exact source provenance");
    assert!(matches.next().is_none(), "source projection must be unique");
    assert_eq!(origin.local_entity_id(), steps.last().unwrap().entity_id());
    entity.clone()
}

fn entity<'a>(document: &'a SceneDocument, entity: &SceneEntityId) -> &'a SceneEntityRecord {
    document
        .entities
        .iter()
        .find(|record| &record.id == entity)
        .unwrap()
}

#[test]
fn selection_retains_exact_native_and_data_owner_snapshots_without_native_data_codecs() {
    let native = native_registry().snapshot().unwrap();
    let owners = data_owners();
    let bindings = SceneComponentBindings::new(native.clone(), owners.clone()).unwrap();
    assert!(bindings.native_snapshot().ptr_eq(&native));
    assert_eq!(bindings.data_owners().len(), 2);
    for owner in &owners {
        let schema = &owner.current().components()[0];
        assert!(
            bindings
                .data_owners()
                .iter()
                .any(|selected| selected.ptr_eq(owner))
        );
        assert!(bindings.data_owner(schema.id()).unwrap().ptr_eq(owner));
        assert!(std::ptr::eq(bindings.schema(schema.id()).unwrap(), schema));
        assert!(native.schema(schema.id()).is_none());
    }
    let name = component("nara.scene.Name");
    assert!(std::ptr::eq(
        bindings.schema(&name).unwrap(),
        native.schema(&name).unwrap()
    ));
    assert!(bindings.data_owner(&name).is_none());
    assert!(bindings.schema(&component("game.Unknown")).is_none());
    assert!(bindings.data_owner(&component("game.Unknown")).is_none());
}

#[test]
fn empty_frozen_native_registry_allows_code_authored_data_and_defaults() {
    let bindings = SceneComponentBindings::new(empty_native(), data_owners()).unwrap();
    let mut node = data_node("node");
    node.components.remove(&component("nara.scene.Name"));
    let document = SceneDocument::from_ordered_roots([node, SceneEntityRecord::new(id("socket"))]);
    let before = document.clone();
    let report = bindings.expand_prefabs(&document, &InMemoryPrefabSourceResolver::new());
    let node = entity(expanded(&report), &id("node"));
    assert_eq!(
        node.components[&component("game.Mover")].value,
        ComponentValue::map([
            ("target", local("socket")),
            ("speed", ComponentValue::U64(7))
        ])
    );
    assert_eq!(
        node.components[&component("game.Sensor")].value,
        ComponentValue::map([
            ("target", local("socket")),
            ("enabled", ComponentValue::Bool(true))
        ])
    );
    assert!(!report.source_upgrade_required());
    assert_eq!(document, before);
}

#[test]
fn repeated_nested_prefabs_share_compilation_but_not_source_reference_namespaces() {
    let bindings = bindings();
    let document = mixed_scene();
    let sources = sources();
    let before_document = document.clone();
    let before_sources = sources.clone();
    let report = bindings.expand_prefabs(&document, &sources);
    let expanded = expanded(&report);
    assert_eq!(expanded.entities.len(), 13);
    assert_eq!(expanded.roots, [id("left"), id("right"), id("socket")]);
    assert_eq!(report.provenance().unwrap().len(), expanded.entities.len());
    let mut distinct_sockets = BTreeSet::from([id("socket")]);
    for root in ["left", "right"] {
        let outer_node = projected(&report, root, &[("outer", "node")]);
        let outer_socket = projected(&report, root, &[("outer", "socket")]);
        let nested = projected(&report, root, &[("outer", "nested")]);
        let leaf_node = projected(&report, root, &[("outer", "nested"), ("leaf", "node")]);
        let leaf_socket = projected(&report, root, &[("outer", "nested"), ("leaf", "socket")]);
        assert_eq!(
            entity(expanded, &id(root)).children.as_slice(),
            std::slice::from_ref(&outer_node)
        );
        assert_eq!(
            entity(expanded, &outer_node).children,
            [outer_socket.clone(), nested.clone()]
        );
        assert_eq!(
            entity(expanded, &nested).children,
            [leaf_node.clone(), leaf_socket.clone()]
        );
        for (node, socket, speed, target) in [
            (
                &outer_node,
                &outer_socket,
                if root == "left" { 42 } else { 7 },
                if root == "left" {
                    &leaf_socket
                } else {
                    &outer_socket
                },
            ),
            (&leaf_node, &leaf_socket, 7, &leaf_socket),
        ] {
            let records = &entity(expanded, node).components;
            assert_eq!(
                records[&component("game.Mover")],
                mover(ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: target.clone()
                        })
                    ),
                    ("speed", ComponentValue::U64(speed)),
                ]))
            );
            let sensor = &records[&component("game.Sensor")];
            assert_eq!(sensor.owner(), Some(&owner_id("game.sensing")));
            assert_eq!(sensor.version, ComponentSchemaVersion::ONE);
            assert_eq!(
                sensor.value,
                ComponentValue::map([
                    (
                        "target",
                        ComponentValue::EntityReference(EntityReference::SceneLocal {
                            entity: socket.clone()
                        })
                    ),
                    ("enabled", ComponentValue::Bool(true)),
                ])
            );
            assert_eq!(
                records[&component("nara.scene.Name")].value,
                ComponentValue::String("native node".into())
            );
            assert!(records[&component("nara.scene.Name")].owner().is_none());
        }
        assert!(distinct_sockets.insert(outer_socket));
        assert!(distinct_sockets.insert(leaf_socket));
    }
    assert_eq!(distinct_sockets.len(), 5);
    assert!(
        expanded
            .entities
            .iter()
            .all(|entity| entity.prefab.is_none())
    );
    assert!(
        report.provenance().unwrap()[&id("socket")]
            .projection_path()
            .is_none()
    );
    assert!(!report.source_upgrade_required());
    assert_eq!(document, before_document);
    assert_eq!(sources, before_sources);
}

#[test]
fn public_loader_returns_the_same_complete_mixed_candidate_as_the_resolver() {
    let bindings = bindings();
    let document = mixed_scene();
    let sources = sources();
    let expected = bindings.expand_prefabs(&document, &sources);
    expanded(&expected);
    let mut requests = Vec::new();
    let actual = bindings
        .try_expand_prefabs_with_loader(&document, Default::default(), None, |request, _| {
            requests.push(request.clone());
            Ok::<_, Infallible>(sources.resolve_prefab(request))
        })
        .unwrap();
    assert_eq!(actual, expected);
    assert!(requests.contains(&source("outer")));
    assert!(requests.contains(&source("leaf")));
    assert!(
        requests
            .iter()
            .all(|request| request == &source("outer") || request == &source("leaf"))
    );
}

#[derive(Debug, PartialEq, Eq)]
enum LoadError {
    Refused(AssetRef),
}

#[test]
fn mixed_loader_acquires_only_selected_nested_sources_and_preserves_its_error_type() {
    let bindings = bindings();
    let mut overlay = PrefabOverlay::new(key("outer"));
    overlay.instances.push(PrefabInstanceClaim {
        target: PrefabEntityTarget::source(id("nested")),
        value: Some(PrefabInstance {
            source: source("replacement"),
            overrides: None,
        }),
    });
    let document = SceneDocument::from_ordered_roots([anchor("instance", "outer", Some(overlay))]);
    // The inherited leaf is deliberately unavailable; only its selected replacement exists.
    let sources = InMemoryPrefabSourceResolver::new()
        .with_prefab(source("outer"), key("outer"), outer_prefab())
        .with_prefab(source("replacement"), key("replacement"), leaf_prefab());
    let before_document = document.clone();
    let before_sources = sources.clone();
    let mut requests = Vec::new();
    let report = bindings
        .try_expand_prefabs_with_loader(&document, Default::default(), None, |request, _| {
            requests.push(request.clone());
            Ok::<_, LoadError>(sources.resolve_prefab(request))
        })
        .unwrap();
    assert_eq!(expanded(&report).entities.len(), 6);
    let node = projected(
        &report,
        "instance",
        &[("outer", "nested"), ("replacement", "node")],
    );
    let socket = projected(
        &report,
        "instance",
        &[("outer", "nested"), ("replacement", "socket")],
    );
    assert_eq!(
        entity(expanded(&report), &node).components[&component("game.Mover")]
            .value
            .field_entity_reference("target")
            .unwrap(),
        &EntityReference::SceneLocal { entity: socket }
    );
    assert_eq!(requests, [source("outer"), source("replacement")]);

    requests.clear();
    let error = bindings
        .try_expand_prefabs_with_loader(&document, Default::default(), None, |request, _| {
            requests.push(request.clone());
            if request == &source("replacement") {
                Err(LoadError::Refused(request.clone()))
            } else {
                Ok(sources.resolve_prefab(request))
            }
        })
        .unwrap_err();
    assert_eq!(error, LoadError::Refused(source("replacement")));
    assert_eq!(requests, [source("outer"), source("replacement")]);
    assert_eq!(document, before_document);
    assert_eq!(sources, before_sources);
}

fn invalid_records() -> [(ComponentTypeId, SceneComponentRecord); 4] {
    let mut future = mover(empty());
    future.version = ComponentSchemaVersion::new(2).unwrap();
    [
        (
            component("game.Mover"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, empty()),
        ),
        (
            component("game.Mover"),
            mover(empty()).with_owner(owner_id("game.sensing")),
        ),
        (component("game.Mover"), future),
        (component("game.Unknown"), mover(empty())),
    ]
}

#[test]
fn invalid_local_and_prefab_records_refuse_without_publishing_valid_siblings() {
    let bindings = bindings();
    for (component, record) in invalid_records() {
        let records = [
            data_node("a_valid"),
            SceneEntityRecord::new(id("socket")),
            SceneEntityRecord::new(id("z_invalid")).with_component(component, record),
        ];
        let local = SceneDocument::from_ordered_roots(records.clone());
        let before = local.clone();
        rejected(&bindings.expand_prefabs(&local, &InMemoryPrefabSourceResolver::new()));
        assert_eq!(local, before);

        let sources = InMemoryPrefabSourceResolver::new().with_prefab(
            source("invalid"),
            key("invalid"),
            PrefabDocument::from_ordered_roots(records),
        );
        let before_sources = sources.clone();
        let document = SceneDocument::from_ordered_roots([anchor("instance", "invalid", None)]);
        rejected(&bindings.expand_prefabs(&document, &sources));
        assert_eq!(sources, before_sources);
    }
}

#[test]
fn invalid_overlay_sets_do_not_degrade_into_a_native_preview_candidate() {
    let bindings = bindings();
    let sources = sources();
    for (component, record) in invalid_records() {
        let known = component.as_str() == "game.Mover";
        let mut overlay = PrefabOverlay::new(key("outer"));
        overlay.components.push(PrefabComponentIntent::Set {
            mode: if known {
                PrefabComponentSetMode::Replace
            } else {
                PrefabComponentSetMode::Add
            },
            target: PrefabEntityTarget::source(id("node")),
            component,
            record,
            bindings: if known {
                vec![PrefabReferenceBinding {
                    slot: ComponentFieldPath::from_fields(["target"]),
                    expected: id("socket"),
                    target: PrefabEntityTarget::source(id("socket")),
                }]
            } else {
                Vec::new()
            },
        });
        let document =
            SceneDocument::from_ordered_roots([anchor("instance", "outer", Some(overlay))]);
        let before = document.clone();
        rejected(&bindings.expand_prefabs(&document, &sources));
        assert_eq!(document, before);
    }
}

#[test]
fn predecessor_data_versions_require_explicit_migration_instead_of_native_convenience() {
    let predecessor = catalog([mover_schema()], &[]);
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
    let mut schema = mover_schema();
    schema.version = ComponentSchemaVersion::new(2).unwrap();
    current.components.push(schema);
    let bindings = SceneComponentBindings::new(
        empty_native(),
        [admit("game.motion", current, Some(predecessor))],
    )
    .unwrap();
    for version in [1, 2, 3] {
        let mut record = mover(empty());
        record.version = ComponentSchemaVersion::new(version).unwrap();
        let document = SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("node")).with_component(component("game.Mover"), record),
            SceneEntityRecord::new(id("socket")),
        ]);
        let report = bindings.expand_prefabs(&document, &InMemoryPrefabSourceResolver::new());
        if version == 2 {
            assert_eq!(
                entity(expanded(&report), &id("node")).components[&component("game.Mover")].version,
                ComponentSchemaVersion::new(2).unwrap()
            );
            assert!(!report.source_upgrade_required());
        } else {
            rejected(&report);
        }
    }
}

#[test]
fn native_only_entry_point_rejects_data_instead_of_implicitly_selecting_storage() {
    let native = native_registry();
    let document = mixed_scene();
    let sources = sources();
    expanded(&bindings().expand_prefabs(&document, &sources));
    rejected(&document.expand_prefabs(&native, &sources));
}

#[test]
fn mixed_entry_point_retains_ownerless_native_name_convenience() {
    let native = native_registry();
    let bindings = SceneComponentBindings::new(native.snapshot().unwrap(), data_owners()).unwrap();
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("named"))
        .with_component(
            component("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Native only".into()),
            ),
        )]);
    let sources = InMemoryPrefabSourceResolver::new();
    let expected = document.expand_prefabs(&native, &sources);
    expanded(&expected);
    assert_eq!(bindings.expand_prefabs(&document, &sources), expected);
}

#[test]
fn duplicate_data_owner_rejects_both_shared_and_equivalent_admissions() {
    let first = data_owners()[0].clone();
    let equivalent = admit("game.motion", first.current().clone(), None);
    assert!(!first.ptr_eq(&equivalent));
    let disjoint = admit("game.motion", catalog([sensor_schema()], &[]), None);
    for second in [first.clone(), equivalent, disjoint] {
        assert!(
            matches!(SceneComponentBindings::new(empty_native(), [first.clone(), second]),
            Err(SceneComponentBindingError::DuplicateOwner { owner }) if owner == owner_id("game.motion"))
        );
    }
}

#[test]
fn native_and_data_cannot_share_an_owner_even_when_their_component_ids_are_disjoint() {
    let owner = SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str();
    let data = admit(owner, catalog([mover_schema()], &[]), None);
    assert!(
        matches!(SceneComponentBindings::new(native_registry().snapshot().unwrap(), [data]),
        Err(SceneComponentBindingError::DuplicateOwner { owner: rejected }) if rejected == owner_id(owner))
    );

    for native_catalog in [
        ComponentSchemaCatalog::default(),
        catalog([], &["game.RetiredNative"]),
    ] {
        let mut native = ComponentRegistry::from_owner_catalog_candidate(
            ComponentSchemaOwnerId::new("game.raw-native"),
            native_catalog,
            None,
        )
        .unwrap();
        native.freeze().unwrap();
        let snapshot = native.snapshot().unwrap();
        assert_eq!(
            snapshot.schema_owners().collect::<Vec<_>>(),
            [ComponentSchemaOwnerId::new("game.raw-native")]
        );
        let data = admit("game.raw-native", catalog([mover_schema()], &[]), None);
        assert!(matches!(SceneComponentBindings::new(snapshot, [data]),
            Err(SceneComponentBindingError::DuplicateOwner { owner }) if owner == owner_id("game.raw-native")));
    }
}

#[test]
fn cross_owner_active_and_tombstone_claims_cannot_select_the_same_component() {
    let active = catalog([mover_schema()], &[]);
    let retired = catalog([], &["game.Mover"]);
    for (left, right) in [
        (active.clone(), active.clone()),
        (active.clone(), retired.clone()),
        (retired.clone(), active),
        (retired.clone(), retired),
    ] {
        let owners = [
            admit("game.first", left, None),
            admit("game.second", right, None),
        ];
        for owners in [owners.clone(), [owners[1].clone(), owners[0].clone()]] {
            assert!(
                matches!(SceneComponentBindings::new(empty_native(), owners),
                Err(SceneComponentBindingError::DivergentComponentOwnership { component: claimed })
                    if claimed == component("game.Mover"))
            );
        }
    }
}

#[test]
fn retired_lineage_stays_reserved_but_does_not_expose_a_current_schema_or_data_owner() {
    let predecessor = catalog([mover_schema()], &[]);
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
    current.type_tombstones.push(component("game.Mover"));
    let retired = admit("game.motion", current, Some(predecessor));
    let bindings = SceneComponentBindings::new(empty_native(), [retired.clone()]).unwrap();
    assert!(bindings.data_owners()[0].ptr_eq(&retired));
    assert!(bindings.schema(&component("game.Mover")).is_none());
    assert!(bindings.data_owner(&component("game.Mover")).is_none());
    let replacement = admit("game.foreign", catalog([mover_schema()], &[]), None);
    assert!(
        matches!(SceneComponentBindings::new(empty_native(), [retired, replacement]),
        Err(SceneComponentBindingError::DivergentComponentOwnership { component: claimed })
            if claimed == component("game.Mover"))
    );
}

#[test]
fn native_active_and_retired_ids_reject_data_claims_from_another_owner() {
    let native = native_registry().snapshot().unwrap();
    let native_name = native
        .schema(&component("nara.scene.Name"))
        .unwrap()
        .clone();
    for data_catalog in [
        catalog([native_name], &[]),
        catalog([], &["nara.scene.Name"]),
    ] {
        let data = admit("game.foreign", data_catalog, None);
        assert!(
            matches!(SceneComponentBindings::new(native.clone(), [data]),
            Err(SceneComponentBindingError::DivergentComponentOwnership { component: claimed })
                if claimed == component("nara.scene.Name"))
        );
    }
    let mut native = ComponentRegistry::from_owner_catalog_candidate(
        ComponentSchemaOwnerId::new("game.native-retired"),
        catalog([], &["game.Mover"]),
        None,
    )
    .unwrap();
    native.freeze().unwrap();
    for data_catalog in [catalog([mover_schema()], &[]), catalog([], &["game.Mover"])] {
        let data = admit("game.foreign", data_catalog, None);
        assert!(
            matches!(SceneComponentBindings::new(native.snapshot().unwrap(), [data]),
            Err(SceneComponentBindingError::DivergentComponentOwnership { component: claimed })
                if claimed == component("game.Mover"))
        );
    }
}

#[test]
fn aggregate_owner_and_claim_limits_accept_exact_caps_and_count_lineage_and_tombstones() {
    let predecessor = catalog([mover_schema()], &["game.Retired"]);
    let mut current = ComponentSchemaCatalog::successor_of(&predecessor).unwrap();
    current.components.push(mover_schema());
    let owners = [
        admit("game.motion", current, Some(predecessor)),
        data_owners()[1].clone(),
    ];
    // Motion has four declarations across its two catalogs; sensing contributes the fifth.
    let exact = SceneComponentBindingLimits::default()
        .with_owners(ItemLimit::new(2).unwrap())
        .with_claims(ItemLimit::new(5).unwrap());
    let bindings =
        SceneComponentBindings::new_with_limits(empty_native(), owners.clone(), exact).unwrap();
    assert_eq!(bindings.data_owners().len(), 2);
    assert!(bindings.schema(&component("game.Mover")).is_some());
    assert!(bindings.schema(&component("game.Sensor")).is_some());
    assert!(bindings.schema(&component("game.Retired")).is_none());
    assert!(matches!(
        SceneComponentBindings::new_with_limits(
            empty_native(),
            owners.clone(),
            exact.with_owners(ItemLimit::ONE)
        ),
        Err(SceneComponentBindingError::OwnerLimitExceeded {
            observed: 2,
            maximum: 1
        })
    ));
    assert!(matches!(
        SceneComponentBindings::new_with_limits(
            empty_native(),
            owners.clone(),
            exact.with_claims(ItemLimit::new(4).unwrap())
        ),
        Err(SceneComponentBindingError::ClaimLimitExceeded {
            observed: 5,
            maximum: 4
        })
    ));
    assert!(matches!(
        SceneComponentBindings::new_with_limits(
            empty_native(),
            [owners[0].clone()],
            exact.with_claims(ItemLimit::new(3).unwrap())
        ),
        Err(SceneComponentBindingError::ClaimLimitExceeded {
            observed: 4,
            maximum: 3
        })
    ));
}
