use std::collections::BTreeMap;

use nara_asset::{AssetRef, AssetReferenceDemand};
use nara_ecs::Component;
use nara_reflect::{
    ComponentSchemaCatalog, ComponentValue, ComponentValueShape, Schema, validate_component_value,
};

type AssetAlias = AssetRef;
type OptionalAssets = Option<Vec<AssetAlias>>;
type AssetGroups = BTreeMap<String, Vec<Option<AssetAlias>>>;

trait ReferenceTypes {
    type Reference;
}

#[derive(Component, Schema, Debug, Clone, PartialEq)]
#[schema(
    id = "nara.test.AssetDemandProbe",
    version = 1,
    alias = "Asset demand probe",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit, asset_ref)
)]
struct AssetDemandProbe {
    #[schema(id = "future", alias = "Future", asset_demand = deferred)]
    future: Option<Vec<AssetRef>>,
    #[schema(id = "alias", alias = "Alias", asset_demand = deferred)]
    aliased: OptionalAssets,
    #[schema(id = "groups", alias = "Groups", asset_demand = deferred)]
    groups: AssetGroups,
    #[schema(id = "associated", alias = "Associated", asset_demand = deferred)]
    associated: <Self as ReferenceTypes>::Reference,
    #[schema(id = "immediate", alias = "Immediate", asset_demand = immediate)]
    immediate: AssetRef,
    #[schema(id = "implicit", alias = "Implicit")]
    implicit: OptionalAssets,
}

impl ReferenceTypes for AssetDemandProbe {
    type Reference = AssetAlias;
}

#[test]
fn field_demand_overrides_recursive_shapes_including_aliases_and_associated_types() {
    use AssetReferenceDemand::{Deferred, Immediate};

    let schema = AssetDemandProbe::type_schema().unwrap();
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
fn asset_codecs_round_trip_nested_empty_and_absent_values() {
    let catalog =
        ComponentSchemaCatalog::from_components([AssetDemandProbe::type_schema().unwrap()])
            .unwrap();
    let schema = &catalog.components()[0];
    let shared = AssetRef::path("scenes/shared.scene").unwrap();
    let other = AssetRef::path("scenes/later.scene").unwrap();

    for (links, groups) in [
        (None, BTreeMap::new()),
        (
            Some(Vec::new()),
            BTreeMap::from([
                ("empty".to_owned(), Vec::new()),
                ("absent".to_owned(), vec![None]),
            ]),
        ),
        (
            Some(vec![shared.clone(), other.clone()]),
            BTreeMap::from([(
                "next".to_owned(),
                vec![Some(shared.clone()), None, Some(other.clone())],
            )]),
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
        let encoded = expected.encode_component().unwrap();
        validate_component_value(schema, &encoded).unwrap();
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

        assert_eq!(
            AssetDemandProbe::decode_component((&encoded).into()).unwrap(),
            expected,
        );
    }
}
