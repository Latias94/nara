use nara_asset::{AssetPath, AssetProductRef, ImportedProductId, StableAssetId};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability, ComponentFieldId,
    ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath, ComponentPropertyStep,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeMember,
    ComponentTypeId, ComponentValue, ComponentValueKind, ComponentValueShape,
    PersistentComponentSchemaOwnerId,
};

use super::*;
use crate::{
    __private::AuthoringAdapterReceipt, PrefabAddition, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceClaim,
    PrefabInstanceHop, PrefabOverlay, PrefabPlacement, PrefabPlacementParent, PrefabPlacementSlot,
    PrefabPropertyClaim, PrefabPropertyIntent, PrefabReferenceBinding, SceneComponentRecord,
    SourceDocumentOrigin,
};

#[path = "tests/refusals.rs"]
mod refusals;

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}
fn component(name: &str) -> ComponentTypeId {
    ComponentTypeId::new(format!("test.Copy{name}"))
}
fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("test.prefab-copy").unwrap()
}
fn reference(name: &str) -> AssetRef {
    AssetRef::path(format!("prefabs/{name}.prefab.json")).unwrap()
}
fn asset(reference: &AssetRef) -> ComponentValue {
    crate::patch::asset_ref_value(reference)
}

fn product(source: StableAssetId, primary: bool) -> AssetRef {
    AssetRef::Product(
        AssetProductRef::new(
            source,
            if primary {
                ImportedProductId::PRIMARY
            } else {
                ImportedProductId::parse_subproduct("00000000-0000-4000-8000-000000000033").unwrap()
            },
        )
        .unwrap(),
    )
}
fn field(name: &str, kind: ComponentValueKind) -> ComponentFieldSchema {
    ComponentFieldSchema::required(
        ComponentFieldId::new(name),
        name,
        ComponentFieldPath::from_fields([name]),
        kind,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
}
fn record(value: ComponentValue) -> SceneComponentRecord {
    SceneComponentRecord::new(ComponentSchemaVersion::ONE, value).with_owner(owner())
}
fn basis() -> AuthoringSchemaBasis {
    let asset_field = ComponentFieldSchema::optional_with_default(
        ComponentFieldId::new("asset"),
        "Asset",
        ComponentFieldPath::from_fields(["asset"]),
        ComponentValueKind::AssetRef,
        asset(&reference("leaf")),
    )
    .with_capabilities([
        ComponentCapability::Scene,
        ComponentCapability::Inspect,
        ComponentCapability::Edit,
        ComponentCapability::AssetRef,
    ]);
    let schemas = [
        ComponentSchema::new(component("Assets"), "Assets", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([asset_field, field("plain", ComponentValueKind::String)]),
        ComponentSchema::new(component("Link"), "Link", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field("target", ComponentValueKind::EntityRef)
                .with_capability(ComponentCapability::EntityRef)]),
        ComponentSchema::new(component("Number"), "Number", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field("value", ComponentValueKind::U64)]),
        ComponentSchema::new(component("Marker"), "Marker", ComponentSchemaVersion::ONE)
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_fields([field("value", ComponentValueKind::U64)]),
        ComponentSchema::new(
            component("Container"),
            "Container",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::record_members([ComponentShapeMember::new(
                "source",
                ComponentValueShape::AssetRef,
            )
            .with_id(ComponentFieldId::new("source-member"))]),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_capability(ComponentCapability::AssetRef)]),
    ];
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        owner(),
        ComponentSchemaCatalog {
            components: schemas.into(),
            ..Default::default()
        },
        None,
    )])
    .unwrap();
    let adapter =
        AuthoringAdapterReceipt::from_schema_owner(&bundle.owners()[0], &Default::default())
            .unwrap();
    AuthoringSchemaBasis::admit(&bundle, &[adapter]).unwrap()
}

