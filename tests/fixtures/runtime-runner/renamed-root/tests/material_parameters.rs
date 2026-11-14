use engine::material::{MaterialParameterValue, MaterialParameters, Vec4};

type ArrayLength = u32;

#[derive(Debug, PartialEq, MaterialParameters)]
struct Parameters {
    #[material(id = "game.weight")]
    reader: f32,
    #[material(id = "game.count")]
    count: ArrayLength,
    #[material(id = "game.tint")]
    tint: Vec4,
}

#[test]
fn renamed_root_material_derive_uses_its_runtime_support_without_a_direct_encoder_dependency() {
    let parameters = Parameters {
        reader: -0.0,
        count: 7,
        tint: Vec4::new(0.25, 0.5, 0.75, 1.0),
    };
    let values = parameters.parameter_values().unwrap();
    assert_eq!(
        values.get("game.count"),
        Some(&MaterialParameterValue::U32(7))
    );
    let restored = Parameters::from_parameter_values(&values).unwrap();
    assert_eq!(restored.reader.to_bits(), parameters.reader.to_bits());
    assert_eq!(restored.count, parameters.count);
    assert_eq!(restored.tint, parameters.tint);
    let layout = Parameters::parameter_layout().unwrap();
    let bytes = parameters.encode_uniform().unwrap();
    assert_eq!(layout.encode(&values).unwrap(), bytes);
    assert_eq!(&bytes[..4], &(-0.0_f32).to_le_bytes());
    let tint = layout
        .fields()
        .iter()
        .find(|field| field.id() == "game.tint")
        .unwrap();
    assert_eq!(tint.offset(), 16);
    assert_eq!(&bytes[16..20], &0.25_f32.to_le_bytes());
    assert_eq!(bytes.len(), 32);
}
