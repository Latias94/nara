mod crate_override {
    use nara_ecs::Component;
    use nara_reflect::Schema;

    #[derive(Component, Schema)]
    #[schema(
        crate = "nara_reflect",
        id = "nara.test.CrateOverride",
        version = 1,
        alias = "Crate override",
        component_capabilities(scene),
        field_capabilities(scene)
    )]
    struct CrateOverride {
        #[schema(id = "value", alias = "Value")]
        value: i64,
    }

    #[test]
    fn explicit_support_crate_path_compiles() {
        let _: Option<CrateOverride> = None;
    }
}

mod nested_aliases {
    use std::collections::BTreeMap;

    use nara_asset::AssetRef;
    use nara_core::EntityUuid;
    use nara_ecs::Component;
    use nara_reflect::{ComponentSchemaCatalog, Schema};

    type EntityTargets = Vec<Option<EntityUuid>>;
    type AssetGroups = BTreeMap<String, Vec<Option<AssetRef>>>;

    #[derive(Component, Schema)]
    #[schema(
        id = "nara.test.NestedAliases",
        version = 1,
        alias = "Nested aliases",
        component_capabilities(scene),
        field_capabilities(scene)
    )]
    struct NestedAliases {
        #[schema(id = "targets", alias = "Targets", capabilities(scene, entity_ref))]
        targets: EntityTargets,
        #[schema(id = "assets", alias = "Assets", capabilities(scene, asset_ref))]
        assets: AssetGroups,
        #[schema(id = "counts", alias = "Counts")]
        counts: std::vec::Vec<std::option::Option<u16>>,
    }

    #[test]
    fn aliased_nested_references_admit_schema() {
        ComponentSchemaCatalog::from_components([NestedAliases::type_schema().unwrap()]).unwrap();
    }
}

mod typed_float_literals {
    use nara_ecs::Component;
    use nara_reflect::{ComponentSchemaCatalog, Schema};

    mod wide_float_alias {
        #[allow(non_camel_case_types)]
        pub type f32 = std::primitive::f64;
    }

    trait ScalarType {
        type Scalar;
    }

    #[derive(Component, Schema)]
    #[schema(
        id = "nara.test.TypedFloatLiterals",
        version = 1,
        alias = "Typed float literals",
        component_capabilities(scene),
        field_capabilities(scene),
        authoring_initial
    )]
    struct TypedFloatLiterals {
        #[schema(
            id = "wide",
            alias = "Wide",
            initial = 1e100,
            range(min_inclusive = 0.0, max_inclusive = 1e101)
        )]
        wide: wide_float_alias::f32,
        #[schema(id = "associated", alias = "Associated", initial = 1.5_f32,
            range(min_inclusive = -2.0_f32, max_inclusive = 2.0_f32))]
        associated: <Self as ScalarType>::Scalar,
    }

    impl ScalarType for TypedFloatLiterals {
        type Scalar = f32;
    }

    #[test]
    fn actual_wide_alias_and_self_associated_type_admit_schema() {
        let catalog =
            ComponentSchemaCatalog::from_components([TypedFloatLiterals::type_schema().unwrap()])
                .unwrap();
        let schema = &catalog.components()[0];
        let initial = schema.authoring_initial_value().unwrap();
        assert_eq!(initial.field("wide").unwrap().as_f64(), Some(1e100));
        assert_eq!(initial.field("associated").unwrap().as_f64(), Some(1.5));
    }
}
