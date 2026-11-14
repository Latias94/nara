use super::*;
use crate::MaterialParameters;

#[derive(Clone, Debug, Default, PartialEq, MaterialParameters)]
#[material(crate = crate)]
struct Palette {
    #[material(id = "palette.tint")]
    tint: Vec4,
}

#[derive(Clone, Debug, Default, PartialEq, MaterialParameters)]
#[material(crate = "crate")]
struct Mixed {
    #[material(id = "surface.gain")]
    gain: f32,
    #[material(id = "surface.direction")]
    direction: Vec3,
    #[material(id = "surface.mode")]
    mode: u32,
    #[material(id = "surface.shift")]
    shift: Vec2,
    #[material(id = "surface.layer")]
    layer: i32,
}

#[derive(MaterialParameters)]
#[material(crate = crate)]
struct RenamedPalette {
    #[material(id = "palette.tint")]
    color: Vec4,
}

#[derive(MaterialParameters)]
#[material(crate = crate)]
struct RawName {
    #[material(id = "surface.weight")]
    r#type: f32,
}

fn mixed() -> Mixed {
    Mixed {
        gain: 1.5,
        direction: Vec3::new(-0.0, -2.0, 3.0),
        mode: 7,
        shift: Vec2::new(4.0, 5.0),
        layer: -7,
    }
}

fn field(id: &str, name: &str, kind: MaterialParameterKind, offset: u32) -> MaterialUniformField {
    MaterialUniformField::new(id, name, kind, offset).expect("valid fixture field")
}

#[test]
fn palette_is_a_wgsl_vector_with_exact_bits_and_dynamic_parity() {
    let palette = Palette {
        tint: Vec4::new(0.25, -0.0, 0.75, 1.0),
    };
    let layout = Palette::parameter_layout().unwrap();
    assert_eq!(layout.byte_size(), 16);
    assert_eq!(
        layout.fields(),
        [field(
            "palette.tint",
            "tint",
            MaterialParameterKind::Vec4,
            0
        )]
    );
    let values = palette.parameter_values().unwrap();
    assert_eq!(
        values,
        MaterialParameterValues::from([(
            "palette.tint".into(),
            MaterialParameterValue::Vec4([0.25, -0.0, 0.75, 1.0]),
        )])
    );
    let expected: Vec<u8> = [0x3e80_0000_u32, 0x8000_0000, 0x3f40_0000, 0x3f80_0000]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    assert_eq!(palette.encode_uniform().unwrap(), expected);
    assert_eq!(layout.encode(&values).unwrap(), expected);
    let restored = Palette::from_parameter_values(&values).unwrap();
    assert_eq!(restored.encode_uniform().unwrap(), expected);
}

