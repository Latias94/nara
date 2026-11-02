use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use nara_asset::{
    AssetProductRef, AssetRecord, AssetRef, AssetSourceKind, AssetSourcePath, ImportedProductId,
    ProjectAssetDatabase, StableAssetId,
};
use nara_core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistryBuilder, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, EntityReference,
    PersistentComponentSchemaOwnerId, encode_asset_reference_value,
};

use super::*;
use crate::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, PrefabProjectionPath, PrefabProjectionStep,
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityPlacement,
    SceneEntityProvenance, SceneEntityRecord, ScenePlacement, ScenePlacementParent,
};

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component(value: &str) -> ComponentTypeId {
    ComponentTypeId::new(value)
}
fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("game.runtime-data").unwrap()
}

fn key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn containing() -> SourceDocumentKey {
    key("scenes.runtime", SourceDocumentKind::Scene)
}
fn prefab_key(name: &str) -> SourceDocumentKey {
    key(name, SourceDocumentKind::Prefab)
}
fn asset_id() -> StableAssetId {
    "00000000-0000-4000-8000-000000000041".parse().unwrap()
}

fn database() -> ProjectAssetDatabase {
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            asset_id(),
            AssetSourcePath::new("images/shared.png").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    database
}

fn primary() -> AssetRef {
    AssetRef::Product(AssetProductRef::new(asset_id(), ImportedProductId::PRIMARY).unwrap())
}

fn root_schema(name: &str, shape: ComponentValueShape) -> ComponentSchema {
    let reference = if shape.contains_kind(ComponentValueKind::AssetRef) {
        Some(ComponentCapability::AssetRef)
    } else if shape.contains_kind(ComponentValueKind::EntityRef) {
        Some(ComponentCapability::EntityRef)
    } else {
        None
    };
    let mut field = ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::empty(),
        shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING);
    if let Some(capability) = reference {
        field = field.with_capability(capability);
    }
    ComponentSchema::new(component(name), name, ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([field])
}

