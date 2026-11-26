use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentFloat,
    ComponentNumericBound::{Excluded, Included},
    ComponentNumericDomain, ComponentNumericInterval, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentShapeErrorKind, ComponentTypeId,
    ComponentValue, ComponentValueKind, ComponentValueShape, validate_component_value,
};

fn float(value: f64) -> ComponentFloat {
    ComponentFloat::new(value).unwrap()
}

fn positive_f32() -> ComponentValueShape {
    ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32)
        .with_numeric_interval(ComponentNumericInterval::F64 {
            min: Some(Excluded(float(0.0))),
            max: None,
        })
}

fn schema(shape: ComponentValueShape) -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new("nara.test.NumericInterval"),
        "Numeric interval",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("value"),
        "Value",
        ComponentFieldPath::from_fields(["value"]),
        shape,
    )
    .with_capability(ComponentCapability::Scene)])
}

fn catalog(schema: ComponentSchema) -> ComponentSchemaCatalog {
    admit(schema).unwrap()
}

fn admit(schema: ComponentSchema) -> Result<ComponentSchemaCatalog, ComponentRegistryError> {
    ComponentSchemaCatalog::from_components([schema])
}

#[test]
fn signed_and_unsigned_intervals_preserve_exact_endpoints_and_wire_kind() {
    let signed = ComponentValueShape::scalar(ComponentValueKind::I64)
        .with_numeric_domain(ComponentNumericDomain::I8)
        .with_numeric_interval(ComponentNumericInterval::I64 {
            min: Some(Excluded(-2)),
            max: Some(Included(2)),
        });
    admit(schema(signed.clone())).unwrap();
    for (value, accepted) in [(-2, false), (-1, true), (2, true), (3, false)] {
        assert_eq!(signed.accepts_value(&ComponentValue::I64(value)), accepted);
    }
    assert!(!signed.accepts_value(&ComponentValue::U64(1)));

    let unsigned = ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
        ComponentNumericInterval::U64 {
            min: Some(Included(u64::MAX - 1)),
            max: Some(Excluded(u64::MAX)),
        },
    );
    admit(schema(unsigned.clone())).unwrap();
    assert!(unsigned.accepts_value(&ComponentValue::U64(u64::MAX - 1)));
    assert!(!unsigned.accepts_value(&ComponentValue::U64(u64::MAX)));
    assert!(!unsigned.accepts_value(&ComponentValue::I64(i64::MAX)));
}

#[test]
fn schema_admission_rejects_wrong_interval_kinds_and_out_of_domain_bounds() {
    let interval = ComponentNumericInterval::U64 {
        min: Some(Included(1)),
        max: None,
    };
    for kind in [
        ComponentValueKind::I64,
        ComponentValueKind::F64,
        ComponentValueKind::Bool,
    ] {
        let shape = ComponentValueShape::scalar(kind).with_numeric_interval(interval);
        assert!(matches!(
            admit(schema(shape)),
            Err(ComponentRegistryError::InvalidComponentFieldShape { source, .. })
                if matches!(source.kind, ComponentShapeErrorKind::InvalidNumericIntervalKind { .. })
        ));
    }
    for shape in [
        ComponentValueShape::scalar(ComponentValueKind::I64)
            .with_numeric_domain(ComponentNumericDomain::I8)
            .with_numeric_interval(ComponentNumericInterval::I64 {
                min: Some(Included(-129)),
                max: None,
            }),
        ComponentValueShape::scalar(ComponentValueKind::U64)
            .with_numeric_domain(ComponentNumericDomain::U8)
            .with_numeric_interval(ComponentNumericInterval::U64 {
                min: None,
                max: Some(Included(256)),
            }),
        ComponentValueShape::scalar(ComponentValueKind::F64)
            .with_numeric_domain(ComponentNumericDomain::F32)
            .with_numeric_interval(ComponentNumericInterval::F64 {
                min: None,
                max: Some(Included(float(f64::MAX))),
            }),
    ] {
        assert_eq!(
            shape.validate().unwrap_err().kind,
            ComponentShapeErrorKind::NumericIntervalBoundOutsideDomain
        );
    }
    assert!(ComponentFloat::new(f64::INFINITY).is_err());
    assert!(ComponentFloat::new(f64::NAN).is_err());
}