#[test]
fn mixed_uniform_matches_golden_wgsl_offsets_padding_and_scalar_types() {
    let parameters = mixed();
    let layout = Mixed::parameter_layout().unwrap();
    assert_eq!(layout.byte_size(), 48);
    assert_eq!(
        layout.fields(),
        [
            field("surface.gain", "gain", MaterialParameterKind::F32, 0),
            field(
                "surface.direction",
                "direction",
                MaterialParameterKind::Vec3,
                16
            ),
            field("surface.mode", "mode", MaterialParameterKind::U32, 28),
            field("surface.shift", "shift", MaterialParameterKind::Vec2, 32),
            field("surface.layer", "layer", MaterialParameterKind::I32, 40),
        ]
    );
    let expected: Vec<u8> = [
        0x3fc0_0000_u32,
        0,
        0,
        0,
        0x8000_0000,
        0xc000_0000,
        0x4040_0000,
        7,
        0x4080_0000,
        0x40a0_0000,
        0xffff_fff9,
        0,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect();
    let values = parameters.parameter_values().unwrap();
    assert_eq!(parameters.encode_uniform().unwrap(), expected);
    assert_eq!(layout.encode(&values).unwrap(), expected);
    assert_eq!(Mixed::from_parameter_values(&values).unwrap(), parameters);
    assert_eq!(
        Mixed::from_parameter_values(&values)
            .unwrap()
            .encode_uniform()
            .unwrap(),
        expected
    );
}

#[test]
fn default_values_and_all_padding_are_zero() {
    assert_eq!(Palette::default().encode_uniform().unwrap(), vec![0; 16]);
    assert_eq!(Mixed::default().encode_uniform().unwrap(), vec![0; 48]);
    let layout = Mixed::parameter_layout().unwrap();
    assert_eq!(
        layout
            .encode(&Mixed::default().parameter_values().unwrap())
            .unwrap(),
        vec![0; 48]
    );
}

#[test]
fn semantic_values_survive_rust_field_renames() {
    let values = Palette {
        tint: Vec4::new(1.0, 0.5, -0.0, 0.25),
    }
    .parameter_values()
    .unwrap();
    let renamed = RenamedPalette::from_parameter_values(&values).unwrap();
    assert_eq!(renamed.parameter_values().unwrap(), values);
    assert_eq!(
        RenamedPalette::parameter_layout().unwrap().fields()[0].name(),
        "color"
    );
    assert_eq!(
        renamed.encode_uniform().unwrap(),
        Palette::from_parameter_values(&values)
            .unwrap()
            .encode_uniform()
            .unwrap()
    );
    assert_eq!(
        RawName::parameter_layout().unwrap().fields()[0].name(),
        "type"
    );
}

#[test]
fn typed_and_dynamic_consumers_require_the_same_exact_value_set() {
    let layout = Mixed::parameter_layout().unwrap();
    let values = mixed().parameter_values().unwrap();
    let mut missing = values.clone();
    missing.remove("surface.mode");
    for result in [
        layout.encode(&missing).map(|_| ()),
        Mixed::from_parameter_values(&missing).map(|_| ()),
    ] {
        assert_eq!(
            result,
            Err(MaterialDeclarationError::MissingParameter(
                "surface.mode".into()
            ))
        );
    }
    let mut unknown = values.clone();
    unknown.insert("surface.unknown".into(), MaterialParameterValue::F32(1.0));
    for result in [
        layout.encode(&unknown).map(|_| ()),
        Mixed::from_parameter_values(&unknown).map(|_| ()),
    ] {
        assert_eq!(
            result,
            Err(MaterialDeclarationError::UnknownParameter(
                "surface.unknown".into()
            ))
        );
    }
    let mut wrong_kind = values;
    wrong_kind.insert("surface.mode".into(), MaterialParameterValue::I32(7));
    for result in [
        layout.encode(&wrong_kind).map(|_| ()),
        Mixed::from_parameter_values(&wrong_kind).map(|_| ()),
    ] {
        assert_eq!(
            result,
            Err(MaterialDeclarationError::ParameterKindMismatch {
                id: "surface.mode".into(),
                expected: MaterialParameterKind::U32,
                actual: MaterialParameterKind::I32,
            })
        );
    }
}

#[test]
fn all_floating_kinds_reject_nan_and_infinity_without_unwinding() {
    for nonfinite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for value in [
            MaterialParameterValue::F32(nonfinite),
            MaterialParameterValue::Vec2([0.0, nonfinite]),
            MaterialParameterValue::Vec3([0.0, nonfinite, 1.0]),
            MaterialParameterValue::Vec4([0.0, 1.0, nonfinite, 1.0]),
        ] {
            let outcome = std::panic::catch_unwind(|| {
                let layout = MaterialParameterLayout::new(
                    vec![field("value", "value", value.kind(), 0)],
                    16,
                )
                .unwrap();
                layout.encode(&MaterialParameterValues::from([("value".into(), value)]))
            })
            .expect("invalid values return an error without unwinding");
            assert_eq!(
                outcome,
                Err(MaterialDeclarationError::NonFiniteParameter("value".into()))
            );
        }
        let mut typed = mixed();
        typed.direction.y = nonfinite;
        assert_eq!(
            typed.parameter_values(),
            Err(MaterialDeclarationError::NonFiniteParameter(
                "surface.direction".into()
            ))
        );
        assert_eq!(
            typed.encode_uniform(),
            Err(MaterialDeclarationError::NonFiniteParameter(
                "surface.direction".into()
            ))
        );
        let mut values = mixed().parameter_values().unwrap();
        values.insert(
            "surface.direction".into(),
            MaterialParameterValue::Vec3([0.0, nonfinite, 1.0]),
        );
        assert_eq!(
            Mixed::from_parameter_values(&values),
            Err(MaterialDeclarationError::NonFiniteParameter(
                "surface.direction".into()
            ))
        );
    }
}