fn bindings() -> SceneComponentBindings {
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let asset_shape = ComponentValueShape::record_members([ComponentShapeMember::new(
        "items",
        ComponentValueShape::list(ComponentValueShape::map(ComponentValueShape::optional(
            ComponentValueShape::from(ComponentValueKind::AssetRef),
        ))),
    )
    .with_id(ComponentFieldId::new("items"))]);
    let assets = ComponentSchema::new(
        component("game.Assets"),
        "Assets",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("assets"),
        "Assets",
        ComponentFieldPath::empty(),
        asset_shape,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let defaults = ComponentSchema::new(
        component("game.Defaults"),
        "Defaults",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("amount"),
        "Amount",
        ComponentFieldPath::from_fields(["amount"]),
        ComponentValueKind::U64,
        ComponentValue::U64(7),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
    let asset_default = ComponentSchema::new(
        component("game.AssetDefault"),
        "AssetDefault",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::from_fields(["image"]),
        ComponentValueShape::from(ComponentValueKind::AssetRef),
        encode_asset_reference_value(&AssetRef::path("images/shared.png").unwrap()),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner_id(),
        ComponentSchemaCatalog {
            components: vec![
                assets,
                defaults,
                asset_default,
                root_schema("game.Link", ComponentValueShape::EntityRef),
                root_schema(
                    "game.Links",
                    ComponentValueShape::list(ComponentValueShape::map(
                        ComponentValueShape::optional(ComponentValueShape::EntityRef),
                    )),
                ),
                root_schema(
                    "game.Image",
                    ComponentValueShape::from(ComponentValueKind::AssetRef),
                ),
                root_schema(
                    "game.Text",
                    ComponentValueShape::scalar(ComponentValueKind::String),
                ),
                root_schema(
                    "game.Number",
                    ComponentValueShape::scalar(ComponentValueKind::U64),
                ),
            ],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SceneComponentBindings::new(registry.clone(), [owner]).unwrap()
}

fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner_id())
}

fn compile(bindings: &SceneComponentBindings, document: SceneDocument) -> CompiledSceneCandidate {
    let report = bindings.compile_scene(&document, &InMemoryPrefabSourceResolver::new());
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(containing())
}

fn copy_claims(candidate: &CompiledSceneCandidate) -> FlatSceneClaims {
    cook::clone_claims(
        candidate,
        &mut budget::WorkBudget::new(RuntimeSceneLimits::default()),
    )
    .unwrap()
}

fn seal_claims(
    claims: FlatSceneClaims,
    bindings: &SceneComponentBindings,
) -> CompiledSceneCandidate {
    crate::compiled::admit_claims(
        claims,
        SchemaAuthority::Mixed(bindings),
        RuntimeSceneLimits::default().value_limits(),
        None,
        false,
        |_, _, _| Ok::<_, RuntimeSceneError>(()),
    )
    .unwrap()
    .bind_source(containing())
}

fn assert_payload_eq(left: &CompiledSceneDocument, right: &CompiledSceneDocument) {
    assert_eq!(left.roots(), right.roots());
    assert_eq!(left.entities(), right.entities());
}

fn fixture(bindings: &SceneComponentBindings, alias: &str) -> CompiledSceneCandidate {
    let nested_request = AssetRef::path(format!("prefabs/{alias}/nested.ron")).unwrap();
    let outer_request = AssetRef::path(format!("prefabs/{alias}/outer.ron")).unwrap();
    let mut nested = SceneEntityRecord::new(id("nested"));
    nested.prefab = Some(PrefabInstance {
        binding: crate::PrefabInstanceBinding::Forest,
        source: nested_request.clone(),
        arguments: None,
        overrides: None,
    });
    let outer = PrefabDocument::new(
        [
            SceneEntityPlacement::root(id("source")),
            SceneEntityPlacement::root(id("nested")),
        ],
        [
            SceneEntityRecord::new(id("source")).with_component(
                component("game.Link"),
                record(ComponentValue::EntityReference(
                    EntityReference::SceneLocal {
                        entity: id("nested"),
                    },
                )),
            ),
            nested,
        ],
    );
    let mut addition = PrefabAddition::new(id("added"));
    addition.prefab = Some(PrefabInstance {
        binding: crate::PrefabInstanceBinding::Forest,
        source: nested_request.clone(),
        arguments: None,
        overrides: None,
    });
    let mut overlay = PrefabOverlay::new(prefab_key("canonical.outer"));
    overlay.additions.push(addition);
    overlay.placements.push(SceneEntityPlacement {
        target: PrefabEntityTarget::addition(id("added")),
        placement: ScenePlacement::last(ScenePlacementParent::Root),
    });
    let resolver = InMemoryPrefabSourceResolver::new()
        .with_prefab(outer_request.clone(), prefab_key("canonical.outer"), outer)
        .with_prefab(
            nested_request,
            prefab_key("canonical.nested"),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
        );
    let mut anchor = SceneEntityRecord::new(id("anchor")).with_component(
        component("game.Text"),
        record(ComponentValue::String("anchor".into())),
    );
    anchor.prefab = Some(PrefabInstance {
        binding: crate::PrefabInstanceBinding::Forest,
        source: outer_request,
        arguments: None,
        overrides: Some(overlay),
    });
    let report = bindings.compile_scene(
        &SceneDocument::from_ordered_roots([anchor, SceneEntityRecord::new(id("local"))]),
        &resolver,
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    report.candidate.unwrap().bind_source(containing())
}

fn cooked_json(bindings: &SceneComponentBindings) -> serde_json::Value {
    let candidate = RuntimeSceneCandidate::cook(
        &fixture(bindings, "first"),
        bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    serde_json::from_slice(&candidate.to_json_bytes().unwrap()).unwrap()
}

fn admit_json(
    value: &serde_json::Value,
    bindings: &SceneComponentBindings,
) -> Result<RuntimeSceneCandidate, RuntimeSceneError> {
    RuntimeSceneDocumentCandidate::from_json_bytes(
        &serde_json::to_vec(value).unwrap(),
        RuntimeSceneLimits::default(),
    )?
    .admit(&containing(), bindings)
}

#[test]
fn linked_nested_and_addition_cook_is_deterministic_and_round_trips_complete_provenance() {
    let bindings = bindings();
    let expanded = fixture(&bindings, "first");
    let before = format!("{:?}", expanded.document());
    let origins_before = expanded.canonical_provenance().clone();
    let first = RuntimeSceneCandidate::cook(
        &expanded,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let second = RuntimeSceneCandidate::cook(
        &expanded,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    assert_eq!(
        first.to_json_bytes().unwrap(),
        second.to_json_bytes().unwrap()
    );
    assert_eq!(first.source_key(), &containing());
    assert!(first.definitions().matches(&bindings));
    assert!(
        first
            .document()
            .entities()
            .iter()
            .any(|entity| entity.id() == &id("local"))
    );
    assert!(first.canonical_provenance().values().any(|origin| {
        origin.projection_path().is_some_and(|path| {
            matches!(
                path.steps(),
                [
                    PrefabProjectionStep::InstanceAddition { .. },
                    PrefabProjectionStep::SourceEntity { .. }
                ]
            )
        })
    }));
    let linked = first
        .document()
        .entities()
        .iter()
        .find_map(|entity| entity.components().get(&component("game.Link")))
        .unwrap();
    let EntityReference::SceneLocal { entity: target } =
        linked.value().as_entity_reference().unwrap()
    else {
        panic!("expected local reference")
    };
    assert!(
        first
            .document()
            .entities()
            .iter()
            .any(|entity| entity.id() == target)
    );
    let bytes = first.to_json_bytes().unwrap();
    let admitted = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, first.limits())
        .unwrap()
        .admit(&containing(), &bindings)
        .unwrap();
    assert_eq!(admitted.document(), first.document());
    assert_eq!(
        admitted.canonical_provenance(),
        first.canonical_provenance()
    );
    assert_eq!(admitted.to_json_bytes().unwrap(), bytes);
    assert_eq!(format!("{:?}", expanded.document()), before);
    assert_eq!(expanded.canonical_provenance(), &origins_before);
}

#[test]
fn lookup_alias_spelling_does_not_select_runtime_ids_but_evidence_is_retained() {
    let bindings = bindings();
    let first = RuntimeSceneCandidate::cook(
        &fixture(&bindings, "first"),
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let second = RuntimeSceneCandidate::cook(
        &fixture(&bindings, "renamed"),
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    assert_payload_eq(first.document(), second.document());
    assert_ne!(first.canonical_provenance(), second.canonical_provenance());
    for candidate in [&first, &second] {
        let bytes = candidate.to_json_bytes().unwrap();
        assert!(
            RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, candidate.limits())
                .unwrap()
                .admit(&containing(), &bindings)
                .is_ok()
        );
    }
}

#[test]
fn generated_expansion_ids_never_select_persisted_identity_or_reference_targets() {
    let bindings = bindings();
    let original = fixture(&bindings, "first");
    let first = RuntimeSceneCandidate::cook(
        &original,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let projected_ids = first
        .canonical_provenance()
        .iter()
        .filter(|(_, origin)| origin.projection_path().is_some())
        .map(|(entity, _)| entity.clone())
        .collect::<Vec<_>>();
    let mut renamed_projected = projected_ids.iter().cycle().skip(1);
    let renames = original
        .canonical_provenance()
        .iter()
        .map(|(entity, origin)| {
            (
                entity.clone(),
                if origin.projection_path().is_some() {
                    // An actual old ID may already spell another entity's final canonical ID.
                    // Remapping must always consult the unchanged complete old-ID map.
                    renamed_projected.next().unwrap().clone()
                } else {
                    entity.clone()
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut claims = copy_claims(&original);
    for root in &mut claims.roots {
        *root = renames[root].clone();
    }
    for entity in &mut claims.entities {
        entity.id = renames[&entity.id].clone();
        for child in &mut entity.children {
            *child = renames[child].clone();
        }
        for record in entity.components.values_mut() {
            if let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) =
                &mut record.value
            {
                *entity = renames[entity].clone();
            }
        }
    }
    claims.canonical_provenance = original
        .canonical_provenance()
        .iter()
        .map(|(entity, origin)| (renames[entity].clone(), origin.clone()))
        .collect();
    let renamed = seal_claims(claims, &bindings);
    let second = RuntimeSceneCandidate::cook(
        &renamed,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    assert_eq!(first.document(), second.document());
    assert_eq!(first.canonical_provenance(), second.canonical_provenance());
    assert_eq!(
        first.to_json_bytes().unwrap(),
        second.to_json_bytes().unwrap()
    );
}

#[test]
fn nested_entity_references_remap_through_the_declared_plan_and_keep_persistent_axes() {
    let bindings = bindings();
    let original = fixture(&bindings, "first");
    let (old_target, target_origin) = original
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| origin.projection_path().is_some())
        .unwrap();
    let persistent = EntityReference::Persistent {
        entity: nara_identity::PersistentRuntimeReference::new(
            nara_identity::PersistentRuntimeNamespaceId::new("game.saved").unwrap(),
            nara_identity::PersistentRuntimeId::parse_str("00000000-0000-4000-8000-000000000056")
                .unwrap(),
        ),
    };
    let mut claims = copy_claims(&original);
    let anchor = claims
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("anchor"))
        .unwrap();
    anchor.components.insert(
        component("game.Links"),
        crate::compiled::claims::FlatSceneComponentClaim {
            schema_owner: Some(owner_id()),
            version: ComponentSchemaVersion::ONE,
            value: ComponentValue::List(vec![ComponentValue::map([
                (
                    "local",
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: old_target.clone(),
                    }),
                ),
                (
                    "persistent",
                    ComponentValue::EntityReference(persistent.clone()),
                ),
                ("empty", ComponentValue::Null),
            ])]),
        },
    );
    anchor.components.insert(
        component("game.Text"),
        crate::compiled::claims::FlatSceneComponentClaim {
            schema_owner: Some(owner_id()),
            version: ComponentSchemaVersion::ONE,
            value: ComponentValue::String(old_target.as_str().to_owned()),
        },
    );
    let input = seal_claims(claims, &bindings);
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let target = cooked
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| *origin == target_origin)
        .unwrap()
        .0;
    let anchor = cooked
        .document()
        .entities()
        .iter()
        .find(|entity| entity.id() == &id("anchor"))
        .unwrap();
    let ComponentValue::List(items) = anchor.components()[&component("game.Links")].value() else {
        panic!("expected list")
    };
    assert_eq!(
        items[0].get("local").unwrap().as_entity_reference(),
        Some(&EntityReference::SceneLocal {
            entity: target.clone()
        })
    );
    assert_eq!(
        items[0].get("persistent").unwrap().as_entity_reference(),
        Some(&persistent)
    );
    assert_eq!(items[0].get("empty"), Some(&ComponentValue::Null));
    assert_eq!(
        anchor.components()[&component("game.Text")]
            .value()
            .as_str(),
        Some(old_target.as_str())
    );
    let bytes = cooked.to_json_bytes().unwrap();
    assert!(
        RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
            .unwrap()
            .admit(&containing(), &bindings)
            .is_ok()
    );
}

#[test]
fn complete_logical_identity_is_injective_and_refuses_preserved_local_collision() {
    let bindings = bindings();
    let original = fixture(&bindings, "first");
    let first = RuntimeSceneCandidate::cook(
        &original,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let collision = first
        .canonical_provenance()
        .iter()
        .find(|(_, origin)| origin.projection_path().is_some())
        .unwrap()
        .0
        .clone();
    let mut claims = copy_claims(&original);
    claims
        .entities
        .push(crate::compiled::claims::FlatSceneEntityClaim {
            id: collision.clone(),
            children: Vec::new(),
            components: BTreeMap::new(),
        });
    claims.roots.push(collision.clone());
    claims
        .canonical_provenance
        .insert(collision.clone(), SceneEntityProvenance::local(collision));
    let input = seal_claims(claims, &bindings);
    let before = format!("{input:?}");
    let error = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap_err();
    assert!(
        error
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.runtime-entity-id-collision")
    );
    assert_eq!(format!("{input:?}"), before);

    // A second final ID cannot claim an already admitted complete logical address.
    let mut invalid: serde_json::Value =
        serde_json::from_slice(&first.to_json_bytes().unwrap()).unwrap();
    let mut existing = invalid["payload"]["provenance"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["origin"]["kind"] == "projected")
        .unwrap()
        .clone();
    existing["entity"] = serde_json::json!("duplicate-logical");
    invalid["payload"]["provenance"]
        .as_array_mut()
        .unwrap()
        .push(existing);
    invalid["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "duplicate-logical", "children": [], "components": {}
        }));
    invalid["payload"]["roots"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("duplicate-logical"));
    assert!(admit_json(&invalid, &bindings).is_err());
}

#[test]
fn cook_preserves_explicit_compiled_defaults_and_rewrites_declared_assets_without_touching_strings()
{
    let bindings = bindings();
    let subproduct = AssetRef::Product(
        AssetProductRef::new(
            asset_id(),
            ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000042").unwrap(),
        )
        .unwrap(),
    );
    let original = [
        AssetRef::path("images/shared.png").unwrap(),
        AssetRef::StableId(asset_id()),
        primary(),
        subproduct.clone(),
    ];
    let items = original
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            ComponentValue::map([
                (
                    format!("entry{index}"),
                    encode_asset_reference_value(reference),
                ),
                ("empty".to_owned(), ComponentValue::Null),
            ])
        })
        .collect();
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local"))
            .with_component(
                component("game.Assets"),
                record(ComponentValue::map([(
                    "items",
                    ComponentValue::List(items),
                )])),
            )
            .with_component(
                component("game.AssetDefault"),
                record(ComponentValue::Map(BTreeMap::new())),
            )
            .with_component(
                component("game.Text"),
                record(ComponentValue::String("images/shared.png".into())),
            )]),
    );
    let before = format!("{:?}", input.document());
    assert!(
        input.document().entities()[0].components()[&component("game.AssetDefault")]
            .value()
            .get("image")
            .is_some()
    );
    let output = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let components = output.document().entities()[0].components();
    let schema = bindings.schema(&component("game.Assets")).unwrap();
    let references = nara_reflect::collect_declared_asset_references_from_schema(
        schema,
        ComponentSchemaVersion::ONE,
        components[&component("game.Assets")].value(),
    )
    .unwrap();
    assert_eq!(
        references
            .iter()
            .map(|reference| reference.asset_ref().clone())
            .collect::<Vec<_>>(),
        vec![primary(), primary(), primary(), subproduct]
    );
    assert_eq!(
        components[&component("game.AssetDefault")]
            .value()
            .get("image"),
        Some(&encode_asset_reference_value(&primary()))
    );
    assert_eq!(
        components[&component("game.Text")].value(),
        &ComponentValue::String("images/shared.png".into())
    );
    assert_eq!(format!("{:?}", input.document()), before);
    assert!(
        RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT),
            RuntimeSceneLimits::default()
        )
        .is_err()
    );
    assert_eq!(format!("{:?}", input.document()), before);
}