#[test]
fn empty_intervals_include_gaps_between_discrete_representable_values() {
    for interval in [
        ComponentNumericInterval::I64 {
            min: Some(Excluded(0)),
            max: Some(Excluded(1)),
        },
        ComponentNumericInterval::I64 {
            min: Some(Excluded(i64::MAX)),
            max: None,
        },
        ComponentNumericInterval::I64 {
            min: None,
            max: Some(Excluded(i64::MIN)),
        },
        ComponentNumericInterval::U64 {
            min: None,
            max: Some(Excluded(0)),
        },
        ComponentNumericInterval::U64 {
            min: Some(Excluded(u64::MAX)),
            max: None,
        },
        ComponentNumericInterval::U64 {
            min: Some(Included(2)),
            max: Some(Included(1)),
        },
        ComponentNumericInterval::F64 {
            min: Some(Excluded(float(1.0))),
            max: Some(Excluded(float(1.0_f64.next_up()))),
        },
        ComponentNumericInterval::F64 {
            min: Some(Excluded(float(f64::MAX))),
            max: None,
        },
    ] {
        let shape =
            ComponentValueShape::scalar(interval.value_kind()).with_numeric_interval(interval);
        assert_eq!(
            shape.validate().unwrap_err().kind,
            ComponentShapeErrorKind::EmptyNumericInterval
        );
    }

    let between_f32_values = ComponentNumericInterval::F64 {
        min: Some(Excluded(float(1.0))),
        max: Some(Excluded(float(f64::from(1.0_f32.next_up())))),
    };
    let f32_shape = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32)
        .with_numeric_interval(between_f32_values);
    assert_eq!(
        f32_shape.validate().unwrap_err().kind,
        ComponentShapeErrorKind::EmptyNumericInterval
    );

    // These different F64 bounds lie strictly between the same adjacent F32 values.
    let no_f32_value = ComponentNumericInterval::F64 {
        min: Some(Included(float(1.0_f64.next_up()))),
        max: Some(Included(float(f64::from(1.0_f32.next_up()).next_down()))),
    };
    assert_eq!(
        f32_shape
            .with_numeric_interval(no_f32_value)
            .validate()
            .unwrap_err()
            .kind,
        ComponentShapeErrorKind::EmptyNumericInterval
    );

    for interval in [
        ComponentNumericInterval::I64 {
            min: Some(Included(7)),
            max: Some(Included(7)),
        },
        ComponentNumericInterval::U64 {
            min: None,
            max: None,
        },
        ComponentNumericInterval::F64 {
            min: Some(Included(float(-0.0))),
            max: Some(Included(float(0.0))),
        },
    ] {
        ComponentValueShape::scalar(interval.value_kind())
            .with_numeric_interval(interval)
            .validate()
            .unwrap();
    }
}

#[test]
fn f32_intervals_compare_materialized_values_without_rewriting_wire_bytes() {
    let positive = positive_f32();
    let tiny = ComponentValue::f64(f64::MIN_POSITIVE).unwrap();
    assert_eq!(
        positive.validate_value(&tiny).unwrap_err().kind,
        ComponentShapeErrorKind::NumericInterval
    );
    assert_eq!(tiny.as_f64(), Some(f64::MIN_POSITIVE));
    assert!(positive.accepts_value(&ComponentValue::f64(f64::from(f32::from_bits(1))).unwrap()));

    let below_next_f32 = ComponentValueShape::scalar(ComponentValueKind::F64)
        .with_numeric_domain(ComponentNumericDomain::F32)
        .with_numeric_interval(ComponentNumericInterval::F64 {
            min: None,
            max: Some(Excluded(float(f64::from(1.0_f32.next_up())))),
        });
    let rounds_up = f64::from(1.0_f32.next_up()).next_down();
    assert_eq!(
        below_next_f32
            .validate_value(&ComponentValue::f64(rounds_up).unwrap())
            .unwrap_err()
            .kind,
        ComponentShapeErrorKind::NumericInterval
    );
    assert!(below_next_f32.accepts_value(&ComponentValue::f64(1.0).unwrap()));
    assert!(!positive.accepts_value(&ComponentValue::f64(-0.0).unwrap()));
    assert!(!positive.accepts_value(&ComponentValue::f64(0.0).unwrap()));
}

#[test]
fn defaults_and_creation_values_use_interval_validation_with_correct_error_classification() {
    let mut with_default = schema(positive_f32());
    with_default.fields[0].default_value = Some(ComponentValue::f64(0.0).unwrap());
    assert!(matches!(
        admit(with_default.clone()),
        Err(ComponentRegistryError::InvalidComponentFieldDefault { source, .. })
            if source.kind == ComponentShapeErrorKind::NumericInterval
    ));
    with_default.fields[0].default_value = Some(ComponentValue::f64(f64::from(f32::MAX)).unwrap());
    admit(with_default.clone()).unwrap();
    with_default.fields[0].default_value = Some(ComponentValue::f64(f64::MAX).unwrap());
    assert!(matches!(
        admit(with_default),
        Err(ComponentRegistryError::InvalidComponentFieldDefault { source, .. })
            if matches!(source.kind, ComponentShapeErrorKind::NumericDomain(_))
    ));

    let invalid_initial = schema(positive_f32()).with_authoring_initial_value(ComponentValue::map(
        [("value", ComponentValue::f64(0.0).unwrap())],
    ));
    assert!(matches!(admit(invalid_initial),
        Err(ComponentRegistryError::InvalidComponentAuthoringInitialValue {
            source: ComponentCodecError::InvalidField { field, .. }, ..
        }) if field == "value"));
}