#[test]
fn layout_rejects_empty_identity_names_duplicates_and_overlap() {
    for (id, name) in [
        ("", "value"),
        (" \t", "value"),
        ("value", ""),
        ("value", " "),
    ] {
        assert!(MaterialUniformField::new(id, name, MaterialParameterKind::F32, 0).is_err());
    }
    assert_eq!(
        MaterialParameterLayout::new(
            vec![
                field("same", "first", MaterialParameterKind::F32, 0),
                field("same", "second", MaterialParameterKind::F32, 4),
            ],
            8,
        ),
        Err(MaterialDeclarationError::DuplicateParameterId(
            "same".into()
        ))
    );
    assert_eq!(
        MaterialParameterLayout::new(
            vec![
                field("first", "same", MaterialParameterKind::F32, 0),
                field("second", "same", MaterialParameterKind::F32, 4),
            ],
            8,
        ),
        Err(MaterialDeclarationError::DuplicateParameterName(
            "same".into()
        ))
    );
    assert_eq!(
        MaterialParameterLayout::new(
            vec![
                field("vector", "vector", MaterialParameterKind::Vec4, 0),
                field("scalar", "scalar", MaterialParameterKind::F32, 4),
            ],
            16,
        ),
        Err(MaterialDeclarationError::OverlappingParameters {
            first: "vector".into(),
            second: "scalar".into(),
        })
    );
}

#[test]
fn layout_rejects_misalignment_out_of_bounds_and_oversized_inputs() {
    assert!(matches!(
        MaterialUniformField::new("direction", "direction", MaterialParameterKind::Vec3, 4),
        Err(MaterialDeclarationError::MisalignedParameter { .. })
    ));
    assert!(matches!(
        MaterialParameterLayout::new(
            vec![field("value", "value", MaterialParameterKind::Vec4, 0)],
            20,
        ),
        Err(MaterialDeclarationError::MisalignedUniformSize { .. })
    ));
    for offset in [16, u32::MAX - 15] {
        assert!(matches!(
            MaterialParameterLayout::new(
                vec![field("value", "value", MaterialParameterKind::Vec4, offset)],
                16,
            ),
            Err(MaterialDeclarationError::ParameterOutOfBounds { .. })
        ));
    }
    for byte_size in [0, MAX_MATERIAL_UNIFORM_BYTES + 1, u32::MAX] {
        assert!(matches!(
            MaterialParameterLayout::new(
                vec![field("value", "value", MaterialParameterKind::F32, 0)],
                byte_size,
            ),
            Err(MaterialDeclarationError::InvalidUniformByteSize { .. })
        ));
    }
    assert!(matches!(
        MaterialParameterLayout::new(Vec::new(), 16),
        Err(MaterialDeclarationError::InvalidParameterCount { count: 0, .. })
    ));
    let fields: Vec<_> = (0..=MAX_MATERIAL_PARAMETERS)
        .map(|index| {
            field(
                &format!("field.{index}"),
                &format!("field_{index}"),
                MaterialParameterKind::F32,
                index as u32 * 4,
            )
        })
        .collect();
    assert!(matches!(
        MaterialParameterLayout::new(fields, 132),
        Err(MaterialDeclarationError::InvalidParameterCount { count: 33, .. })
    ));
    assert!(matches!(
        material_uniform_size(u64::MAX),
        Err(MaterialDeclarationError::InvalidUniformByteSize { .. })
    ));
    assert_eq!(
        material_uniform_offset(u64::MAX),
        Err(MaterialDeclarationError::UniformOffsetOverflow(u64::MAX))
    );
}

#[test]
fn layout_accepts_exact_field_and_uniform_limits() {
    let fields = (0..MAX_MATERIAL_PARAMETERS)
        .map(|index| {
            field(
                &format!("field.{index}"),
                &format!("field_{index}"),
                MaterialParameterKind::F32,
                index as u32 * 4,
            )
        })
        .collect();
    let layout = MaterialParameterLayout::new(fields, MAX_MATERIAL_UNIFORM_BYTES).unwrap();
    let values = (0..MAX_MATERIAL_PARAMETERS)
        .map(|index| (format!("field.{index}"), MaterialParameterValue::F32(-0.0)))
        .collect();
    let encoded = layout.encode(&values).unwrap();
    assert_eq!(encoded.len(), MAX_MATERIAL_UNIFORM_BYTES as usize);
    assert!(
        encoded[MAX_MATERIAL_PARAMETERS * 4..]
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn integer_extremes_are_encoded_without_loss_or_float_conversion() {
    let layout = MaterialParameterLayout::new(
        vec![
            field("signed", "signed", MaterialParameterKind::I32, 0),
            field("unsigned", "unsigned", MaterialParameterKind::U32, 4),
        ],
        8,
    )
    .unwrap();
    for signed in [i32::MIN, -1, 0, i32::MAX] {
        let values = MaterialParameterValues::from([
            ("signed".into(), MaterialParameterValue::I32(signed)),
            ("unsigned".into(), MaterialParameterValue::U32(u32::MAX)),
        ]);
        let expected: Vec<u8> = signed
            .to_le_bytes()
            .into_iter()
            .chain(u32::MAX.to_le_bytes())
            .collect();
        assert_eq!(layout.encode(&values).unwrap(), expected);
    }
}