#[test]
fn canonical_primary_and_subproduct_cook_at_the_exact_value_node_limit() {
    let bindings = bindings();
    for (product, nodes) in [
        (ImportedProductId::PRIMARY, 6),
        (
            ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000042").unwrap(),
            7,
        ),
    ] {
        let reference = AssetRef::Product(AssetProductRef::new(asset_id(), product).unwrap());
        let input = compile(
            &bindings,
            SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local"))
                .with_component(
                    component("game.Image"),
                    record(encode_asset_reference_value(&reference)),
                )]),
        );
        let limits = RuntimeSceneLimits::default();
        let limits = limits.with_base(
            limits
                .base()
                .with_component_value_nodes(ItemLimit::new(nodes).unwrap()),
        );
        let cooked = RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            database().resolver(&nara_asset::AssetPathScope::ROOT),
            limits,
        )
        .unwrap();
        let bytes = cooked.to_json_bytes().unwrap();
        RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, limits)
            .unwrap()
            .admit(&containing(), &bindings)
            .unwrap();
        assert_eq!(cooked.document(), input.document());
        let too_small = limits.with_base(
            limits
                .base()
                .with_component_value_nodes(ItemLimit::new(nodes - 1).unwrap()),
        );
        assert!(
            RuntimeSceneCandidate::cook(
                &input,
                &bindings,
                database().resolver(&nara_asset::AssetPathScope::ROOT),
                too_small
            )
            .is_err()
        );
    }
    // Each leaf revisits its shared field/list prefix during path validation, decoding and
    // encoding. A graph-value node allowance is not the cumulative rewrite-work allowance.
    let value = ComponentValue::map([(
        "items",
        ComponentValue::List(
            (0..32)
                .map(|index| {
                    ComponentValue::map([(
                        format!("slot{index}"),
                        encode_asset_reference_value(&primary()),
                    )])
                })
                .collect(),
        ),
    )]);
    let nodes = value.cost().nodes();
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local"))
            .with_component(component("game.Assets"), record(value))]),
    );
    let limits = RuntimeSceneLimits::default();
    let limits = limits.with_base(
        limits
            .base()
            .with_component_value_nodes(ItemLimit::new(nodes).unwrap()),
    );
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        limits,
    )
    .unwrap();
    assert_eq!(cooked.document(), input.document());
}

