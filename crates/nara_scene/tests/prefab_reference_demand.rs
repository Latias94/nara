//! Actual Prefab composition preserves demand through defaults, property pins and identity writes.

use std::{convert::Infallible, sync::Arc};

use nara_asset::{
    AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetReferenceDemand, AssetSourceKind,
    AssetSourcePath, ImportedProductId, ProjectAssetDatabase, StableAssetId,
};
use nara_reflect::{
    __private::collect_declared_effective_asset_references_with_admission, ComponentCapability,
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentPropertyPath,
    ComponentPropertyStep, ComponentRegistry, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentShapeMember, ComponentTypeId,
    ComponentValue, ComponentValueShape, DeclaredAssetReference, PersistentComponentSchemaOwnerId,
    encode_asset_reference_value, replace_component_property, rewrite_declared_asset_references,
};
use nara_scene::{
    ExpandedSceneCandidate, PrefabComponentIntent, PrefabDocument, PrefabEntityTarget,
    PrefabInstance, PrefabOverlay, PrefabPropertyClaim, PrefabPropertyIntent, ResolvedPrefabSource,
    SceneComponentBindings, SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
    SourceDocumentKey, SourceDocumentKind,
};

const SHARED: u64 = 1;
const ATLAS: u64 = 2;
const DISCARDED_BASE: u64 = 3;
const EXPLICIT_DEFAULT: u64 = 4;
const REWRITTEN: u64 = 5;

fn component() -> ComponentTypeId {
    ComponentTypeId::new("consumer.prefab.ReferenceDemand")
}

fn owner_id() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("consumer.prefab-reference-demand").unwrap()
}

fn entity(name: &str) -> SceneEntityId {
    SceneEntityId::new(name).unwrap()
}

