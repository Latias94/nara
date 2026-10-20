mod crate_override {
    use nara_ecs::Component;
    use nara_reflect::PersistentComponent;

    #[derive(Component, PersistentComponent)]
    #[nara(
        crate = "nara_reflect",
        id = "nara.test.CrateOverride",
        version = 1,
        alias = "Crate override",
        component_capabilities(scene),
        field_capabilities(scene)
    )]
    struct CrateOverride {
        #[nara(id = "value", alias = "Value")]
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
    use nara_ecs::Component;
    use nara_identity::EntityReference;
    use nara_reflect::{ComponentRegistry, PersistentComponent};

    type EntityTargets = Vec<Option<EntityReference>>;
    type AssetGroups = BTreeMap<String, Vec<Option<AssetRef>>>;

    #[derive(Component, PersistentComponent)]
    #[nara(
        id = "nara.test.NestedAliases",
        version = 1,
        alias = "Nested aliases",
        component_capabilities(scene),
        field_capabilities(scene)
    )]
    struct NestedAliases {
        #[nara(id = "targets", alias = "Targets", capabilities(scene, entity_ref))]
        targets: EntityTargets,
        #[nara(id = "assets", alias = "Assets", capabilities(scene, asset_ref))]
        assets: AssetGroups,
        #[nara(id = "counts", alias = "Counts")]
        counts: std::vec::Vec<std::option::Option<u16>>,
    }

    #[test]
    fn aliased_nested_references_register_and_freeze() {
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component::<NestedAliases>()
            .unwrap();
        registry.freeze().unwrap();
    }
}

mod typed_float_literals {
    use nara_ecs::Component;
    use nara_reflect::{ComponentRegistry, PersistentComponent, PersistentComponentProvider};

    mod wide_float_alias {
        #[allow(non_camel_case_types)]
        pub type f32 = std::primitive::f64;
    }

    trait ScalarType {
        type Scalar;
    }

    #[derive(Component, PersistentComponent)]
    #[nara(
        id = "nara.test.TypedFloatLiterals",
        version = 1,
        alias = "Typed float literals",
        component_capabilities(scene),
        field_capabilities(scene),
        authoring_initial
    )]
    struct TypedFloatLiterals {
        #[nara(
            id = "wide",
            alias = "Wide",
            initial = 1e100,
            range(min_inclusive = 0.0, max_inclusive = 1e101)
        )]
        wide: wide_float_alias::f32,
        #[nara(id = "associated", alias = "Associated", initial = 1.5_f32,
            range(min_inclusive = -2.0_f32, max_inclusive = 2.0_f32))]
        associated: <Self as ScalarType>::Scalar,
    }

    impl ScalarType for TypedFloatLiterals {
        type Scalar = f32;
    }

    #[test]
    fn actual_wide_alias_and_self_associated_type_register_and_freeze() {
        let mut registry = ComponentRegistry::new();
        registry
            .register_persistent_component::<TypedFloatLiterals>()
            .unwrap();
        registry.freeze().unwrap();
        let schema = TypedFloatLiterals::persistent_component_schema();
        let initial = schema.authoring_initial_value().unwrap();
        assert_eq!(initial.field("wide").unwrap().as_f64(), Some(1e100));
        assert_eq!(initial.field("associated").unwrap().as_f64(), Some(1.5));
    }
}