#[test]
fn strict_wire_refuses_source_fields_even_null_or_empty_and_missing_or_duplicate_fields() {
    let bindings = bindings();
    let original = cooked_json(&bindings);
    for field in [
        "prefab",
        "overlay",
        "patch",
        "overrides",
        "contextual_defaults",
        "arguments",
    ] {
        for value in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!([]),
        ] {
            for location in [
                "payload",
                "entity",
                "component",
                "provenance",
                "origin",
                "step",
                "source_key",
                "source_origin",
            ] {
                let mut invalid = original.clone();
                let object = match location {
                    "payload" => invalid["payload"].as_object_mut().unwrap(),
                    "entity" => invalid["payload"]["entities"][0].as_object_mut().unwrap(),
                    "component" => invalid["payload"]["entities"][0]["components"]["game.Text"]
                        .as_object_mut()
                        .unwrap(),
                    "provenance" => invalid["payload"]["provenance"][0].as_object_mut().unwrap(),
                    "source_key" => invalid["payload"]["source_key"].as_object_mut().unwrap(),
                    "source_origin" => invalid["payload"]["source_key"]["origin"]
                        .as_object_mut()
                        .unwrap(),
                    "origin" => invalid["payload"]["provenance"][0]["origin"]
                        .as_object_mut()
                        .unwrap(),
                    _ => {
                        let entry = invalid["payload"]["provenance"]
                            .as_array_mut()
                            .unwrap()
                            .iter_mut()
                            .find(|entry| entry["origin"]["kind"] == "projected")
                            .unwrap();
                        entry["origin"]["steps"][0].as_object_mut().unwrap()
                    }
                };
                object.insert(field.into(), value.clone());
                assert!(
                    RuntimeSceneDocumentCandidate::from_json_bytes(
                        &serde_json::to_vec(&invalid).unwrap(),
                        RuntimeSceneLimits::default()
                    )
                    .is_err(),
                    "{field} {location}"
                );
            }
        }
    }
    for field in ["source_key", "provenance", "roots", "entities"] {
        let mut invalid = original.clone();
        invalid["payload"].as_object_mut().unwrap().remove(field);
        assert!(admit_json(&invalid, &bindings).is_err());
    }
    for field in ["version", "value"] {
        let mut invalid = original.clone();
        invalid["payload"]["entities"][0]["components"]["game.Text"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(admit_json(&invalid, &bindings).is_err());
    }
    let bytes = serde_json::to_string(&original).unwrap();
    let duplicate = bytes.replacen(
        "\"format_version\":1",
        "\"format_version\":1,\"format_version\":1",
        1,
    );
    assert_ne!(duplicate, bytes);
    assert!(
        RuntimeSceneDocumentCandidate::from_json_bytes(
            duplicate.as_bytes(),
            RuntimeSceneLimits::default()
        )
        .is_err()
    );
    for (field, value) in [
        ("kind", serde_json::json!("scene")),
        ("format_version", serde_json::json!(2)),
        ("format_version", serde_json::json!(0)),
    ] {
        let mut invalid = original.clone();
        invalid[field] = value;
        assert!(admit_json(&invalid, &bindings).is_err());
    }
}

