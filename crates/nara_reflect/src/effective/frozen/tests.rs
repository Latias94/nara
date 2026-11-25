use super::*;
use crate::{
    ComponentCapability, ComponentFieldPath, ComponentFieldSchema, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueKind, EntityReference,
    effective::{
        DefaultPlan, ValueCursor, component_defaults_are_explicit_with_admission,
        effective_component_value_with_default_overrides,
        validate_native_component_value_references_with_admission,
        validate_native_component_value_references_with_limits,
        validate_value_materialization_budget,
        validate_value_materialization_budget_with_admission,
    },
};
use nara_core::source::{AssetSourcePath, SourceDocumentKey, SourceDocumentKind};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, PersistentRuntimeId,
    PersistentRuntimeNamespaceId, PersistentRuntimeReference, PrefabEntityOrigin,
    PrefabInstanceHop, ReferenceInputId, SceneEntityId,
};

fn limits(nodes: usize, bytes: usize, depth: usize) -> ComponentValueMaterializationLimits {
    ComponentValueMaterializationLimits::new(
        ItemLimit::new(nodes.max(1)).unwrap(),
        ByteLimit::new(bytes.max(1)).unwrap(),
        DepthLimit::new(depth).unwrap(),
    )
    .remaining(usize::from(nodes == 0), usize::from(bytes == 0))
}

fn field(id: &str, path: ComponentFieldPath, default: ComponentValue) -> ComponentFieldSchema {
    let mut field =
        ComponentFieldSchema::required(ComponentFieldId::new(id), id, path, default.kind());
    field.default_value = Some(default);
    field
}

fn schema(fields: impl IntoIterator<Item = ComponentFieldSchema>) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("test.FrozenDefaults"),
        "Frozen defaults",
        ComponentSchemaVersion::ONE,
    )
    .with_fields(fields)
}

fn row_path(index: u32, name: &str) -> ComponentFieldPath {
    ComponentFieldPath::new([
        ComponentFieldPathSegment::field("rows"),
        ComponentFieldPathSegment::index(index),
        ComponentFieldPathSegment::field(name),
    ])
}

#[test]
fn nested_maps_lists_nulls_and_overrides_match_the_original_materializer() {
    let schema = schema([
        field(
            "first",
            row_path(0, "label"),
            ComponentValue::String("canonical".into()),
        ),
        field("second", row_path(1, "enabled"), ComponentValue::Bool(true)),
        field(
            "third",
            ComponentFieldPath::new([
                ComponentFieldPathSegment::field("rows"),
                ComponentFieldPathSegment::index(2),
            ]),
            ComponentValue::map([(
                "nested",
                ComponentValue::List(vec![ComponentValue::Null, ComponentValue::U64(7)]),
            )]),
        ),
        field(
            "nullable",
            ComponentFieldPath::from_fields(["nullable"]),
            ComponentValue::String("hidden".into()),
        ),
        field(
            "mode",
            ComponentFieldPath::from_fields(["settings", "mode"]),
            ComponentValue::String("auto".into()),
        ),
        field(
            "suffix",
            ComponentFieldPath::new([
                ComponentFieldPathSegment::field("created"),
                ComponentFieldPathSegment::field("items"),
                ComponentFieldPathSegment::index(0),
                ComponentFieldPathSegment::field("value"),
            ]),
            ComponentValue::I64(-9),
        ),
    ]);
    let source = ComponentValue::map([
        (
            "rows",
            ComponentValue::List(vec![ComponentValue::map([(
                "explicit",
                ComponentValue::U64(42),
            )])]),
        ),
        ("nullable", ComponentValue::Null),
        ("z-last", ComponentValue::Bool(false)),
    ]);
    let original = source.clone();
    let overrides = BTreeMap::from([
        (
            ComponentFieldId::new("first"),
            ComponentValue::String("selected".into()),
        ),
        (
            ComponentFieldId::new("nullable"),
            ComponentValue::String("also hidden".into()),
        ),
    ]);
    let limits = ComponentValueMaterializationLimits::default();
    let (expected, cost) =
        effective_component_value_with_default_overrides(&schema, &source, &overrides, limits)
            .unwrap();
    let frozen = freeze_component_defaults(&schema, &source, &overrides, limits)
        .unwrap()
        .unwrap();
    let read = frozen.read(&source);

    assert_eq!(read.to_owned(), *expected);
    assert_eq!(Some(frozen.cost()), cost);
    assert_eq!(read.cost(), expected.cost());
    assert_eq!(read.as_map().unwrap().len(), 5);
    assert_eq!(
        read.as_map()
            .unwrap()
            .iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["created", "nullable", "rows", "settings", "z-last"],
    );
    let rows = read.field("rows").unwrap().as_list().unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows.iter().len(), 3);
    assert_eq!(rows.get(0).unwrap().as_map().unwrap().len(), 2);
    assert_eq!(rows.get(0).unwrap().field_str("label").unwrap(), "selected");
    assert!(rows.get(1).unwrap().field_bool("enabled").unwrap());
    assert!(rows.get(3).is_none());
    assert_eq!(
        read.field("nullable").unwrap().kind(),
        ComponentValueKind::Null,
    );
    assert!(read.as_value().is_none());
    assert!(
        component_defaults_are_explicit_with_admission(&schema, read, |_, _, _| {
            Ok::<(), Infallible>(())
        })
        .unwrap()
    );
    assert!(
        !component_defaults_are_explicit_with_admission(&schema, &source, |_, _, _| {
            Ok::<(), Infallible>(())
        })
        .unwrap()
    );
    assert_eq!(source, original);
}