fn key(name: &str) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
        name,
        SourceDocumentKind::Prefab,
    )
    .unwrap()
}
fn mapped_source(name: &str, document: PrefabDocument) -> PrefabCopySource {
    PrefabCopySource::new(
        key(name),
        key(&format!("copied-{name}")),
        vec![reference(name)],
        reference(&format!("copied-{name}")),
        document,
    )
}
fn instance(name: &str, overlay: Option<PrefabOverlay>) -> PrefabInstance {
    PrefabInstance {
        source: reference(name),
        overrides: overlay,
    }
}
fn anchor(name: &str, source: &str, overlay: Option<PrefabOverlay>) -> SceneEntityRecord {
    let mut entity = SceneEntityRecord::new(id(name));
    entity.prefab = Some(instance(source, overlay));
    entity
}
fn assets() -> SceneComponentRecord {
    // The omitted asset default is semantic data; the same spelling in plain must stay unchanged.
    record(ComponentValue::map([(
        "plain",
        ComponentValue::String("prefabs/leaf.prefab.json".into()),
    )]))
}
fn link(target: &str) -> SceneComponentRecord {
    record(ComponentValue::map([(
        "target",
        ComponentValue::EntityReference(nara_reflect::EntityReference::SceneLocal {
            entity: id(target),
        }),
    )]))
}
fn number() -> SceneComponentRecord {
    record(ComponentValue::map([("value", ComponentValue::U64(1))]))
}
fn through(name: &str) -> PrefabEntityTarget {
    PrefabEntityTarget {
        through: vec![PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(id("nested")),
            expected_base: key("leaf"),
        }],
        entity: PrefabEntityOrigin::SourceEntity(id(name)),
    }
}
fn binding(target: PrefabEntityTarget) -> PrefabReferenceBinding {
    PrefabReferenceBinding {
        slot: ComponentFieldPath::from_fields(["target"]),
        expected: id("b"),
        target,
    }
}
fn root_placement(target: PrefabEntityTarget) -> PrefabPlacement {
    PrefabPlacement {
        target,
        parent: PrefabPlacementParent::Root,
        slot: PrefabPlacementSlot::Last,
    }
}
fn property(name: &str, value: ComponentValue) -> PrefabPropertyClaim {
    PrefabPropertyClaim {
        path: ComponentPropertyPath::new(ComponentFieldId::new(name)),
        intent: PrefabPropertyIntent::Set(value),
        bindings: Vec::new(),
    }
}