#[test]
fn admission_rejects_wrong_source_and_incomplete_malformed_or_rekeyed_provenance() {
    let bindings = bindings();
    let original = cooked_json(&bindings);
    let bytes = serde_json::to_vec(&original).unwrap();
    assert!(
        RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, RuntimeSceneLimits::default())
            .unwrap()
            .admit(&key("different", SourceDocumentKind::Scene), &bindings)
            .is_err()
    );
    let mut variants = Vec::new();
    let mut invalid = original.clone();
    invalid["payload"]["source_key"]["kind"] = serde_json::json!("prefab");
    variants.push(invalid);
    let mut invalid = original.clone();
    invalid["payload"]["source_key"]["origin"] =
        serde_json::json!({"named": {"universe": "not-a-universe", "name": "bad"}});
    variants.push(invalid);
    let mut invalid = original.clone();
    invalid["payload"]["provenance"]
        .as_array_mut()
        .unwrap()
        .pop();
    variants.push(invalid);
    let mut invalid = original.clone();
    let duplicate = invalid["payload"]["provenance"][0].clone();
    invalid["payload"]["provenance"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    variants.push(invalid);
    let projected_index = original["payload"]["provenance"]
        .as_array()
        .unwrap()
        .iter()
        .position(|entry| entry["origin"]["kind"] == "projected")
        .unwrap();
    for field in ["root_anchor", "steps"] {
        let mut invalid = original.clone();
        invalid["payload"]["provenance"][projected_index]["origin"][field] =
            if field == "root_anchor" {
                serde_json::json!("absent")
            } else {
                serde_json::json!([])
            };
        variants.push(invalid);
    }
    let mut invalid = original.clone();
    invalid["payload"]["provenance"][projected_index]["local_entity"] =
        serde_json::json!("wrong-terminal");
    variants.push(invalid);
    let mut invalid = original.clone();
    invalid["payload"]["provenance"][projected_index]["entity"] = serde_json::json!("noncanonical");
    variants.push(invalid);
    let mut invalid = original.clone();
    let source_step = invalid["payload"]["provenance"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .flat_map(|entry| entry["origin"]["steps"].as_array_mut())
        .flat_map(|steps| steps.iter_mut())
        .find(|step| step["kind"] == "source_entity")
        .unwrap();
    source_step["canonical_source"]["kind"] = serde_json::json!("scene");
    variants.push(invalid);
    for invalid in variants {
        assert!(admit_json(&invalid, &bindings).is_err());
    }
}

#[test]
fn admission_refuses_current_schema_owner_shape_defaults_and_surviving_reference_failures() {
    let bindings = bindings();
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("a"))
                .with_component(
                    component("game.Link"),
                    record(ComponentValue::EntityReference(
                        EntityReference::SceneLocal { entity: id("b") },
                    )),
                )
                .with_component(
                    component("game.Defaults"),
                    record(ComponentValue::Map(BTreeMap::new())),
                )
                .with_component(
                    component("game.Image"),
                    record(encode_asset_reference_value(&primary())),
                ),
            SceneEntityRecord::new(id("b")),
        ]),
    );
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let original: serde_json::Value =
        serde_json::from_slice(&cooked.to_json_bytes().unwrap()).unwrap();
    for mutate in 0..8 {
        let mut invalid = original.clone();
        let components = &mut invalid["payload"]["entities"][0]["components"];
        match mutate {
            0 => components["game.Link"]["version"] = serde_json::json!(2),
            1 => components["game.Link"]["owner"] = serde_json::json!("game.foreign"),
            2 => {
                components["game.Link"]
                    .as_object_mut()
                    .unwrap()
                    .remove("owner");
            }
            3 => {
                components["game.Link"]["value"] =
                    serde_json::to_value(ComponentValue::Bool(false)).unwrap()
            }
            4 => {
                components["game.Link"]["value"] = serde_json::to_value(
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: id("missing"),
                    }),
                )
                .unwrap()
            }
            5 => {
                components["game.Defaults"]["value"] =
                    serde_json::to_value(ComponentValue::Map(BTreeMap::new())).unwrap()
            }
            6 => {
                components["game.Image"]["value"] = serde_json::to_value(
                    encode_asset_reference_value(&AssetRef::StableId(asset_id())),
                )
                .unwrap()
            }
            _ => {
                invalid["payload"]["entities"][0]["children"] = serde_json::json!(["missing"]);
            }
        }
        assert!(admit_json(&invalid, &bindings).is_err(), "case {mutate}");
    }
    let registry = ComponentRegistryBuilder::new().freeze().unwrap();
    let no_data = SceneComponentBindings::new(registry.clone(), []).unwrap();
    assert!(admit_json(&original, &no_data).is_err());
}

#[test]
fn consistent_flat_rekeying_is_rejected_before_a_runtime_candidate_escapes() {
    let bindings = bindings();
    let mut invalid = cooked_json(&bindings);
    let provenance = invalid["payload"]["provenance"].as_array_mut().unwrap();
    let entry = provenance
        .iter_mut()
        .find(|entry| entry["origin"]["kind"] == "projected")
        .unwrap();
    let old = entry["entity"].as_str().unwrap().to_owned();
    let replacement = serde_json::json!("processed-id-with-wrong-address");
    entry["entity"] = replacement.clone();
    for root in invalid["payload"]["roots"].as_array_mut().unwrap() {
        if root.as_str() == Some(old.as_str()) {
            *root = replacement.clone();
        }
    }
    for entity in invalid["payload"]["entities"].as_array_mut().unwrap() {
        if entity["id"].as_str() == Some(old.as_str()) {
            entity["id"] = replacement.clone();
        }
        for child in entity["children"].as_array_mut().unwrap() {
            if child.as_str() == Some(old.as_str()) {
                *child = replacement.clone();
            }
        }
        if let Some(record) = entity["components"]
            .as_object_mut()
            .unwrap()
            .get_mut("game.Link")
        {
            let mut value: ComponentValue =
                serde_json::from_value(record["value"].clone()).unwrap();
            if let ComponentValue::EntityReference(EntityReference::SceneLocal { entity }) =
                &mut value
                && entity.as_str() == old
            {
                *entity = id(replacement.as_str().unwrap());
            }
            record["value"] = serde_json::to_value(value).unwrap();
        }
    }
    let error = admit_json(&invalid, &bindings).unwrap_err();
    assert!(error.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "scene.runtime-noncanonical-entity-id"
    }));
}