#[test]
fn inserted_defaults_preserve_the_original_raw_cell_backing() {
    let cells = vec![ComponentValue::U64(7); 16_384];
    let cells_pointer = cells.as_ptr();
    let source = ComponentValue::map([("cells", ComponentValue::List(cells))]);
    let schema = schema([field(
        "visible",
        ComponentFieldPath::from_fields(["settings", "visible"]),
        ComponentValue::Bool(true),
    )]);
    let frozen = freeze_component_defaults(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let read = frozen.read(&source);
    let cells = read.field("cells").unwrap();
    assert!(std::ptr::eq(
        cells.as_value().unwrap(),
        source.get("cells").unwrap(),
    ));
    let ComponentValue::List(raw_cells) = cells.as_value().unwrap() else {
        panic!("unchanged cells retain their original List");
    };
    assert_eq!(raw_cells.as_ptr(), cells_pointer);
    let items = cells.as_list().unwrap();
    assert_eq!(items.len(), 16_384);
    for index in [0, 1, 8_192, 16_383] {
        assert!(std::ptr::eq(
            items.get(index).unwrap().as_value().unwrap(),
            &raw_cells[index],
        ));
    }
    assert_eq!(frozen.cost(), read.cost());
    assert!(
        read.field("settings")
            .unwrap()
            .field_bool("visible")
            .unwrap(),
    );
    assert!(source.get("settings").is_none());
}

#[test]
fn no_insertion_returns_none_without_a_full_source_walk() {
    let schema = schema([field(
        "value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValue::String("hidden".repeat(16_384)),
    )]);
    let source = ComponentValue::map([
        ("value", ComponentValue::Null),
        (
            "cells",
            ComponentValue::List(vec![ComponentValue::Null; 16_384]),
        ),
    ]);
    let mut work = 0;
    let result = freeze_component_defaults_with_admission(
        &schema,
        &source,
        &BTreeMap::new(),
        limits(0, 0, 1),
        |nodes, _, _| {
            work += nodes;
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    assert!(result.is_none());
    assert_eq!(work, 2);
    assert!(
        freeze_component_defaults(&schema, &source, &BTreeMap::new(), limits(0, 0, 1))
            .unwrap()
            .is_none(),
    );
}

#[test]
fn frozen_values_outlive_schema_and_override_selection() {
    let source = ComponentValue::Map(BTreeMap::new());
    let frozen = {
        let schema = schema([
            field(
                "selected",
                ComponentFieldPath::from_fields(["selected"]),
                ComponentValue::String("canonical".into()),
            ),
            field(
                "fallback",
                ComponentFieldPath::from_fields(["fallback"]),
                ComponentValue::String("fallback".into()),
            ),
        ]);
        let overrides = BTreeMap::from([(
            ComponentFieldId::new("selected"),
            ComponentValue::String("chosen".into()),
        )]);
        let frozen = freeze_component_defaults(
            &schema,
            &source,
            &overrides,
            ComponentValueMaterializationLimits::default(),
        )
        .unwrap()
        .unwrap();
        assert_ne!(
            frozen.read(&source).field_str("selected").unwrap().as_ptr(),
            overrides
                .values()
                .next()
                .unwrap()
                .as_str()
                .unwrap()
                .as_ptr(),
        );
        frozen
    };
    assert_eq!(
        frozen.read(&source).field_str("selected").unwrap(),
        "chosen"
    );
    assert_eq!(
        frozen.read(&source).field_str("fallback").unwrap(),
        "fallback"
    );
}

#[test]
fn frozen_and_original_materialization_refuse_the_same_invalid_plans() {
    let field_path = |index| {
        ComponentFieldPath::new([
            ComponentFieldPathSegment::field("rows"),
            ComponentFieldPathSegment::index(index),
            ComponentFieldPathSegment::field("label"),
        ])
    };
    for (source, fields) in [
        (
            ComponentValue::map([("settings", ComponentValue::Null)]),
            vec![field(
                "bad-parent",
                ComponentFieldPath::from_fields(["settings", "value"]),
                ComponentValue::Bool(true),
            )],
        ),
        (
            ComponentValue::Map(BTreeMap::new()),
            vec![field("sparse", field_path(1), ComponentValue::Null)],
        ),
        (
            ComponentValue::map([("rows", ComponentValue::List(vec![ComponentValue::Null]))]),
            vec![field("sparse", field_path(2), ComponentValue::Null)],
        ),
        (
            ComponentValue::Map(BTreeMap::new()),
            vec![
                field(
                    "first",
                    ComponentFieldPath::from_fields(["a"]),
                    ComponentValue::Bool(true),
                ),
                field("late-sparse", field_path(2), ComponentValue::Null),
            ],
        ),
        (
            ComponentValue::Map(BTreeMap::new()),
            vec![
                field(
                    "parent",
                    ComponentFieldPath::from_fields(["parent"]),
                    ComponentValue::Null,
                ),
                field(
                    "child",
                    ComponentFieldPath::from_fields(["parent", "child"]),
                    ComponentValue::Null,
                ),
            ],
        ),
        (
            ComponentValue::Map(BTreeMap::new()),
            vec![
                field(
                    "map",
                    ComponentFieldPath::from_fields(["mixed", "value"]),
                    ComponentValue::Null,
                ),
                field(
                    "list",
                    ComponentFieldPath::new([
                        ComponentFieldPathSegment::field("mixed"),
                        ComponentFieldPathSegment::index(0),
                    ]),
                    ComponentValue::Null,
                ),
            ],
        ),
    ] {
        let schema = schema(fields);
        let original = source.clone();
        let limits = ComponentValueMaterializationLimits::default();
        let expected = effective_component_value_with_default_overrides(
            &schema,
            &source,
            &BTreeMap::new(),
            limits,
        )
        .unwrap_err();
        let actual =
            freeze_component_defaults(&schema, &source, &BTreeMap::new(), limits).unwrap_err();
        assert_eq!(actual, expected);
        assert_eq!(source, original);
    }
}

#[test]
fn exact_and_one_short_limits_cover_the_full_effective_value() {
    let schema = schema([
        field(
            "label",
            row_path(0, "label"),
            ComponentValue::String("canonical".into()),
        ),
        field(
            "enabled",
            row_path(1, "enabled"),
            ComponentValue::Bool(true),
        ),
        field(
            "mode",
            ComponentFieldPath::from_fields(["settings", "mode"]),
            ComponentValue::String("auto".into()),
        ),
    ]);
    let source = ComponentValue::map([("retained", ComponentValue::String("kept".into()))]);
    let original = source.clone();
    let overrides = BTreeMap::from([(
        ComponentFieldId::new("label"),
        ComponentValue::String("chosen".into()),
    )]);
    let exact = limits(9, 51, 4);
    let (expected, cost) =
        effective_component_value_with_default_overrides(&schema, &source, &overrides, exact)
            .unwrap();
    let frozen = freeze_component_defaults(&schema, &source, &overrides, exact)
        .unwrap()
        .unwrap();
    assert_eq!(Some(frozen.cost()), cost);
    assert_eq!(frozen.read(&source).to_owned(), *expected);
    for limits in [exact, limits(8, 51, 4), limits(9, 50, 4), limits(9, 51, 3)] {
        let expected_result =
            effective_component_value_with_default_overrides(&schema, &source, &overrides, limits);
        match expected_result {
            Ok((_, expected_cost)) => assert_eq!(
                Some(
                    freeze_component_defaults(&schema, &source, &overrides, limits)
                        .unwrap()
                        .unwrap()
                        .cost(),
                ),
                expected_cost,
            ),
            Err(error) => assert_eq!(
                freeze_component_defaults(&schema, &source, &overrides, limits).unwrap_err(),
                error,
            ),
        }
        assert_eq!(
            validate_value_materialization_budget(frozen.read(&source), limits),
            validate_value_materialization_budget(expected.as_ref(), limits),
        );
        assert_eq!(
            validate_native_component_value_references_with_limits(frozen.read(&source), limits),
            validate_native_component_value_references_with_limits(expected.as_ref(), limits),
        );
    }
    assert_eq!(source, original);
}

#[test]
fn list_root_defaults_keep_exact_zero_byte_allowances() {
    let source = ComponentValue::List(Vec::new());
    for default in [
        ComponentValue::Null,
        ComponentValue::String(String::new()),
        ComponentValue::String("x".into()),
    ] {
        let schema = schema([field(
            "value",
            ComponentFieldPath::new([ComponentFieldPathSegment::index(0)]),
            default,
        )]);
        for limits in [limits(2, 0, 2), limits(1, 0, 2), limits(2, 0, 1)] {
            let expected = effective_component_value_with_default_overrides(
                &schema,
                &source,
                &BTreeMap::new(),
                limits,
            );
            let actual = freeze_component_defaults(&schema, &source, &BTreeMap::new(), limits);
            match expected {
                Ok((expected, cost)) => {
                    let frozen = actual.unwrap().unwrap();
                    let read = frozen.read(&source);
                    assert_eq!(read.to_owned(), *expected);
                    assert_eq!(read.as_list().unwrap().len(), 1);
                    assert_eq!(Some(frozen.cost()), cost);
                    assert_eq!(frozen.cost().logical_bytes(), 0);
                    assert_eq!(
                        validate_value_materialization_budget(read, limits),
                        Ok(frozen.cost()),
                    );
                }
                Err(error) => assert_eq!(actual.unwrap_err(), error),
            }
        }
    }
    assert_eq!(source, ComponentValue::List(Vec::new()));
}

#[test]
fn selected_contextual_references_and_opaque_native_rejections_use_the_same_view() {
    let persistent = ComponentValue::EntityReference(EntityReference::Persistent {
        entity: PersistentRuntimeReference::new(
            PersistentRuntimeNamespaceId::new("save").unwrap(),
            PersistentRuntimeId::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
        ),
    });
    let authored = ComponentValue::EntityReference(EntityReference::Authored {
        reference: AuthoredEntityReference::input(
            nara_ecs::identity::ReferenceInputId::new("socket").unwrap(),
        ),
    });
    let schema = schema([field(
        "target",
        ComponentFieldPath::from_fields(["target"]),
        authored.clone(),
    )]);
    let limits = ComponentValueMaterializationLimits::default();
    let overrides = BTreeMap::from([(ComponentFieldId::new("target"), persistent.clone())]);
    for source in [
        ComponentValue::Map(BTreeMap::new()),
        ComponentValue::map([("opaque", ComponentValue::List(vec![authored.clone()]))]),
    ] {
        let (expected, _) =
            effective_component_value_with_default_overrides(&schema, &source, &overrides, limits)
                .unwrap();
        let frozen = freeze_component_defaults(&schema, &source, &overrides, limits)
            .unwrap()
            .unwrap();
        assert_eq!(
            frozen.read(&source).field("target").unwrap().to_owned(),
            persistent,
        );
        assert_eq!(frozen.read(&source).to_owned(), *expected);
        let native =
            validate_native_component_value_references_with_limits(frozen.read(&source), limits);
        assert_eq!(
            native,
            validate_native_component_value_references_with_limits(expected.as_ref(), limits),
        );
        assert_eq!(
            native,
            if source.get("opaque").is_some() {
                Err(ComponentCodecError::UnresolvedAuthoredReference)
            } else {
                Ok(())
            },
        );
        assert_eq!(
            validate_value_materialization_budget(frozen.read(&source), limits),
            Ok(expected.cost()),
        );
    }
    let source = ComponentValue::Map(BTreeMap::new());
    let frozen = freeze_component_defaults(&schema, &source, &BTreeMap::new(), limits)
        .unwrap()
        .unwrap();
    assert_eq!(
        validate_native_component_value_references_with_limits(frozen.read(&source), limits),
        Err(ComponentCodecError::UnresolvedAuthoredReference),
    );
}

#[test]
fn frozen_allocation_requests_cover_only_locators_and_selected_default_data() {
    let default = ComponentValue::map([
        (
            "flags",
            ComponentValue::List(vec![
                ComponentValue::Bool(true),
                ComponentValue::String("hi".into()),
            ]),
        ),
        ("empty", ComponentValue::Map(BTreeMap::new())),
        (
            "local",
            ComponentValue::EntityReference(EntityReference::SceneLocal {
                entity: SceneEntityId::new("socket").unwrap(),
            }),
        ),
    ]);
    let schema = schema([field(
        "config",
        ComponentFieldPath::from_fields(["config"]),
        default,
    )]);
    let source = ComponentValue::map([(
        "cells",
        ComponentValue::List(vec![ComponentValue::U64(3); 16_384]),
    )]);
    let mut observations = Vec::new();
    let frozen = freeze_component_defaults_with_admission(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
        |nodes, bytes, allocated| {
            observations.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap()
    .unwrap();
    let expected = size_of::<FrozenComponentDefaults>()
        + btree_entry_bytes::<String, FrozenDefaultNode>()
        + "config".len()
        + 3 * btree_entry_bytes::<String, ComponentValue>()
        + "flags".len()
        + "empty".len()
        + "local".len()
        + 2 * size_of::<ComponentValue>()
        + "hi".len()
        + "socket".len();
    assert_eq!(frozen.retained_bytes(), expected);
    assert_eq!(
        observations[observations.len() - 3..]
            .iter()
            .map(|(_, _, allocated)| allocated)
            .sum::<usize>(),
        frozen.retained_bytes(),
    );
    let empty_source = ComponentValue::map([("cells", ComponentValue::List(Vec::new()))]);
    let mut empty_work = (0usize, 0usize);
    let empty_frozen = freeze_component_defaults_with_admission(
        &schema,
        &empty_source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
        |nodes, bytes, _| {
            empty_work.0 += nodes;
            empty_work.1 += bytes;
            Ok::<(), Infallible>(())
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(frozen.retained_bytes(), empty_frozen.retained_bytes());
    assert_eq!(
        observations
            .iter()
            .map(|(nodes, _, _)| nodes)
            .sum::<usize>()
            - empty_work.0,
        16_384,
    );
    assert_eq!(
        observations
            .iter()
            .map(|(_, bytes, _)| bytes)
            .sum::<usize>()
            - empty_work.1,
        16_384 * 8,
    );
}

#[test]
fn selected_authored_routes_admit_exact_retained_storage_before_cloning() {
    let universe = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let authored = |endpoint| EntityReference::Authored {
        reference: AuthoredEntityReference {
            through: vec![
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("outer").unwrap()),
                    expected_base: SourceDocumentKey::named(
                        universe,
                        "reusable",
                        SourceDocumentKind::Prefab,
                    )
                    .unwrap(),
                },
                PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::InstanceAddition(
                        SceneEntityId::new("inner").unwrap(),
                    ),
                    expected_base: SourceDocumentKey::project_location(
                        universe,
                        AssetSourcePath::new("parts/socket.prefab").unwrap(),
                        SourceDocumentKind::Prefab,
                    ),
                },
            ],
            target: AuthoredReferenceTarget::Input(ReferenceInputId::new(endpoint).unwrap()),
        },
    };
    let schema = schema([field(
        "target",
        ComponentFieldPath::from_fields(["settings", "target"]),
        ComponentValue::EntityReference(authored("canonical_socket")),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::EntityRef)]);
    let source = ComponentValue::map([("retained", ComponentValue::String("original".into()))]);
    let limits = ComponentValueMaterializationLimits::default();
    for (endpoint, overrides) in [
        ("canonical_socket", BTreeMap::new()),
        (
            "context_socket",
            BTreeMap::from([(
                ComponentFieldId::new("target"),
                ComponentValue::EntityReference(authored("context_socket")),
            )]),
        ),
    ] {
        let original = (source.clone(), schema.clone(), overrides.clone());
        let (expected, cost) =
            effective_component_value_with_default_overrides(&schema, &source, &overrides, limits)
                .unwrap();
        let mut observations = Vec::new();
        let frozen = freeze_component_defaults_with_admission(
            &schema,
            &source,
            &overrides,
            limits,
            |nodes, bytes, allocated| {
                observations.push((nodes, bytes, allocated));
                Ok::<(), Infallible>(())
            },
        )
        .unwrap()
        .unwrap();
        let reference_bytes = endpoint.len()
            + "outer".len()
            + "inner".len()
            + "reusable".len()
            + "parts/socket.prefab".len()
            + 2 * (36 + 1);
        let value_bytes = reference_bytes + 2 * size_of::<PrefabInstanceHop>();
        let entry_bytes = size_of::<(String, FrozenDefaultNode)>() + 6 * size_of::<usize>();
        let frozen_requests = [
            (1, 0, size_of::<FrozenComponentDefaults>()),
            (1, "settings".len(), entry_bytes + "settings".len()),
            (1, "target".len(), entry_bytes + "target".len()),
            (6, reference_bytes, value_bytes),
        ];
        let retained_bytes = size_of::<FrozenComponentDefaults>()
            + 2 * entry_bytes
            + "settings".len()
            + "target".len()
            + value_bytes;
        let first_frozen = observations.len() - frozen_requests.len();
        assert_eq!(&observations[first_frozen..], &frozen_requests);
        assert_eq!(frozen.retained_bytes(), retained_bytes);
        let path_bytes =
            2 * size_of::<ComponentFieldPathSegment>() + "settings".len() + "target".len();
        let plan_entry =
            size_of::<(&ComponentFieldPathSegment, DefaultPlan<'_>)>() + 6 * size_of::<usize>();
        assert_eq!(
            observations
                .iter()
                .map(|(_, _, allocated)| allocated)
                .sum::<usize>(),
            5 * path_bytes
                + 4 * size_of::<(&ComponentFieldPath, &ComponentValue)>()
                + 2 * plan_entry
                + 4 * size_of::<ValueCursor<'_>>()
                + retained_bytes,
        );
        assert_eq!(frozen.read(&source).to_owned(), *expected);
        assert_eq!(Some(frozen.cost()), cost);
        assert_eq!(
            (&source, &schema, &overrides),
            (&original.0, &original.1, &original.2)
        );
        let marker = String::from("authored frozen allocation refused");
        for boundary in first_frozen..observations.len() {
            let mut calls = 0;
            let result = freeze_component_defaults_with_admission(
                &schema,
                &source,
                &overrides,
                limits,
                |nodes, bytes, allocated| {
                    assert_eq!((nodes, bytes, allocated), observations[calls]);
                    let current = calls;
                    calls += 1;
                    if current == boundary {
                        Err(&marker)
                    } else {
                        Ok(())
                    }
                },
            );
            let Err(EffectiveValueAdmissionError::Admission(error)) = result else {
                panic!("allocation boundary {boundary} must preserve the caller refusal");
            };
            assert!(std::ptr::eq(error, &marker));
            assert_eq!(calls, boundary + 1);
            assert_eq!(
                (&source, &schema, &overrides),
                (&original.0, &original.1, &original.2)
            );
        }
    }
}

#[test]
fn caller_refusal_is_preserved_at_every_freezing_boundary() {
    #[derive(Debug)]
    struct Refusal<'a>(&'a String);

    let schema = schema([
        field(
            "label",
            row_path(0, "label"),
            ComponentValue::String("chosen".into()),
        ),
        field(
            "enabled",
            row_path(1, "enabled"),
            ComponentValue::Bool(true),
        ),
    ]);
    let source = ComponentValue::map([("retained", ComponentValue::String("original".into()))]);
    let mut observations = Vec::new();
    freeze_component_defaults_with_admission(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
        |nodes, bytes, allocated| {
            observations.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    let marker = "borrowed caller refusal".to_owned();
    for boundary in 0..observations.len() {
        let mut calls = 0;
        let result = freeze_component_defaults_with_admission(
            &schema,
            &source,
            &BTreeMap::new(),
            ComponentValueMaterializationLimits::default(),
            |_, _, _| {
                let current = calls;
                calls += 1;
                if current == boundary {
                    Err(Refusal(&marker))
                } else {
                    Ok(())
                }
            },
        );
        let Err(EffectiveValueAdmissionError::Admission(Refusal(actual))) = result else {
            panic!("boundary {boundary} must retain the caller refusal");
        };
        assert!(std::ptr::eq(actual, &marker));
        assert_eq!(calls, boundary + 1);
        assert!(source.get("rows").is_none());
    }
}

#[test]
fn selected_large_default_is_admitted_before_its_clone() {
    let payload_bytes = 64 * 1024;
    let schema = schema([field(
        "value",
        ComponentFieldPath::from_fields(["value"]),
        ComponentValue::String("x".repeat(payload_bytes)),
    )]);
    let source = ComponentValue::Map(BTreeMap::new());
    let mut reached_clone = false;
    let result = freeze_component_defaults_with_admission(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
        |nodes, bytes, allocated| {
            if (nodes, bytes, allocated) == (1, payload_bytes, payload_bytes) {
                reached_clone = true;
                Err("selected clone refused")
            } else {
                Ok(())
            }
        },
    );
    assert!(reached_clone);
    assert!(matches!(
        result,
        Err(EffectiveValueAdmissionError::Admission(
            "selected clone refused"
        )),
    ));
    assert_eq!(source, ComponentValue::Map(BTreeMap::new()));
}

#[test]
fn hidden_canonical_defaults_are_neither_cloned_nor_retained() {
    let schema = schema(["selected", "fallback"].map(|name| {
        field(
            name,
            ComponentFieldPath::from_fields([name]),
            ComponentValue::String("x".repeat(64 * 1024)),
        )
    }));
    let source = ComponentValue::Map(BTreeMap::new());
    let mut overrides = BTreeMap::from([(
        ComponentFieldId::new("selected"),
        ComponentValue::String(String::new()),
    )]);
    let limits = ComponentValueMaterializationLimits::default();
    let refuse_large_copy = |_, _, allocated| {
        if allocated >= 32 * 1024 {
            Err("large default copy refused")
        } else {
            Ok(())
        }
    };
    assert!(matches!(
        freeze_component_defaults_with_admission(
            &schema,
            &source,
            &overrides,
            limits,
            refuse_large_copy,
        ),
        Err(EffectiveValueAdmissionError::Admission(
            "large default copy refused"
        )),
    ));
    overrides.insert(
        ComponentFieldId::new("fallback"),
        ComponentValue::String(String::new()),
    );
    let frozen = freeze_component_defaults_with_admission(
        &schema,
        &source,
        &overrides,
        limits,
        refuse_large_copy,
    )
    .unwrap()
    .unwrap();
    assert_eq!(frozen.read(&source).field_str("selected").unwrap(), "");
    assert_eq!(frozen.read(&source).field_str("fallback").unwrap(), "");
    assert!(frozen.retained_bytes() < 32 * 1024);
    assert_eq!(source, ComponentValue::Map(BTreeMap::new()));
}

#[test]
fn borrowed_native_and_raw_walkers_admit_the_same_complete_work_as_owned_values() {
    let schema = schema([field(
        "mode",
        ComponentFieldPath::from_fields(["settings", "mode"]),
        ComponentValue::String("auto".into()),
    )]);
    let source = ComponentValue::map([(
        "opaque",
        ComponentValue::List(vec![ComponentValue::Bool(false), ComponentValue::Null]),
    )]);
    let limits = ComponentValueMaterializationLimits::default();
    let (owned, _) = effective_component_value_with_default_overrides(
        &schema,
        &source,
        &BTreeMap::new(),
        limits,
    )
    .unwrap();
    let frozen = freeze_component_defaults(&schema, &source, &BTreeMap::new(), limits)
        .unwrap()
        .unwrap();
    let mut owned_work = Vec::new();
    let mut borrowed_work = Vec::new();
    validate_value_materialization_budget_with_admission(
        owned.as_ref(),
        limits,
        |nodes, bytes, allocated| {
            owned_work.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    validate_value_materialization_budget_with_admission(
        frozen.read(&source),
        limits,
        |nodes, bytes, allocated| {
            borrowed_work.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(owned_work, borrowed_work);
    borrowed_work.clear();
    validate_native_component_value_references_with_admission(
        frozen.read(&source),
        limits,
        |nodes, bytes, allocated| {
            borrowed_work.push((nodes, bytes, allocated));
            Ok::<(), Infallible>(())
        },
    )
    .unwrap();
    assert_eq!(owned_work, borrowed_work);
    let marker = String::from("native walker refusal");
    for boundary in 0..owned_work.len() {
        let mut calls = 0;
        let result = validate_native_component_value_references_with_admission(
            frozen.read(&source),
            limits,
            |_, _, _| {
                let current = calls;
                calls += 1;
                if current == boundary {
                    Err(&marker)
                } else {
                    Ok(())
                }
            },
        );
        let Err(EffectiveValueAdmissionError::Admission(actual)) = result else {
            panic!("native walker must retain its caller refusal at boundary {boundary}");
        };
        assert!(std::ptr::eq(actual, &marker));
        assert_eq!(calls, boundary + 1);
    }
}
