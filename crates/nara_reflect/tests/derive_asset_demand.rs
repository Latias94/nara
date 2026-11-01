use std::collections::BTreeMap;

use nara_asset::{AssetRef, AssetReferenceDemand};
use nara_ecs::{Component, World};
use nara_reflect::{
    ComponentRegistryBuilder, ComponentValue, ComponentValueShape, PersistentComponent,
    PersistentComponentProvider, collect_declared_asset_references_from_schema,
    validate_component_value,
};

type AssetAlias = AssetRef;
type OptionalAssets = Option<Vec<AssetAlias>>;
type AssetGroups = BTreeMap<String, Vec<Option<AssetAlias>>>;

trait ReferenceTypes {
    type Reference;
}

#[derive(Component, PersistentComponent, Debug, Clone, PartialEq)]
#[nara(
    id = "nara.test.AssetDemandProbe",
    version = 1,
    alias = "Asset demand probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit, asset_ref)
)]
struct AssetDemandProbe {
    #[nara(id = "future", alias = "Future", asset_demand = deferred)]
    future: Option<Vec<AssetRef>>,
    #[nara(id = "alias", alias = "Alias", asset_demand = deferred)]
    aliased: OptionalAssets,
    #[nara(id = "groups", alias = "Groups", asset_demand = deferred)]
    groups: AssetGroups,
    #[nara(id = "associated", alias = "Associated", asset_demand = deferred)]
    associated: <Self as ReferenceTypes>::Reference,
    #[nara(id = "immediate", alias = "Immediate", asset_demand = immediate)]
    immediate: AssetRef,
    #[nara(id = "implicit", alias = "Implicit")]
    implicit: OptionalAssets,
}

impl ReferenceTypes for AssetDemandProbe {
    type Reference = AssetAlias;
}

#[test]
fn field_demand_overrides_recursive_shapes_including_aliases_and_associated_types() {
    use AssetReferenceDemand::{Deferred, Immediate};

    let schema = AssetDemandProbe::persistent_component_schema();
    for (id, expected) in [
        (
            "future",
            ComponentValueShape::optional(ComponentValueShape::list(
                ComponentValueShape::AssetRef { demand: Deferred },
            )),
        ),
        (
            "alias",
            ComponentValueShape::optional(ComponentValueShape::list(
                ComponentValueShape::AssetRef { demand: Deferred },
            )),
        ),
        (
            "groups",
            ComponentValueShape::map(ComponentValueShape::list(ComponentValueShape::optional(
                ComponentValueShape::AssetRef { demand: Deferred },
            ))),
        ),
        (
            "associated",
            ComponentValueShape::AssetRef { demand: Deferred },
        ),
        (
            "immediate",
            ComponentValueShape::AssetRef { demand: Immediate },
        ),
        (
            "implicit",
            ComponentValueShape::optional(ComponentValueShape::list(
                ComponentValueShape::AssetRef { demand: Immediate },
            )),
        ),
    ] {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id().as_str() == id)
            .unwrap();
        assert_eq!(field.shape(), &expected, "field {id}");
    }
}

#[test]
fn demand_survives_native_round_trips_with_nested_empty_and_absent_values() {
    let mut registry = ComponentRegistryBuilder::new();
    registry
        .register_persistent_component::<AssetDemandProbe>()
        .unwrap();
    let snapshot = registry.freeze().unwrap();
    let schema = AssetDemandProbe::persistent_component_schema();
    let shared = AssetRef::path("scenes/shared.scene").unwrap();
    let other = AssetRef::path("scenes/later.scene").unwrap();

    for (links, groups, expected_reference_count) in [
        (None, BTreeMap::new(), 3),
        (
            Some(Vec::new()),
            BTreeMap::from([
                ("empty".to_owned(), Vec::new()),
                ("absent".to_owned(), vec![None]),
            ]),
            3,
        ),
        (
            Some(vec![shared.clone(), other.clone()]),
            BTreeMap::from([(
                "next".to_owned(),
                vec![Some(shared.clone()), None, Some(other.clone())],
            )]),
            9,
        ),
    ] {
        let expected = AssetDemandProbe {
            future: links.clone(),
            aliased: links,
            groups,
            associated: shared.clone(),
            immediate: shared.clone(),
            implicit: Some(vec![shared.clone()]),
        };
        let mut source = World::new();
        let source_entity = source.spawn(expected.clone()).id();
        let encoded = snapshot
            .encode_component(schema.id(), &source, source_entity)
            .unwrap()
            .unwrap();
        validate_component_value(&schema, &encoded).unwrap();
        assert_eq!(
            encoded.field("future").unwrap(),
            encoded.field("alias").unwrap()
        );
        match &expected.future {
            None => assert_eq!(encoded.field("future").unwrap(), &ComponentValue::Null),
            Some(links) if links.is_empty() => assert_eq!(
                encoded.field("future").unwrap(),
                &ComponentValue::List(Vec::new())
            ),
            Some(_) => {}
        }

        let references =
            collect_declared_asset_references_from_schema(&schema, schema.version(), &encoded)
                .unwrap();
        assert_eq!(references.len(), expected_reference_count);
        for reference in &references {
            let expected_demand = match reference.field_id().as_str() {
                "future" | "alias" | "groups" | "associated" => AssetReferenceDemand::Deferred,
                "immediate" | "implicit" => AssetReferenceDemand::Immediate,
                field => panic!("unexpected asset field {field}"),
            };
            assert_eq!(
                reference.demand(),
                expected_demand,
                "{:?}",
                reference.path()
            );
            assert!(reference.asset_ref() == &shared || reference.asset_ref() == &other);
        }
        assert_eq!(
            references
                .iter()
                .filter(|reference| reference.demand() == AssetReferenceDemand::Immediate)
                .count(),
            2
        );

        let mut target = World::new();
        let target_entity = target.spawn_empty().id();
        snapshot
            .preflight_component(schema.id(), &encoded)
            .unwrap()
            .apply(&mut target, target_entity)
            .unwrap();
        assert_eq!(
            target.get::<AssetDemandProbe>(target_entity),
            Some(&expected)
        );
    }
}
