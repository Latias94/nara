use std::{collections::BTreeMap, convert::Infallible};

use nara_reflect::{
    __private::{
        ComponentValueAdmissionError, FrozenComponentDefaults, freeze_component_defaults,
        validate_component_value_with_admission, validate_explicit_component_value,
        validate_explicit_component_value_with_admission,
    },
    ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentNumericBound,
    ComponentNumericDomain, ComponentNumericInterval, ComponentSchema, ComponentSchemaVersion,
    ComponentShapeMember, ComponentTypeId, ComponentValue, ComponentValueKind,
    ComponentValueMaterializationLimits, ComponentValueRef, ComponentValueShape,
    validate_component_value_with_limits,
};

fn schema() -> ComponentSchema {
    let key = ComponentFieldId::new("row-key");
    ComponentSchema::new(
        ComponentTypeId::new("test.ExplicitValidation"),
        "Explicit validation",
        ComponentSchemaVersion::ONE,
    )
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("rows"),
            "Rows",
            ComponentFieldPath::from_fields(["rows"]),
            ComponentValueShape::keyed_list(
                ComponentValueShape::record_members([
                    ComponentShapeMember::new("key", ComponentValueKind::String)
                        .with_id(key.clone()),
                    ComponentShapeMember::new(
                        "details",
                        ComponentValueShape::record_members([
                            ComponentShapeMember::new("active", ComponentValueKind::Bool),
                            ComponentShapeMember::new(
                                "count",
                                ComponentValueShape::scalar(ComponentValueKind::U64)
                                    .with_numeric_domain(ComponentNumericDomain::U8),
                            ),
                        ]),
                    ),
                ]),
                key,
            ),
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("threshold"),
            "Threshold",
            ComponentFieldPath::from_fields(["settings", "threshold"]),
            ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
                ComponentNumericInterval::U64 {
                    min: None,
                    max: Some(ComponentNumericBound::Included(10)),
                },
            ),
        ),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("label"),
            "Label",
            ComponentFieldPath::from_fields(["settings", "label"]),
            ComponentValueKind::String,
            ComponentValue::String("schema default".into()),
        ),
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::opaque(ComponentValueKind::List),
        ),
    ])
}

fn row(key: &str, count: ComponentValue) -> ComponentValue {
    ComponentValue::map([
        ("key", ComponentValue::String(key.into())),
        (
            "details",
            ComponentValue::map([("active", ComponentValue::Bool(true)), ("count", count)]),
        ),
    ])
}

fn source(rows: Vec<ComponentValue>, threshold: Option<ComponentValue>) -> ComponentValue {
    let mut settings = BTreeMap::new();
    if let Some(threshold) = threshold {
        settings.insert("threshold".into(), threshold);
    }
    ComponentValue::map([
        ("rows", ComponentValue::List(rows)),
        ("settings", ComponentValue::Map(settings)),
        ("payload", ComponentValue::List(Vec::new())),
    ])
}