#[test]
fn exact_and_one_short_encoded_working_provenance_count_depth_and_value_budgets() {
    let bindings = bindings();
    let candidate = RuntimeSceneCandidate::cook(
        &fixture(&bindings, "first"),
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let bytes = candidate.to_json_bytes().unwrap();
    let default = RuntimeSceneLimits::default();
    for (maximum, accepted) in [(bytes.len(), true), (bytes.len() - 1, false)] {
        let limits = default.with_base(
            default
                .base()
                .with_encoded_bytes(ByteLimit::new(maximum).unwrap()),
        );
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, limits).is_ok(),
            accepted
        );
    }
    let working = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, default)
        .unwrap()
        .work
        .used();
    for (maximum, accepted) in [(working, true), (working - 1, false)] {
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(
                &bytes,
                default.with_working_bytes(ByteLimit::new(maximum).unwrap())
            )
            .is_ok(),
            accepted
        );
    }
    let provenance = budget::provenance_cost(
        candidate.source_key(),
        candidate.canonical_provenance(),
        default,
    )
    .unwrap();
    for (maximum, accepted) in [(provenance, true), (provenance - 1, false)] {
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(
                &bytes,
                default.with_provenance_bytes(ByteLimit::new(maximum).unwrap())
            )
            .is_ok(),
            accepted
        );
    }
    let nodes = 1 + candidate
        .canonical_provenance()
        .values()
        .map(|origin| {
            1 + origin
                .projection_path()
                .map_or(0, |path| path.steps().len())
        })
        .sum::<usize>();
    for (maximum, accepted) in [(nodes, true), (nodes - 1, false)] {
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(
                &bytes,
                default.with_provenance_nodes(ItemLimit::new(maximum).unwrap())
            )
            .is_ok(),
            accepted
        );
    }
    let depth = candidate
        .canonical_provenance()
        .values()
        .filter_map(|origin| origin.projection_path())
        .map(|path| path.steps().len())
        .max()
        .unwrap();
    for (maximum, accepted) in [(depth, true), (depth - 1, false)] {
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(
                &bytes,
                default.with_provenance_depth(DepthLimit::new(maximum).unwrap())
            )
            .is_ok(),
            accepted
        );
    }
    for (maximum, accepted) in [
        (candidate.document().entities().len(), true),
        (candidate.document().entities().len() - 1, false),
    ] {
        assert_eq!(
            RuntimeSceneDocumentCandidate::from_json_bytes(
                &bytes,
                default.with_base(
                    default
                        .base()
                        .with_entities(ItemLimit::new(maximum).unwrap())
                )
            )
            .is_ok(),
            accepted
        );
    }
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots(["a", "b"].map(|entity| {
            SceneEntityRecord::new(id(entity)).with_component(
                component("game.Defaults"),
                record(ComponentValue::Map(BTreeMap::new())),
            )
        })),
    );
    // Each normalized value is two nodes and fourteen logical bytes. Each record retains
    // its version/owner metadata and component ID; each one-byte local ID appears in both
    // its root placement and entity record. Cook preserves these local IDs.
    let exact_bytes = 2 * (14 + 5 + owner_id().as_str().len() + "game.Defaults".len() + 2);
    for (nodes, bytes, accepted) in [
        (4, exact_bytes, true),
        (3, exact_bytes, false),
        (4, exact_bytes - 1, false),
    ] {
        let limits = default.with_base(
            default
                .base()
                .with_component_value_nodes(ItemLimit::new(nodes).unwrap())
                .with_component_value_bytes(ByteLimit::new(bytes).unwrap()),
        );
        let result = RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            database().resolver(&nara_asset::AssetPathScope::ROOT),
            limits,
        );
        assert_eq!(
            result.is_ok(),
            accepted,
            "{nodes} nodes, {bytes} bytes: {result:?}"
        );
    }
}

#[test]
fn final_asset_collection_and_aggregate_inventory_join_runtime_working_admission() {
    let bindings = bindings();
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots(["a", "b"].map(|entity| {
            SceneEntityRecord::new(id(entity)).with_component(
                component("game.Image"),
                record(encode_asset_reference_value(&primary())),
            )
        })),
    );
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let default = RuntimeSceneLimits::default();
    let bytes = cooked.to_json_bytes().unwrap();
    let decoded = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, default).unwrap();
    let mut measured = budget::WorkBudget::new(default);
    admit_flat_claims(decoded.claims, &bindings, &mut measured).unwrap();
    assert!(measured.used() > 1);
    for (maximum, accepted) in [(measured.used(), true), (measured.used() - 1, false)] {
        let mut work =
            budget::WorkBudget::new(default.with_working_bytes(ByteLimit::new(maximum).unwrap()));
        let decoded = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, default).unwrap();
        let result = admit_flat_claims(decoded.claims, &bindings, &mut work);
        assert_eq!(result.is_ok(), accepted);
        if let Err(error) = result {
            assert!(
                error
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str() == "scene.runtime-budget")
            );
        }
    }
}

#[test]
fn wire_shape_preflight_rejects_depth_count_and_duplicate_component_keys() {
    let bindings = bindings();
    let original = cooked_json(&bindings);
    let bytes = serde_json::to_vec(&original).unwrap();
    let shape = SerdeShapeLimits::new(
        DepthLimit::ONE,
        ItemLimit::new(100_000).unwrap(),
        ItemLimit::new(100_000).unwrap(),
        ByteLimit::new(1024 * 1024).unwrap(),
        ByteLimit::new(8 * 1024 * 1024).unwrap(),
    );
    let limits =
        RuntimeSceneLimits::default().with_base(SceneFileLimits::default().with_shape(shape));
    assert!(RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, limits).is_err());
    let shape = SerdeShapeLimits::new(
        DepthLimit::new(64).unwrap(),
        ItemLimit::ONE,
        ItemLimit::new(100_000).unwrap(),
        ByteLimit::new(1024 * 1024).unwrap(),
        ByteLimit::new(8 * 1024 * 1024).unwrap(),
    );
    let limits =
        RuntimeSceneLimits::default().with_base(SceneFileLimits::default().with_shape(shape));
    assert!(RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, limits).is_err());
    let duplicate = String::from_utf8(bytes).unwrap().replacen(
        "\"components\":{}",
        "\"components\":{\"game.Text\":null,\"game.Text\":null}",
        1,
    );
    assert!(
        RuntimeSceneDocumentCandidate::from_json_bytes(
            duplicate.as_bytes(),
            RuntimeSceneLimits::default()
        )
        .is_err()
    );
}