fn nested_fixture() -> PrefabCopyRequest {
    let leaf = PrefabDocument::new(
        [id("leaf-root")],
        [
            SceneEntityRecord::new(id("leaf-root")).with_children([
                id("a"),
                id("b"),
                id("c"),
                id("hidden"),
                id("claim-anchor"),
            ]),
            SceneEntityRecord::new(id("a"))
                .with_component(component("Assets"), assets())
                .with_component(component("Number"), number())
                .with_component(component("Marker"), number()),
            SceneEntityRecord::new(id("b")).with_component(component("Link"), link("a")),
            SceneEntityRecord::new(id("c")),
            SceneEntityRecord::new(id("hidden")),
            SceneEntityRecord::new(id("claim-anchor")),
        ],
    );
    let middle = PrefabDocument::from_ordered_roots([anchor("nested", "leaf", None)]);
    let mut overlay = PrefabOverlay::new(key("middle"));

    let mut nested_addition = PrefabOverlay::new(key("leaf"));
    nested_addition
        .additions
        .push(PrefabAddition::new(id("addition-child")));
    nested_addition
        .placements
        .push(root_placement(PrefabEntityTarget::addition(id(
            "addition-child",
        ))));
    let mut addition = PrefabAddition::new(id("added"));
    addition.prefab = Some(instance("leaf", Some(nested_addition)));
    addition.components.insert(component("Link"), link("b"));
    addition
        .bindings
        .insert(component("Link"), vec![binding(through("b"))]);
    overlay.additions.push(addition);
    overlay
        .additions
        .push(PrefabAddition::new(id("before-added")));

    let mut nested_claim = PrefabOverlay::new(key("leaf"));
    let mut child = PrefabAddition::new(id("claim-child"));
    child.components.insert(component("Assets"), assets());
    nested_claim.additions.push(child);
    nested_claim
        .placements
        .push(root_placement(PrefabEntityTarget::addition(id(
            "claim-child",
        ))));
    overlay.instances.push(PrefabInstanceClaim {
        target: through("claim-anchor"),
        value: Some(instance("leaf", Some(nested_claim))),
    });
    overlay.components.push(PrefabComponentIntent::Set {
        target: through("a"),
        component: component("Link"),
        mode: PrefabComponentSetMode::Add,
        record: link("b"),
        bindings: vec![binding(through("b"))],
    });
    overlay.components.push(PrefabComponentIntent::Properties {
        target: through("a"),
        component: component("Assets"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![property("asset", asset(&reference("middle")))],
    });
    overlay.components.push(PrefabComponentIntent::Properties {
        target: through("a"),
        component: component("Number"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![property("value", ComponentValue::U64(7))],
    });
    overlay.components.push(PrefabComponentIntent::Suppress {
        target: through("a"),
        component: component("Marker"),
    });
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("added")),
        parent: PrefabPlacementParent::Entity(through("leaf-root")),
        slot: PrefabPlacementSlot::After(through("b")),
    });
    overlay.placements.push(PrefabPlacement {
        target: PrefabEntityTarget::addition(id("before-added")),
        parent: PrefabPlacementParent::Entity(through("leaf-root")),
        slot: PrefabPlacementSlot::Before(through("b")),
    });
    overlay.placements.push(PrefabPlacement {
        target: through("c"),
        parent: PrefabPlacementParent::Entity(through("leaf-root")),
        slot: PrefabPlacementSlot::Last,
    });
    overlay.suppressions.push(through("hidden"));
    let outer = PrefabDocument::from_ordered_roots([anchor("host", "middle", Some(overlay))]);
    PrefabCopyRequest::new(vec![
        mapped_source("outer", outer),
        mapped_source("middle", middle),
        mapped_source("leaf", leaf),
    ])
}