fn freeze(schema: &ComponentSchema, source: &ComponentValue) -> FrozenComponentDefaults {
    freeze_component_defaults(
        schema,
        source,
        &BTreeMap::from([(
            ComponentFieldId::new("label"),
            ComponentValue::String("selected default".into()),
        )]),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .expect("the missing nested label requires actual frozen insertions")
}

fn semantic_cases() -> Vec<(&'static str, ComponentValue, bool)> {
    let valid = source(
        vec![
            row("first", ComponentValue::U64(1)),
            row("second", ComponentValue::U64(2)),
        ],
        Some(ComponentValue::U64(7)),
    );
    let mut missing_member = row("first", ComponentValue::U64(1));
    missing_member
        .remove_path(&ComponentFieldPath::from_fields(["details", "count"]))
        .unwrap();
    let mut undeclared = valid.clone();
    undeclared
        .set_path(
            &ComponentFieldPath::from_fields(["undeclared\n\"\\"]),
            ComponentValue::Null,
        )
        .unwrap();
    vec![
        ("valid", valid, true),
        (
            "duplicate key",
            source(
                vec![
                    row("duplicate\n\"\\", ComponentValue::U64(1)),
                    row("duplicate\n\"\\", ComponentValue::U64(2)),
                ],
                Some(ComponentValue::U64(7)),
            ),
            false,
        ),
        (
            "missing record member",
            source(vec![missing_member], Some(ComponentValue::U64(7))),
            false,
        ),
        (
            "nested wire kind",
            source(
                vec![row("first", ComponentValue::Bool(false))],
                Some(ComponentValue::U64(7)),
            ),
            false,
        ),
        (
            "numeric domain",
            source(
                vec![row("first", ComponentValue::U64(256))],
                Some(ComponentValue::U64(7)),
            ),
            false,
        ),
        (
            "numeric interval",
            source(
                vec![row("first", ComponentValue::U64(1))],
                Some(ComponentValue::U64(11)),
            ),
            false,
        ),
        (
            "required path",
            source(vec![row("first", ComponentValue::U64(1))], None),
            false,
        ),
        ("undeclared coverage", undeclared, false),
    ]
}

#[test]
fn explicit_semantics_match_standalone_validation_on_closed_raw_and_frozen_values() {
    let schema = schema();
    let limits = ComponentValueMaterializationLimits::default();
    for (case, source, valid) in semantic_cases() {
        let original = source.clone();
        let defaults = freeze(&schema, &source);
        let frozen = defaults.read(&source);
        let raw = frozen.to_owned();
        let raw_before = raw.clone();
        assert!(frozen.as_value().is_none(), "{case}");
        assert!(
            frozen.field("settings").unwrap().as_value().is_none(),
            "{case}",
        );
        let expected = validate_component_value_with_limits(&schema, &raw, limits);
        assert_eq!(expected.is_ok(), valid, "{case}");
        for value in [ComponentValueRef::from(&raw), frozen] {
            assert_eq!(
                validate_explicit_component_value(&schema, value),
                expected,
                "{case}",
            );
            let expected = expected
                .clone()
                .map_err(ComponentValueAdmissionError::Value);
            assert_eq!(
                validate_explicit_component_value_with_admission(&schema, value, |_, _, _| {
                    Ok::<(), Infallible>(())
                }),
                expected,
                "{case}",
            );
            assert_eq!(
                validate_component_value_with_admission(&schema, value, limits, |_, _, _| {
                    Ok::<(), Infallible>(())
                }),
                expected,
                "{case}",
            );
            assert_eq!(
                value.field("settings").unwrap().field_str("label").unwrap(),
                "selected default",
            );
        }
        assert_eq!(source, original, "{case}");
        assert_eq!(raw, raw_before, "{case}");
    }
}

#[derive(Debug)]
struct Refusal {
    boundary: usize,
}

#[test]
fn every_explicit_admission_boundary_returns_the_original_refusal_and_stops() {
    let schema = schema();
    for (case, source, _) in semantic_cases() {
        let original = source.clone();
        let defaults = freeze(&schema, &source);
        let frozen = defaults.read(&source);
        let raw = frozen.to_owned();
        let raw_before = raw.clone();
        for value in [ComponentValueRef::from(&raw), frozen] {
            let mut boundaries = 0;
            let baseline =
                validate_explicit_component_value_with_admission(&schema, value, |_, _, _| {
                    boundaries += 1;
                    Ok::<(), Infallible>(())
                });
            assert_eq!(
                baseline,
                validate_explicit_component_value(&schema, value)
                    .map_err(ComponentValueAdmissionError::Value),
                "{case}",
            );
            assert!(boundaries > 0, "{case}");
            for stop in 1..=boundaries {
                let refusal = Box::new(Refusal { boundary: stop });
                let identity = std::ptr::from_ref(refusal.as_ref());
                let mut refusal = Some(refusal);
                let mut calls = 0;
                let result =
                    validate_explicit_component_value_with_admission(&schema, value, |_, _, _| {
                        calls += 1;
                        assert!(calls <= stop, "{case}: callback after refusal at {stop}");
                        if calls == stop {
                            Err(refusal.take().unwrap())
                        } else {
                            Ok(())
                        }
                    });
                let Err(ComponentValueAdmissionError::Admission(returned)) = result else {
                    panic!("{case}: expected caller refusal at boundary {stop}");
                };
                assert_eq!(std::ptr::from_ref(returned.as_ref()), identity, "{case}");
                assert_eq!(returned.boundary, stop, "{case}");
                assert_eq!(calls, stop, "{case}");
                assert_eq!(value, &raw_before, "{case}");
            }
        }
        assert_eq!(source, original, "{case}");
    }
}

#[test]
fn opaque_descendants_do_not_increase_explicit_semantic_work_or_change_backing() {
    let schema = schema();
    let mut measurements = [Vec::new(), Vec::new()];
    for length in [1, 4096] {
        let mut source = source(
            vec![row("first", ComponentValue::U64(1))],
            Some(ComponentValue::U64(7)),
        );
        let item = ComponentValue::map([
            ("text", ComponentValue::String("opaque".repeat(32))),
            (
                "nested",
                ComponentValue::List(vec![ComponentValue::U64(11), ComponentValue::Null]),
            ),
        ]);
        source
            .replace_path(
                &ComponentFieldPath::from_fields(["payload"]),
                ComponentValue::List(vec![item; length]),
            )
            .unwrap();
        let original = source.clone();
        let defaults = freeze(&schema, &source);
        let frozen = defaults.read(&source);
        let raw = frozen.to_owned();
        let raw_before = raw.clone();
        for (representation, (value, backing)) in
            [(ComponentValueRef::from(&raw), &raw), (frozen, &source)]
                .into_iter()
                .enumerate()
        {
            let mut work = (0usize, 0usize, 0usize);
            validate_explicit_component_value_with_admission(
                &schema,
                value,
                |nodes, bytes, scratch| {
                    work.0 += nodes;
                    work.1 += bytes;
                    work.2 += scratch;
                    Ok::<(), Infallible>(())
                },
            )
            .unwrap();
            assert!(work.0 > 0);
            assert!(work.2 > 0);
            assert_eq!(
                value.field("payload").unwrap().as_list().unwrap().len(),
                length,
            );
            assert!(std::ptr::eq(
                value.field("payload").unwrap().as_value().unwrap(),
                backing.field("payload").unwrap(),
            ));
            assert_eq!(value, &raw_before);
            measurements[representation].push(work);
        }
        assert_eq!(source, original);
    }
    for work in measurements {
        assert_eq!(work[0], work[1]);
    }
}