fn product(value: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("73410000-0000-4000-8000-{value:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn asset(value: u64) -> ComponentValue {
    encode_asset_reference_value(&AssetRef::Product(product(value)))
}

fn source_key(name: &str, kind: SourceDocumentKind) -> SourceDocumentKey {
    SourceDocumentKey::named(
        "73410000-0000-4000-8000-000000000099".parse().unwrap(),
        name,
        kind,
    )
    .unwrap()
}

fn prefab_key() -> SourceDocumentKey {
    source_key("reference-demand.base", SourceDocumentKind::Prefab)
}

fn prefab_request() -> AssetRef {
    AssetRef::path("prefabs/reference-demand.json").unwrap()
}

fn future_property() -> ComponentPropertyPath {
    ComponentPropertyPath::new(ComponentFieldId::new("links")).with_steps([
        ComponentPropertyStep::Member(ComponentFieldId::new("future")),
        ComponentPropertyStep::MapEntry("door".into()),
    ])
}

fn schema() -> ComponentSchema {
    let mixed = ComponentValueShape::record_members([
        ComponentShapeMember::new(
            "wire_current",
            ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
        )
        .with_id(ComponentFieldId::new("current")),
        ComponentShapeMember::new(
            "wire_future",
            ComponentValueShape::map(ComponentValueShape::optional(
                ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
            )),
        )
        .with_id(ComponentFieldId::new("future")),
    ]);
    ComponentSchema::new(component(), "Reference demand", ComponentSchemaVersion::ONE)
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([
            ComponentFieldSchema::required(
                ComponentFieldId::new("links"),
                "Links",
                ComponentFieldPath::from_fields(["wire_links"]),
                mixed,
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
            ComponentFieldSchema::optional_with_default(
                ComponentFieldId::new("fallback"),
                "Immediate fallback",
                ComponentFieldPath::from_fields(["wire_fallback"]),
                ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
                asset(SHARED),
            )
            .with_capabilities(ComponentCapability::SCENE_AUTHORING)
            .with_capability(ComponentCapability::AssetRef),
        ])
}

struct Fixture {
    owner: ComponentSchemaOwnerSnapshot,
    bindings: SceneComponentBindings,
    source: Arc<PrefabDocument>,
    assets: ProjectAssetDatabase,
}

impl Fixture {
    fn new(explicit_default: bool) -> Self {
        let owner = ComponentSchemaOwnerSnapshot::admit(
            owner_id(),
            ComponentSchemaCatalog {
                components: vec![schema()],
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let mut native = ComponentRegistry::new();
        native.freeze().unwrap();
        let bindings =
            SceneComponentBindings::new(native.snapshot().unwrap(), [owner.clone()]).unwrap();
        let mut value = ComponentValue::map([(
            "wire_links",
            ComponentValue::map([
                ("wire_current", asset(ATLAS)),
                (
                    "wire_future",
                    ComponentValue::map([
                        ("door", asset(DISCARDED_BASE)),
                        ("empty", ComponentValue::Null),
                    ]),
                ),
            ]),
        )]);
        if explicit_default {
            let ComponentValue::Map(fields) = &mut value else {
                unreachable!()
            };
            fields.insert("wire_fallback".into(), asset(EXPLICIT_DEFAULT));
        }
        let source = Arc::new(PrefabDocument::from_ordered_roots([
            SceneEntityRecord::new(entity("entry")).with_component(
                component(),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
                    .with_owner(owner_id()),
            ),
        ]));
        let mut assets = ProjectAssetDatabase::default();
        for value in [SHARED, ATLAS, REWRITTEN] {
            assets
                .insert(AssetRecord::new(
                    product(value).source(),
                    AssetSourcePath::new(format!("textures/{value}.png")).unwrap(),
                    AssetSourceKind::Image,
                ))
                .unwrap();
        }
        // The discarded inherited target and superseded explicit default have no asset records.
        // Only the selected production result can pass actual reference resolution.
        Self {
            owner,
            bindings,
            source,
            assets,
        }
    }

    fn schema(&self) -> &ComponentSchema {
        let selected = self.bindings.data_owner(&component()).unwrap();
        assert!(selected.ptr_eq(&self.owner));
        let schema = selected.schema(&component()).unwrap();
        assert!(std::ptr::eq(
            schema,
            self.bindings.schema(&component()).unwrap()
        ));
        schema
    }

    fn expand(&self, restore_default: bool) -> ExpandedSceneCandidate {
        let mut claims = vec![PrefabPropertyClaim {
            path: future_property(),
            intent: PrefabPropertyIntent::Set(asset(SHARED)),
            bindings: vec![],
        }];
        if restore_default {
            claims.push(PrefabPropertyClaim {
                path: ComponentPropertyPath::new(ComponentFieldId::new("fallback")),
                intent: PrefabPropertyIntent::Absent,
                bindings: vec![],
            });
        }
        let mut overlay = PrefabOverlay::new(prefab_key());
        overlay.components.push(PrefabComponentIntent::Properties {
            target: PrefabEntityTarget::source(entity("entry")),
            component: component(),
            version: ComponentSchemaVersion::ONE,
            claims,
        });
        let mut anchor = SceneEntityRecord::new(entity("anchor"));
        anchor.prefab = Some(PrefabInstance {
            source: prefab_request(),
            overrides: Some(overlay),
        });
        let document = SceneDocument::from_ordered_roots([anchor]);
        let before = document.clone();
        let original_source = self.source.as_ref().clone();
        let mut loads = 0;
        let report = self
            .bindings
            .try_expand_prefabs_with_loader(
                &document,
                Default::default(),
                Some(self.assets.resolver(&AssetPathScope::ROOT)),
                |requested, _| {
                    assert_eq!(requested, &prefab_request());
                    loads += 1;
                    let source =
                        ResolvedPrefabSource::bound(Arc::clone(&self.source), prefab_key());
                    assert!(std::ptr::eq(source.document(), self.source.as_ref()));
                    Ok::<_, Infallible>(Some(source))
                },
            )
            .unwrap();
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        assert_eq!(loads, 1);
        assert_eq!(document, before);
        assert_eq!(self.source.as_ref(), &original_source);
        let candidate = report.candidate.unwrap().bind_source(source_key(
            "reference-demand.scene",
            SourceDocumentKind::Scene,
        ));
        assert_eq!(candidate.document().entities.len(), 2);
        assert!(
            candidate
                .document()
                .entities
                .iter()
                .all(|entity| entity.prefab.is_none())
        );
        candidate
    }
}

fn expanded_record(candidate: &ExpandedSceneCandidate) -> &SceneComponentRecord {
    let mut records = candidate
        .document()
        .entities
        .iter()
        .filter_map(|entity| entity.components.get(&component()));
    let record = records.next().unwrap();
    assert!(records.next().is_none());
    assert_eq!(record.owner.as_ref(), Some(&owner_id()));
    record
}

fn inventory(schema: &ComponentSchema, value: &ComponentValue) -> Vec<DeclaredAssetReference> {
    collect_declared_effective_asset_references_with_admission(
        schema,
        ComponentSchemaVersion::ONE,
        value,
        Default::default(),
        |_, _, _| Ok::<_, Infallible>(()),
    )
    .unwrap()
}

fn assert_inventory(
    schema: &ComponentSchema,
    value: &ComponentValue,
    deferred: u64,
    fallback: u64,
) -> Vec<DeclaredAssetReference> {
    // Assert actual compiler materialization before using the already-effective collector.
    assert_eq!(value.get("wire_fallback"), Some(&asset(fallback)));
    let references = inventory(schema, value);
    assert_eq!(references.len(), 3);
    for (path, field, target, demand) in [
        (
            ComponentFieldPath::from_fields(["wire_links", "wire_current"]),
            "links",
            ATLAS,
            AssetReferenceDemand::Immediate,
        ),
        (
            ComponentFieldPath::from_fields(["wire_links", "wire_future", "door"]),
            "links",
            deferred,
            AssetReferenceDemand::Deferred,
        ),
        (
            ComponentFieldPath::from_fields(["wire_fallback"]),
            "fallback",
            fallback,
            AssetReferenceDemand::Immediate,
        ),
    ] {
        let reference = references
            .iter()
            .find(|reference| reference.path() == &path)
            .unwrap();
        assert_eq!(reference.field_id().as_str(), field);
        assert_eq!(reference.asset_ref(), &AssetRef::Product(product(target)));
        assert_eq!(reference.demand(), demand);
    }
    assert!(references.iter().all(|reference| {
        reference.asset_ref() != &AssetRef::Product(product(DISCARDED_BASE))
            && reference.asset_ref() != &AssetRef::Product(product(EXPLICIT_DEFAULT))
    }));
    references
}

#[test]
fn prefab_property_pin_and_materialized_default_keep_distinct_demands_for_one_product() {
    let fixture = Fixture::new(false);
    let original = &fixture.source.entities[0].components[&component()];
    assert!(original.value.get("wire_fallback").is_none());
    let candidate = fixture.expand(false);
    let record = expanded_record(&candidate);
    let schema = fixture.schema();
    let references = assert_inventory(schema, &record.value, SHARED, SHARED);
    assert_eq!(
        references
            .iter()
            .filter(|reference| { reference.asset_ref() == &AssetRef::Product(product(SHARED)) })
            .count(),
        2
    );

    let mut rewritten = record.value.clone();
    rewrite_declared_asset_references(&mut rewritten, &references, |reference| {
        Ok::<_, Infallible>(
            if reference.asset_ref() == &AssetRef::Product(product(SHARED)) {
                AssetRef::Product(product(REWRITTEN))
            } else {
                reference.asset_ref().clone()
            },
        )
    })
    .unwrap();
    let after = assert_inventory(schema, &rewritten, REWRITTEN, REWRITTEN);
    assert_eq!(
        references
            .iter()
            .map(|reference| (reference.path(), reference.demand()))
            .collect::<Vec<_>>(),
        after
            .iter()
            .map(|reference| (reference.path(), reference.demand()))
            .collect::<Vec<_>>()
    );
    assert_inventory(schema, &record.value, SHARED, SHARED);
}

#[test]
fn prefab_absent_restores_immediate_default_and_property_set_keeps_actual_deferred_leaf() {
    let fixture = Fixture::new(true);
    assert_eq!(
        fixture.source.entities[0].components[&component()]
            .value
            .get("wire_fallback"),
        Some(&asset(EXPLICIT_DEFAULT))
    );
    let candidate = fixture.expand(true);
    let record = expanded_record(&candidate);
    let schema = fixture.schema();
    assert_inventory(schema, &record.value, SHARED, SHARED);

    let mut edited = record.value.clone();
    let previous = replace_component_property(
        schema,
        record.version,
        &mut edited,
        &future_property(),
        asset(REWRITTEN),
    )
    .unwrap();
    assert_eq!(previous, asset(SHARED));
    assert_inventory(schema, &edited, REWRITTEN, SHARED);
    assert_inventory(schema, &record.value, SHARED, SHARED);
}