#[test]
fn production_preparation_copies_complete_nested_overlay_closure() {
    let request = nested_fixture();
    let originals: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    for ((original, source), result) in originals
        .iter()
        .zip(&request.sources)
        .zip(prepared.sources())
    {
        assert_eq!(&source.document, original);
        assert_eq!(result.original(), &source.original);
        assert_eq!(result.destination(), &source.destination);
        assert_eq!(
            result.destination_reference(),
            &source.destination_reference
        );
        assert_eq!(result.document.roots, original.roots);
        assert_eq!(
            result
                .document
                .entities
                .iter()
                .map(|entity| &entity.id)
                .collect::<Vec<_>>(),
            original
                .entities
                .iter()
                .map(|entity| &entity.id)
                .collect::<Vec<_>>()
        );
    }
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    assert_eq!(overlay.base, key("copied-middle"));
    assert_eq!(
        overlay.additions[0].prefab.as_ref().unwrap().source,
        reference("copied-leaf")
    );
    assert_eq!(
        overlay.additions[0]
            .prefab
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .base,
        key("copied-leaf")
    );
    assert_eq!(
        overlay.instances[0]
            .value
            .as_ref()
            .unwrap()
            .overrides
            .as_ref()
            .unwrap()
            .base,
        key("copied-leaf")
    );
    let check = |target: &PrefabEntityTarget| {
        assert!(!target.through.is_empty());
        for hop in &target.through {
            assert_eq!(hop.expected_base, key("copied-leaf"));
        }
    };
    check(&overlay.additions[0].bindings[&component("Link")][0].target);
    check(&overlay.instances[0].target);
    for intent in &overlay.components {
        match intent {
            PrefabComponentIntent::Set {
                target, bindings, ..
            } => {
                check(target);
                check(&bindings[0].target);
            }
            PrefabComponentIntent::Properties {
                target,
                component: ty,
                claims,
                ..
            } => {
                check(target);
                if ty == &component("Assets") {
                    assert_eq!(
                        claims[0].intent,
                        PrefabPropertyIntent::Set(asset(&reference("copied-middle")))
                    );
                }
            }
            PrefabComponentIntent::Suppress { target, .. } => check(target),
        }
    }
    for placement in &overlay.placements {
        if !placement.target.through.is_empty() {
            check(&placement.target);
        }
        if let PrefabPlacementParent::Entity(target) = &placement.parent {
            check(target);
        }
        if let PrefabPlacementSlot::Before(target) | PrefabPlacementSlot::After(target) =
            &placement.slot
        {
            check(target);
        }
    }
    check(&overlay.suppressions[0]);
    let leaf = prepared.sources()[2].document();
    let a = leaf
        .entities
        .iter()
        .find(|entity| entity.id == id("a"))
        .unwrap();
    assert_eq!(a.components[&component("Assets")].owner(), Some(&owner()));
    assert_eq!(
        a.components[&component("Assets")]
            .value
            .get_path(&ComponentFieldPath::from_fields(["asset"]))
            .unwrap(),
        &asset(&reference("copied-leaf"))
    );
    assert_eq!(
        a.components[&component("Assets")]
            .value
            .get_path(&ComponentFieldPath::from_fields(["plain"]))
            .unwrap(),
        &ComponentValue::String("prefabs/leaf.prefab.json".into())
    );
    let b = leaf
        .entities
        .iter()
        .find(|entity| entity.id == id("b"))
        .unwrap();
    assert_eq!(b.components[&component("Link")], link("a"));
    let mut budget = CopyBudget::new(PrefabCopyLimits::default());
    // This calls the same actual compiler a second time as an explicit consumability assertion.
    validate_closure(prepared.sources(), &basis(), &mut budget).unwrap();
    assert_eq!(prepared.into_sources().len(), 3);
}

#[test]
fn property_payload_bindings_and_multiple_hops_are_rewritten() {
    let mut request = nested_fixture();
    // The nested leaf's a needs a base Link for a property pin.
    request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("a"))
        .unwrap()
        .components
        .insert(component("Link"), link("b"));
    let outer = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let mut inner = through("a");
    inner.through.push(PrefabInstanceHop {
        anchor: PrefabEntityOrigin::SourceEntity(id("claim-anchor")),
        expected_base: key("leaf"),
    });
    outer.components.push(PrefabComponentIntent::Properties {
        target: inner,
        component: component("Link"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("target")),
            intent: PrefabPropertyIntent::Set(ComponentValue::EntityReference(
                nara_reflect::EntityReference::SceneLocal { entity: id("b") },
            )),
            bindings: vec![PrefabReferenceBinding {
                slot: ComponentFieldPath::empty(),
                expected: id("b"),
                target: through("b"),
            }],
        }],
    });
    if let PrefabComponentIntent::Set { mode, .. } = &mut outer.components[0] {
        *mode = PrefabComponentSetMode::Replace;
    }
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Properties { target, claims, .. } =
        overlay.components.last().unwrap()
    else {
        panic!("property intent");
    };
    assert_eq!(target.through.len(), 2);
    assert!(
        target
            .through
            .iter()
            .all(|hop| hop.expected_base == key("copied-leaf"))
    );
    assert_eq!(
        claims[0].bindings[0].target.through[0].expected_base,
        key("copied-leaf")
    );
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(ComponentValue::EntityReference(
            nara_reflect::EntityReference::SceneLocal { entity: id("b") }
        ))
    );
}