#[test]
fn raw_decode_and_admission_never_invoke_native_codecs() {
    #[derive(nara_ecs::Component)]
    struct Counted;
    let calls = Arc::new(AtomicUsize::new(0));
    let decode_calls = Arc::clone(&calls);
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component_with_codec::<Counted, _, _>(
            root_schema("game.Native", ComponentValueShape::EntityRef),
            move |_| {
                decode_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Counted)
            },
            |_| {
                Ok(ComponentValue::EntityReference(
                    EntityReference::SceneLocal {
                        entity: id("local"),
                    },
                ))
            },
        )
        .unwrap();
    let registry = registry.freeze().unwrap();
    let bindings = SceneComponentBindings::new(registry.clone(), []).unwrap();
    let input = compile(
        &bindings,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local")).with_component(
            component("game.Native"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("local"),
                }),
            ),
        )]),
    );
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap();
    let bytes = cooked.to_json_bytes().unwrap();
    RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
        .unwrap()
        .admit(&containing(), &bindings)
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    for reference in [
        nara_identity::AuthoredEntityReference::source(id("outside")),
        nara_identity::AuthoredEntityReference::input(
            nara_identity::ReferenceInputId::new("required-input").unwrap(),
        ),
    ] {
        for metadata in [false, true] {
            let mut malicious: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let component = &mut malicious["payload"]["entities"][0]["components"]["game.Native"];
            let authored =
                serde_json::to_value(ComponentValue::EntityReference(EntityReference::Authored {
                    reference: reference.clone(),
                }))
                .unwrap();
            if metadata {
                // Even a hidden declaration is source metadata and cannot enter processed
                // Scene admission beside an otherwise flat explicit value.
                component["contextual_defaults"] = serde_json::json!({"value": authored});
            } else {
                component["value"] = authored;
            }
            let decoded = RuntimeSceneDocumentCandidate::from_json_bytes(
                &serde_json::to_vec(&malicious).unwrap(),
                cooked.limits(),
            );
            if metadata {
                assert!(decoded.is_err());
            } else {
                let error = decoded
                    .unwrap()
                    .admit(&containing(), &bindings)
                    .unwrap_err();
                assert!(error.diagnostics().iter().any(|diagnostic| {
                    diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "codec-error-kind"
                            && field.value()
                                == nara_diagnostic::DiagnosticValueRef::Identifier(
                                    "unresolved-authored-reference",
                                )
                    })
                }));
            }
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn native_owner_claims_remain_exact_across_owned_and_unowned_receiving_definitions() {
    #[derive(nara_ecs::Component)]
    struct Native;

    let make_bindings = |owner: Option<&'static str>| {
        let mut registry = match owner {
            None => ComponentRegistryBuilder::new(),
            Some(owner) => ComponentRegistryBuilder::from_owner_catalog_candidate(
                nara_reflect::ComponentSchemaOwnerId::new(owner),
                ComponentSchemaCatalog::default(),
                None,
            )
            .unwrap(),
        };
        registry
            .register_persistent_component_with_codec::<Native, _, _>(
                root_schema("game.Native", ComponentValueShape::EntityRef),
                |_| Ok(Native),
                |_| {
                    Ok(ComponentValue::EntityReference(
                        EntityReference::SceneLocal {
                            entity: id("local"),
                        },
                    ))
                },
            )
            .unwrap();
        SceneComponentBindings::new(registry.freeze().unwrap(), []).unwrap()
    };
    let owned = make_bindings(Some("game.native"));
    let foreign = make_bindings(Some("game.foreign"));
    let unowned = make_bindings(None);
    let source = SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local"))
        .with_component(
            component("game.Native"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: id("local"),
                }),
            ),
        )]);
    let database = ProjectAssetDatabase::default();
    for (sender, owner) in [(&owned, Some("game.native")), (&unowned, None)] {
        let input = compile(sender, source.clone());
        let record = &input.document().entities()[0].components()[&component("game.Native")];
        assert_eq!(record.schema_owner().map(|owner| owner.as_str()), owner);
        let cooked = RuntimeSceneCandidate::cook(
            &input,
            sender,
            database.resolver(&nara_asset::AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&cooked.to_json_bytes().unwrap()).unwrap();
        assert!(admit_json(&original, sender).is_ok());
        assert!(admit_json(&original, &foreign).is_err());
        let receiver = if owner.is_some() { &unowned } else { &owned };
        assert!(admit_json(&original, receiver).is_err());
        assert!(
            RuntimeSceneCandidate::cook(
                &input,
                receiver,
                database.resolver(&nara_asset::AssetPathScope::ROOT),
                RuntimeSceneLimits::default(),
            )
            .is_err()
        );
        let mut invalid = original;
        let record = invalid["payload"]["entities"][0]["components"]["game.Native"]
            .as_object_mut()
            .unwrap();
        if owner.is_some() {
            record.remove("owner");
        } else {
            record.insert("owner".into(), serde_json::json!("game.native"));
        }
        assert!(admit_json(&invalid, sender).is_err());
    }
}