#[test]
fn recursive_intervals_validate_the_selected_value() {
    let schema = schema(ComponentValueShape::map(ComponentValueShape::list(
        ComponentValueShape::optional(positive_f32()),
    )));
    admit(schema.clone()).unwrap();
    let value = |entries| {
        ComponentValue::map([(
            "value",
            ComponentValue::map([("entry", ComponentValue::List(entries))]),
        )])
    };
    validate_component_value(
        &schema,
        &value(vec![
            ComponentValue::Null,
            ComponentValue::f64(1.0).unwrap(),
        ]),
    )
    .unwrap();
    let invalid = value(vec![
        ComponentValue::Null,
        ComponentValue::f64(0.0).unwrap(),
    ]);
    assert!(matches!(
        validate_component_value(&schema, &invalid),
        Err(ComponentCodecError::InvalidField { field, .. }) if field == "value.entry[1]"
    ));
}

#[test]
fn interval_identity_tracks_endpoints() {
    let unbounded = catalog(schema(ComponentValueShape::scalar(ComponentValueKind::I64)));
    let intervals = [
        ComponentNumericInterval::I64 {
            min: Some(Included(0)),
            max: None,
        },
        ComponentNumericInterval::I64 {
            min: Some(Excluded(0)),
            max: None,
        },
        ComponentNumericInterval::I64 {
            min: Some(Included(1)),
            max: None,
        },
        ComponentNumericInterval::I64 {
            min: None,
            max: Some(Included(0)),
        },
    ];
    let mut fingerprints = std::collections::BTreeSet::from([unbounded.fingerprint()]);
    let catalogs = intervals.map(|interval| {
        catalog(schema(
            ComponentValueShape::scalar(ComponentValueKind::I64).with_numeric_interval(interval),
        ))
    });
    for catalog in &catalogs {
        assert!(fingerprints.insert(catalog.fingerprint()));
    }
    let zero = |value| {
        catalog(schema(
            ComponentValueShape::scalar(ComponentValueKind::F64).with_numeric_interval(
                ComponentNumericInterval::F64 {
                    min: Some(Included(float(value))),
                    max: None,
                },
            ),
        ))
    };
    assert_eq!(zero(-0.0).fingerprint(), zero(0.0).fingerprint());
}

#[test]
fn interval_builder_preserves_optional_null_and_rejects_nonnumeric_containers() {
    let interval = ComponentNumericInterval::U64 {
        min: Some(Included(1)),
        max: None,
    };
    let optional =
        ComponentValueShape::optional(ComponentValueShape::scalar(ComponentValueKind::U64))
            .with_numeric_interval(interval);
    assert_eq!(optional.numeric_interval(), Some(interval));
    assert!(optional.accepts_value(&ComponentValue::Null));
    assert!(!optional.accepts_value(&ComponentValue::U64(0)));
    assert!(
        ComponentValueShape::list(ComponentValueShape::scalar(ComponentValueKind::U64))
            .with_numeric_interval(interval)
            .validate()
            .is_err()
    );
}

#[cfg(feature = "serde")]
#[test]
fn interval_metadata_round_trips_and_unknown_contracts_fail_closed() {
    let legacy = catalog(schema(ComponentValueShape::scalar(ComponentValueKind::F64)));
    let legacy_json = legacy.to_json_string().unwrap();
    assert!(!legacy_json.contains("numeric_interval"));
    let ranged = catalog(schema(positive_f32()));
    let json = ranged.to_json_string().unwrap();
    let decoded = ComponentSchemaCatalog::from_json_bytes(json.as_bytes()).unwrap();
    assert_eq!(decoded, ranged);
    assert_eq!(decoded.fingerprint(), ranged.fingerprint());
    for shape in [
        ComponentValueShape::scalar(ComponentValueKind::I64).with_numeric_interval(
            ComponentNumericInterval::I64 {
                min: Some(Included(i64::MIN)),
                max: Some(Excluded(i64::MAX)),
            },
        ),
        ComponentValueShape::scalar(ComponentValueKind::U64).with_numeric_interval(
            ComponentNumericInterval::U64 {
                min: Some(Excluded(0)),
                max: Some(Included(u64::MAX)),
            },
        ),
    ] {
        let expected = catalog(schema(shape));
        assert_eq!(
            ComponentSchemaCatalog::from_json_bytes(expected.to_json_string().unwrap().as_bytes())
                .unwrap(),
            expected
        );
    }
    assert!(
        ComponentSchemaCatalog::from_json_bytes(
            json.replace("\"bound\": \"excluded\"", "\"bound\": \"future\"")
                .as_bytes()
        )
        .is_err()
    );
    let mut wire: serde_json::Value = serde_json::from_str(&json).unwrap();
    wire["components"][0]["fields"][0]["shape"]["numeric_interval"]["kind"] =
        serde_json::json!("f16");
    assert!(ComponentSchemaCatalog::from_json_bytes(&serde_json::to_vec(&wire).unwrap()).is_err());
    let mut wire: serde_json::Value = serde_json::from_str(&json).unwrap();
    let interval = &mut wire["components"][0]["fields"][0]["shape"]["numeric_interval"];
    interval["future_rule"] = serde_json::json!(true);
    assert!(ComponentSchemaCatalog::from_json_bytes(&serde_json::to_vec(&wire).unwrap()).is_err());
}