#[test]
fn admitted_stable_and_project_location_aliases_use_preferred_destination_reference() {
    for stable in [false, true] {
        let old_id: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
        let new_id: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
        let universe = "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap();
        let (original, destination, preferred) = if stable {
            (
                SourceDocumentKey::stable_asset(old_id, SourceDocumentKind::Prefab),
                SourceDocumentKey::stable_asset(new_id, SourceDocumentKind::Prefab),
                AssetRef::StableId(new_id),
            )
        } else {
            (
                SourceDocumentKey::project_location(
                    universe,
                    AssetPath::new("old/leaf.prefab.json").unwrap(),
                    SourceDocumentKind::Prefab,
                ),
                SourceDocumentKey::project_location(
                    universe,
                    AssetPath::new("new/leaf.prefab.json").unwrap(),
                    SourceDocumentKind::Prefab,
                ),
                reference("copied-leaf"),
            )
        };
        let mut overlay = PrefabOverlay::new(original.clone());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(id("leaf")),
            component: component("Assets"),
            version: ComponentSchemaVersion::ONE,
            claims: vec![property("asset", asset(&AssetRef::StableId(old_id)))],
        });
        let outer = PrefabDocument::from_ordered_roots([anchor("host", "leaf", Some(overlay))]);
        let request = PrefabCopyRequest::new(vec![
            mapped_source("outer", outer),
            PrefabCopySource::new(
                original.clone(),
                destination.clone(),
                vec![reference("leaf"), AssetRef::StableId(old_id)],
                preferred.clone(),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))
                    .with_component(component("Assets"), assets())]),
            ),
        ]);
        let prepared = request
            .prepare(&basis(), PrefabCopyLimits::default())
            .unwrap();
        let outer = prepared.sources()[0].document.entities[0]
            .prefab
            .as_ref()
            .unwrap();
        assert_eq!(outer.source, preferred);
        assert_eq!(outer.overrides.as_ref().unwrap().base, destination);
        assert_eq!(prepared.sources()[1].original(), &original);
        assert_eq!(
            matches!(
                destination.origin(),
                SourceDocumentOrigin::StableAsset { .. }
            ),
            stable
        );
        let PrefabComponentIntent::Properties { claims, .. } =
            &outer.overrides.as_ref().unwrap().components[0]
        else {
            panic!("property intent");
        };
        assert_eq!(
            claims[0].intent,
            PrefabPropertyIntent::Set(asset(&preferred))
        );
    }
}

#[test]
fn nested_primary_product_source_resolves_both_preferred_destination_spellings() {
    let old: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    let original = SourceDocumentKey::stable_asset(old, SourceDocumentKind::Prefab);
    let destination = SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab);
    for preferred in [AssetRef::StableId(new), reference("copied-leaf")] {
        let mut nested = anchor("host", "leaf", Some(PrefabOverlay::new(original.clone())));
        nested.prefab.as_mut().unwrap().source = product(old, true);
        let request = PrefabCopyRequest::new(vec![
            mapped_source("outer", PrefabDocument::from_ordered_roots([nested])),
            PrefabCopySource::new(
                original.clone(),
                destination.clone(),
                vec![reference("leaf"), AssetRef::StableId(old)],
                preferred.clone(),
                PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))]),
            ),
        ]);
        // Preparation includes production compilation of the copied nested instance, rather
        // than only checking the rewritten asset value in an ordinary component field.
        let prepared = request
            .prepare(&basis(), PrefabCopyLimits::default())
            .unwrap();
        let nested = prepared.sources()[0].document.entities[0]
            .prefab
            .as_ref()
            .unwrap();
        assert_eq!(nested.source, product(new, true));
        assert_eq!(nested.overrides.as_ref().unwrap().base, destination);
        assert_eq!(prepared.sources()[1].destination_reference(), &preferred);
        assert_eq!(
            request.sources[0].document.entities[0]
                .prefab
                .as_ref()
                .unwrap()
                .source,
            product(old, true)
        );
    }
}