#[test]
fn cook_does_not_synthesize_defaults_from_a_different_receiving_schema() {
    let make_bindings = |extra| {
        let mut fields = vec![
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("amount"),
                "Amount",
                ComponentFieldPath::from_fields(["amount"]),
                ComponentValueKind::U64,
                ComponentValue::U64(7),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ];
        if extra {
            fields.push(
                ComponentFieldSchema::optional_with_default(
                    ComponentFieldId::new("extra"),
                    "Extra",
                    ComponentFieldPath::from_fields(["extra"]),
                    ComponentValueKind::Bool,
                    ComponentValue::Bool(false),
                )
                .with_capabilities(ComponentCapability::SCENE_AUTHORING),
            );
        }
        let schema = ComponentSchema::new(
            component("game.Defaults"),
            "Defaults",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields(fields);
        let owner = ComponentSchemaOwnerSnapshot::admit(
            owner_id(),
            ComponentSchemaCatalog {
                components: vec![schema],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        SceneComponentBindings::new(ComponentRegistryBuilder::new().freeze().unwrap(), [owner])
            .unwrap()
    };
    let original = make_bindings(false);
    let changed = make_bindings(true);
    let input = compile(
        &original,
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(id("local")).with_component(
            component("game.Defaults"),
            record(ComponentValue::Map(BTreeMap::new())),
        )]),
    );
    let before = format!("{input:?}");
    assert!(
        RuntimeSceneCandidate::cook(
            &input,
            &changed,
            ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .is_err()
    );
    assert_eq!(format!("{input:?}"), before);
}

#[test]
fn cook_checks_complete_output_encoding_and_derived_id_strings_without_changing_input() {
    let bindings = bindings();
    let input = fixture(&bindings, "first");
    let before = format!("{input:?}");
    let default = RuntimeSceneLimits::default();
    let cooked = RuntimeSceneCandidate::cook(
        &input,
        &bindings,
        database().resolver(&nara_asset::AssetPathScope::ROOT),
        default,
    )
    .unwrap();
    let encoded = cooked.to_json_bytes().unwrap().len();
    let longest_id = cooked
        .document()
        .entities()
        .iter()
        .map(|entity| entity.id().as_str().len())
        .max()
        .unwrap();
    for (maximum, accepted) in [(encoded, true), (encoded - 1, false)] {
        let limits = default.with_base(
            default
                .base()
                .with_encoded_bytes(ByteLimit::new(maximum).unwrap()),
        );
        let result = RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            database().resolver(&nara_asset::AssetPathScope::ROOT),
            limits,
        );
        assert_eq!(result.is_ok(), accepted);
    }
    for (maximum, accepted) in [(longest_id, true), (longest_id - 1, false)] {
        let shape = default.base().shape();
        let shape = SerdeShapeLimits::new(
            shape.depth(),
            shape.nodes(),
            shape.container_items(),
            ByteLimit::new(maximum).unwrap(),
            shape.total_string_bytes(),
        );
        let limits = default.with_base(default.base().with_shape(shape));
        let result = RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            database().resolver(&nara_asset::AssetPathScope::ROOT),
            limits,
        );
        assert_eq!(result.is_ok(), accepted);
    }
    assert_eq!(format!("{input:?}"), before);
}

#[test]
fn product_lookup_evidence_preserves_named_and_project_location_canonical_identity() {
    let bindings = bindings();
    let base = compile(
        &bindings,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("anchor")),
            SceneEntityRecord::new(id("generated")),
        ]),
    );
    let location = SourceDocumentKey::project_location(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        AssetSourcePath::new("prefabs/canonical.ron").unwrap(),
        SourceDocumentKind::Prefab,
    );
    let keys = [
        prefab_key("canonical.prefab"),
        location,
        SourceDocumentKey::stable_asset(asset_id(), SourceDocumentKind::Prefab),
    ];
    let mut ids = std::collections::BTreeSet::new();
    for canonical in keys {
        let mut claims = copy_claims(&base);
        claims.canonical_provenance.insert(
            id("generated"),
            SceneEntityProvenance::projected(
                id("leaf"),
                PrefabProjectionPath::from_resolved_steps(
                    id("anchor"),
                    vec![PrefabProjectionStep::SourceEntity {
                        source_document: primary(),
                        canonical_source: canonical.clone(),
                        entity: id("leaf"),
                    }],
                ),
            ),
        );
        let input = seal_claims(claims, &bindings);
        let cooked = RuntimeSceneCandidate::cook(
            &input,
            &bindings,
            ProjectAssetDatabase::default().resolver(&nara_asset::AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap();
        let bytes = cooked.to_json_bytes().unwrap();
        let admitted = RuntimeSceneDocumentCandidate::from_json_bytes(&bytes, cooked.limits())
            .unwrap()
            .admit(&containing(), &bindings)
            .unwrap();
        assert_eq!(admitted.document(), cooked.document());
        let mut invalid: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let projected = invalid["payload"]["provenance"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["origin"]["kind"] == "projected")
            .unwrap();
        projected["origin"]["steps"][0]["source_document"] =
            serde_json::to_value(AssetRef::Product(
                AssetProductRef::new(
                    asset_id(),
                    ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000042")
                        .unwrap(),
                )
                .unwrap(),
            ))
            .unwrap();
        assert!(admit_json(&invalid, &bindings).is_err());
        let (entity, origin) = cooked
            .canonical_provenance()
            .iter()
            .find(|(_, origin)| origin.projection_path().is_some())
            .unwrap();
        assert!(ids.insert(entity.clone()));
        assert!(matches!(&origin.projection_path().unwrap().steps()[0],
            PrefabProjectionStep::SourceEntity { source_document, canonical_source, .. }
                if source_document == &primary() && canonical_source == &canonical));
    }
}

#[test]
fn identity_framing_keeps_origin_kind_steps_root_and_containing_namespace_distinct() {
    let bindings = bindings();
    let source = containing();
    let projected = |steps| {
        SceneEntityProvenance::projected(
            id("terminal"),
            PrefabProjectionPath::from_resolved_steps(id("anchor"), steps),
        )
    };
    let source_step = PrefabProjectionStep::SourceEntity {
        source_document: AssetRef::path("aliases/source.ron").unwrap(),
        canonical_source: prefab_key("p"),
        entity: id("terminal"),
    };
    let addition = PrefabProjectionStep::InstanceAddition {
        entity: id("terminal"),
    };
    let origins = [
        projected(vec![source_step.clone()]),
        projected(vec![addition.clone()]),
        projected(vec![source_step, addition]),
    ];
    let compiled = compile(
        &bindings,
        SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(id("anchor")),
            SceneEntityRecord::new(id("generated")),
        ]),
    );
    let mut outputs = std::collections::BTreeSet::new();
    for origin in origins {
        let mut claims = copy_claims(&compiled);
        claims.canonical_provenance = [
            (id("anchor"), SceneEntityProvenance::local(id("anchor"))),
            (id("generated"), origin),
        ]
        .into_iter()
        .collect();
        let document = seal_claims(claims, &bindings);
        for source in [&source, &key("other.scene", SourceDocumentKind::Scene)] {
            let ids = identity::canonical_ids(
                source,
                document.document(),
                false,
                &mut budget::WorkBudget::new(RuntimeSceneLimits::default()),
            )
            .unwrap();
            assert!(outputs.insert(ids[&id("generated")].clone()));
        }
    }
}