#[test]
fn destination_product_resolution_requires_unique_keys_and_exact_aliases() {
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    let destination = SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab);
    for (other_destination, other_reference, duplicate_key) in [
        (destination.clone(), reference("copied-other"), true),
        (key("copied-other"), product(new, true), false),
    ] {
        let request = PrefabCopyRequest::new(vec![
            PrefabCopySource::new(
                key("leaf"),
                destination.clone(),
                vec![reference("leaf")],
                product(new, true),
                PrefabDocument::default(),
            ),
            PrefabCopySource::new(
                key("other"),
                other_destination,
                vec![reference("other")],
                other_reference,
                PrefabDocument::default(),
            ),
        ]);
        let error = refusals::prepare_error(&request);
        if duplicate_key {
            assert!(matches!(
                error,
                PrefabCopyError::DuplicateDestination { source: 1 }
            ));
        } else {
            assert!(matches!(
                error,
                PrefabCopyError::DestinationAliasCollision { source: 1 }
            ));
        }
    }
}

#[test]
fn selected_nested_asset_shape_uses_scalar_payload_relative_paths() {
    let mut request = nested_fixture();
    request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("a"))
        .unwrap()
        .components
        .insert(
            component("Container"),
            record(ComponentValue::map([(
                "payload",
                ComponentValue::map([("source", asset(&reference("leaf")))]),
            )])),
        );
    let overlay = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    overlay.components.push(PrefabComponentIntent::Properties {
        target: through("a"),
        component: component("Container"),
        version: ComponentSchemaVersion::ONE,
        claims: vec![PrefabPropertyClaim {
            path: ComponentPropertyPath::new(ComponentFieldId::new("payload")).with_steps([
                ComponentPropertyStep::Member(ComponentFieldId::new("source-member")),
            ]),
            intent: PrefabPropertyIntent::Set(asset(&reference("middle"))),
            bindings: vec![],
        }],
    });
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = overlay.components.last().unwrap()
    else {
        panic!("nested property");
    };
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(asset(&reference("copied-middle")))
    );
    assert_eq!(
        claims[0].path.steps,
        vec![ComponentPropertyStep::Member(ComponentFieldId::new(
            "source-member"
        ))]
    );
}

#[test]
fn nested_prefab_copy_remaps_product_sources_without_rekeying_inner_products() {
    let old: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    let mut request = nested_fixture();
    request.sources[2]
        .original_references
        .push(AssetRef::StableId(old));
    request.sources[2].destination_reference = AssetRef::StableId(new);
    request.sources[2].destination =
        SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab);
    let leaf = request.sources[2]
        .document
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("a"))
        .unwrap();
    leaf.components.insert(
        component("Assets"),
        record(ComponentValue::map([
            ("asset", asset(&product(old, false))),
            (
                "plain",
                ComponentValue::String(product(old, false).to_string()),
            ),
        ])),
    );
    leaf.components.insert(
        component("Container"),
        record(ComponentValue::map([(
            "payload",
            ComponentValue::map([("source", asset(&product(old, true)))]),
        )])),
    );
    let overlay = request.sources[0].document.entities[0]
        .prefab
        .as_mut()
        .unwrap()
        .overrides
        .as_mut()
        .unwrap();
    let PrefabComponentIntent::Properties { claims, .. } = &mut overlay.components[1] else {
        panic!("asset property");
    };
    claims[0].intent = PrefabPropertyIntent::Set(asset(&product(old, false)));
    let originals: Vec<_> = request
        .sources
        .iter()
        .map(|source| source.document.clone())
        .collect();
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    assert!(
        request
            .sources
            .iter()
            .zip(originals)
            .all(|(source, original)| source.document == original)
    );
    let leaf = prepared.sources()[2]
        .document
        .entities
        .iter()
        .find(|entity| entity.id == id("a"))
        .unwrap();
    assert_eq!(
        leaf.components[&component("Assets")].value.get("asset"),
        Some(&asset(&product(new, false)))
    );
    assert_eq!(
        leaf.components[&component("Assets")].value.get("plain"),
        Some(&ComponentValue::String(product(old, false).to_string()))
    );
    assert_eq!(
        leaf.components[&component("Container")]
            .value
            .get("payload")
            .unwrap()
            .get("source"),
        Some(&asset(&product(new, true)))
    );
    let overlay = prepared.sources()[0].document.entities[0]
        .prefab
        .as_ref()
        .unwrap()
        .overrides
        .as_ref()
        .unwrap();
    let PrefabComponentIntent::Properties { target, claims, .. } = &overlay.components[1] else {
        panic!("asset property");
    };
    assert_eq!(
        claims[0].intent,
        PrefabPropertyIntent::Set(asset(&product(new, false)))
    );
    assert_eq!(
        target.through[0].expected_base,
        SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab)
    );
}

#[test]
fn product_copy_accepts_only_explicit_enclosing_source_mappings_with_stable_destinations() {
    let old: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    for stable_origin in [false, true] {
        let original = if stable_origin {
            SourceDocumentKey::stable_asset(old, SourceDocumentKind::Prefab)
        } else {
            key("leaf")
        };
        let aliases = if stable_origin {
            vec![reference("leaf")]
        } else {
            vec![product(old, true)]
        };
        let source = PrefabCopySource::new(
            original,
            SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab),
            aliases,
            AssetRef::StableId(new),
            PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf"))
                .with_component(
                    component("Assets"),
                    record(ComponentValue::map([
                        ("asset", asset(&product(old, false))),
                        ("plain", ComponentValue::String("retained".into())),
                    ])),
                )]),
        );
        let prepared = PrefabCopyRequest::new(vec![source])
            .prepare(&basis(), PrefabCopyLimits::default())
            .unwrap();
        assert_eq!(
            prepared.sources()[0].document.entities[0].components[&component("Assets")]
                .value
                .get("asset"),
            Some(&asset(&product(new, false)))
        );
    }
    let source = PrefabCopySource::new(
        key("leaf"),
        key("copied-leaf"),
        vec![AssetRef::StableId(old)],
        reference("copied-leaf"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(id("leaf")).with_component(
            component("Assets"),
            record(ComponentValue::map([
                ("asset", asset(&product(old, false))),
                ("plain", ComponentValue::String("retained".into())),
            ])),
        )]),
    );
    assert!(matches!(
        refusals::prepare_error(&PrefabCopyRequest::new(vec![source])),
        PrefabCopyError::ProductSourceIdentityUnavailable { .. }
    ));
}

#[test]
fn complete_product_copy_alias_precedes_enclosing_source_alias() {
    let old: StableAssetId = "00000000-0000-4000-8000-000000000011".parse().unwrap();
    let new: StableAssetId = "00000000-0000-4000-8000-000000000022".parse().unwrap();
    let other: StableAssetId = "00000000-0000-4000-8000-000000000044".parse().unwrap();
    let payload = record(ComponentValue::map([
        ("asset", asset(&product(old, true))),
        ("plain", ComponentValue::String("retained".into())),
    ]));
    let request = PrefabCopyRequest::new(vec![
        PrefabCopySource::new(
            key("explicit"),
            SourceDocumentKey::stable_asset(new, SourceDocumentKind::Prefab),
            vec![product(old, true)],
            product(new, true),
            PrefabDocument::from_ordered_roots([
                SceneEntityRecord::new(id("leaf")).with_component(component("Assets"), payload)
            ]),
        ),
        PrefabCopySource::new(
            key("enclosing"),
            SourceDocumentKey::stable_asset(other, SourceDocumentKind::Prefab),
            vec![AssetRef::StableId(old)],
            AssetRef::StableId(other),
            PrefabDocument::default(),
        ),
    ]);
    let prepared = request
        .prepare(&basis(), PrefabCopyLimits::default())
        .unwrap();
    assert_eq!(
        prepared.sources()[0].document.entities[0].components[&component("Assets")]
            .value
            .get("asset"),
        Some(&asset(&product(new, true)))
    );
}
