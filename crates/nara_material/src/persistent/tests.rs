use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

use glam::{Vec2, Vec3, Vec4};
use nara_asset::{
    AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
    AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetRefExportPolicy, AssetServer,
    AssetSourceKind, AssetSourcePath, ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec,
    SourceAssetImporter, StableAssetId, allocate_anonymous_handle,
};
use nara_core::{ByteLimit, DepthLimit, ItemLimit};
use nara_ecs::World;
use nara_image::ImageAsset;
use nara_reflect::{
    AssetReferenceDemand, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
    ComponentFieldPath, ComponentNumericDomain, ComponentRegistryBuilder,
    ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind, ComponentValueMaterializationLimits,
    collect_declared_asset_references_from_schema,
};

use crate::{
    AlphaMode2d, Material2dDeclaration, MaterialParameterLayout, MaterialParameterValue,
    MaterialParameterValues, MaterialParameters, MaterialTextureDeclaration, MaterialUniformField,
    asset::{Material2dAsset, Material2dDomain, Material2dRuntimeCodec, Material2dSource},
};

use super::{
    MATERIAL_SCHEMA_PROVIDER, Material2d, Material2dRecord, material2d_schema,
    register_material_components,
};

#[derive(MaterialParameters)]
#[material(crate = "crate")]
struct SurfaceParameters {
    #[material(id = "surface.strength")]
    strength: f32,
    #[material(id = "surface.signed")]
    signed: i32,
    #[material(id = "surface.count")]
    count: u32,
    #[material(id = "surface.uv")]
    uv: Vec2,
    #[material(id = "surface.bias")]
    bias: Vec3,
    #[material(id = "surface.color")]
    color: Vec4,
}

fn typed_parameters() -> SurfaceParameters {
    SurfaceParameters {
        strength: -0.0,
        signed: i32::MIN,
        count: u32::MAX,
        uv: Vec2::new(0.5, -1.25),
        bias: Vec3::new(-2.0, 0.0, 3.0),
        color: Vec4::new(1.0, 0.5, 0.25, 1.0),
    }
}

fn values() -> MaterialParameterValues {
    typed_parameters().parameter_values().unwrap()
}

fn declaration(id: &str, version: u32) -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<SurfaceParameters>(
        id,
        version,
        AlphaMode2d::Blend,
        vec![
            MaterialTextureDeclaration::new("image.detail", "detail_texture", 1).unwrap(),
            MaterialTextureDeclaration::new("image.mask", "mask_texture", 2).unwrap(),
        ],
    )
    .unwrap()
}

fn record() -> Material2dRecord {
    Material2dRecord::new(
        AssetRef::path("materials/surface.material2d").unwrap(),
        &declaration("test.surface", 2),
        values(),
        BTreeMap::from([(
            "image.detail".to_owned(),
            AssetRef::path("images/detail.png").unwrap(),
        )]),
    )
    .unwrap()
}

fn product(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        id.parse::<StableAssetId>().unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn material_product() -> AssetProductRef {
    product("11111111-1111-4111-8111-111111111111")
}

fn image_product() -> AssetProductRef {
    product("22222222-2222-4222-8222-222222222222")
}

fn registry() -> ComponentRegistrySnapshot {
    let mut builder = ComponentRegistryBuilder::new();
    register_material_components(&mut builder).unwrap();
    builder.freeze().unwrap()
}

fn source_asset(declaration: Material2dDeclaration) -> Material2dAsset {
    let textures = declaration
        .textures()
        .iter()
        .map(|texture| (texture.id().to_owned(), AssetRef::Product(image_product())))
        .collect();
    let source = Material2dSource::new(
        declaration,
        vec![AssetPath::new("surface.wgsl").unwrap()],
        values(),
        textures,
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            material_product().source(),
            AssetPath::new("materials/surface.material2d").unwrap(),
            AssetSourceKind::Other("material2d".to_owned()),
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(metadata.as_bytes()).unwrap();
    let mut reader = |path: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, Infallible> {
        assert_eq!(path.as_str(), "surface.wgsl");
        Ok(Arc::from(
            br#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.color;
}
"#
            .as_slice(),
        ))
    };
    Material2dDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        Arc::from(source.encode().unwrap()),
        &mut reader,
    ))
    .unwrap()
}

fn replace_field(value: &mut ComponentValue, path: &[&str], replacement: ComponentValue) {
    value
        .replace_path(
            &ComponentFieldPath::from_fields(path.iter().copied()),
            replacement,
        )
        .unwrap();
}

#[test]
fn record_roundtrip_preserves_all_semantic_kinds_and_complete_product_references() {
    let original = Material2dRecord::new(
        AssetRef::Product(material_product()),
        &declaration("test.surface", 2),
        values(),
        BTreeMap::from([(
            "image.detail".to_owned(),
            AssetRef::Product(image_product()),
        )]),
    )
    .unwrap();
    let value = original.to_component_value().unwrap();
    nara_reflect::validate_component_value(&material2d_schema(), &value).unwrap();
    let restored = Material2dRecord::from_component_value(&value).unwrap();
    assert_eq!(restored, original);
    let MaterialParameterValue::F32(strength) = restored.parameters()["surface.strength"] else {
        panic!("the semantic scalar kind changed");
    };
    assert_eq!(strength.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(
        restored.parameters()["surface.count"],
        MaterialParameterValue::U32(u32::MAX),
    );
    assert_eq!(
        restored.parameters()["surface.signed"],
        MaterialParameterValue::I32(i32::MIN),
    );
    assert_eq!(
        value
            .field("parameters")
            .unwrap()
            .field("vec2")
            .unwrap()
            .field("surface.uv")
            .unwrap(),
        &ComponentValue::List(vec![
            ComponentValue::f64(0.5).unwrap(),
            ComponentValue::f64(-1.25).unwrap(),
        ]),
    );
}

#[test]
fn carrier_declares_material_and_every_nested_texture_reference_as_immediate() {
    let schema = material2d_schema();
    let value = record().to_component_value().unwrap();
    let references =
        collect_declared_asset_references_from_schema(&schema, ComponentSchemaVersion::ONE, &value)
            .unwrap();
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .all(|reference| reference.demand() == AssetReferenceDemand::Immediate),
    );
    assert_eq!(
        references[0].path(),
        &ComponentFieldPath::from_fields(["material"]),
    );
    assert_eq!(references[0].asset_ref(), record().material());
    assert_eq!(
        references[1].path(),
        &ComponentFieldPath::from_fields(["texture_overrides", "image.detail"]),
    );
    assert_eq!(references[1].field_id().as_str(), "texture_overrides");
    assert_eq!(registry().catalog().components.len(), 1);
    nara_reflect::__private::resolve_schema_provider(MATERIAL_SCHEMA_PROVIDER).unwrap();
}

#[test]
fn declaration_projection_uses_stable_record_paths_real_types_and_authored_defaults() {
    let original = declaration("test.surface", 2);
    let layout = MaterialParameterLayout::new(
        original
            .parameters()
            .fields()
            .iter()
            .enumerate()
            .map(|(index, field)| {
                MaterialUniformField::new(
                    field.id(),
                    format!("renamed_{index}"),
                    field.kind(),
                    field.offset(),
                )
                .unwrap()
            })
            .collect(),
        original.parameters().byte_size(),
    )
    .unwrap();
    let renamed = Material2dDeclaration::new(
        original.id(),
        original.version(),
        original.alpha_mode(),
        layout,
        vec![
            MaterialTextureDeclaration::new("image.detail", "new_detail", 2).unwrap(),
            MaterialTextureDeclaration::new("image.mask", "new_mask", 1).unwrap(),
        ],
    )
    .unwrap();
    let fields = original.binding_fields(&values()).unwrap();
    assert_eq!(fields, renamed.binding_fields(&values()).unwrap());
    let scalar = fields
        .iter()
        .find(|field| {
            field.path == ComponentFieldPath::from_fields(["parameters", "f32", "surface.strength"])
        })
        .unwrap();
    assert_eq!(
        scalar.path,
        ComponentFieldPath::from_fields(["parameters", "f32", "surface.strength"]),
    );
    assert_eq!(scalar.numeric_domain(), Some(ComponentNumericDomain::F32));
    assert_eq!(
        scalar.default_value,
        Some(ComponentValue::f64(-0.0).unwrap())
    );
    let signed = fields
        .iter()
        .find(|field| {
            field.path == ComponentFieldPath::from_fields(["parameters", "i32", "surface.signed"])
        })
        .unwrap();
    assert_eq!(signed.value_kind(), ComponentValueKind::I64);
    assert_eq!(signed.numeric_domain(), Some(ComponentNumericDomain::I32));
    let vector = fields
        .iter()
        .find(|field| {
            field.path == ComponentFieldPath::from_fields(["parameters", "vec4", "surface.color"])
        })
        .unwrap();
    assert_eq!(vector.value_kind(), ComponentValueKind::List);
    assert!(vector.accepts_value(&ComponentValue::List(vec![
        ComponentValue::f64(1.0).unwrap();
        4
    ])));
    let schema = original.binding_schema(&values()).unwrap();
    assert_eq!(schema.id.as_str(), "nara.material2d");
    nara_reflect::validate_component_value(&schema, &record().to_component_value().unwrap())
        .unwrap();
    nara_reflect::ComponentSchemaOwnerSnapshot::admit(
        nara_reflect::PersistentComponentSchemaOwnerId::try_new("test.material-projection")
            .unwrap(),
        nara_reflect::ComponentSchemaCatalog {
            components: vec![schema],
            ..nara_reflect::ComponentSchemaCatalog::default()
        },
        None,
    )
    .unwrap();
    let untextured = Material2dDeclaration::from_parameters::<SurfaceParameters>(
        "test.untextured",
        1,
        AlphaMode2d::Blend,
        Vec::new(),
    )
    .unwrap();
    let untextured_record = Material2dRecord::new(
        record().material().clone(),
        &untextured,
        values(),
        BTreeMap::new(),
    )
    .unwrap();
    nara_reflect::validate_component_value(
        &untextured.binding_schema(&values()).unwrap(),
        &untextured_record.to_component_value().unwrap(),
    )
    .unwrap();
}

#[test]
fn exact_record_shape_kinds_ranges_vectors_and_unknown_tags_refuse() {
    let valid = record().to_component_value().unwrap();
    let replacements = [
        (
            vec!["declaration_id"],
            ComponentValue::String("bad id".to_owned()),
        ),
        (vec!["declaration_version"], ComponentValue::U64(0)),
        (
            vec!["declaration_version"],
            ComponentValue::U64(u64::from(u32::MAX) + 1),
        ),
        (
            vec!["parameters", "f32", "surface.strength"],
            ComponentValue::I64(1),
        ),
        (
            vec!["parameters", "f32", "surface.strength"],
            ComponentValue::f64(f64::MAX).unwrap(),
        ),
        (
            vec!["parameters", "f32", "surface.strength"],
            ComponentValue::map([("float", ComponentValue::f64(1.0).unwrap())]),
        ),
        (
            vec!["parameters", "f32", "surface.strength"],
            ComponentValue::map([
                ("f32", ComponentValue::f64(1.0).unwrap()),
                ("u32", ComponentValue::U64(1)),
            ]),
        ),
        (
            vec!["parameters", "i32", "surface.signed"],
            ComponentValue::I64(i64::MAX),
        ),
        (
            vec!["parameters", "u32", "surface.count"],
            ComponentValue::I64(1),
        ),
        (
            vec!["parameters", "u32", "surface.count"],
            ComponentValue::U64(u64::MAX),
        ),
        (
            vec!["parameters", "vec2", "surface.uv"],
            ComponentValue::List(vec![ComponentValue::f64(1.0).unwrap(); 3]),
        ),
        (
            vec!["parameters"],
            ComponentValue::map([] as [(&str, ComponentValue); 0]),
        ),
        (
            vec!["texture_overrides"],
            ComponentValue::map([("image.detail", ComponentValue::Null)]),
        ),
    ];
    for (path, replacement) in replacements {
        let mut malformed = valid.clone();
        replace_field(&mut malformed, &path, replacement);
        assert!(
            Material2dRecord::from_component_value(&malformed).is_err(),
            "accepted {path:?}"
        );
    }
    for unknown_tag in [false, true] {
        let mut malformed = valid.clone();
        let ComponentValue::Map(groups) = malformed
            .get_path_mut(&ComponentFieldPath::from_fields(["parameters"]))
            .unwrap()
        else {
            panic!("parameters must be a map");
        };
        let floats = groups.remove("f32").unwrap();
        if unknown_tag {
            groups.insert("float".to_owned(), floats);
        }
        assert!(Material2dRecord::from_component_value(&malformed).is_err());
    }
    let mut duplicate = valid.clone();
    let ComponentValue::Map(signed) = duplicate
        .get_path_mut(&ComponentFieldPath::from_fields(["parameters", "i32"]))
        .unwrap()
    else {
        panic!("the signed parameter group must be a map");
    };
    signed.insert("surface.strength".to_owned(), ComponentValue::I64(1));
    assert!(Material2dRecord::from_component_value(&duplicate).is_err());
    let ComponentValue::Map(mut extra) = valid.clone() else {
        panic!("record must be a map");
    };
    extra.insert("uniform_bytes".to_owned(), ComponentValue::List(Vec::new()));
    assert!(Material2dRecord::from_component_value(&ComponentValue::Map(extra)).is_err());
    let ComponentValue::Map(mut missing) = valid else {
        panic!("record must be a map");
    };
    missing.remove("parameters");
    assert!(Material2dRecord::from_component_value(&ComponentValue::Map(missing)).is_err());
}

#[test]
fn parameter_texture_counts_and_nonfinite_authored_values_refuse() {
    let mut value = record().to_component_value().unwrap();
    replace_field(
        &mut value,
        &["parameters", "f32"],
        ComponentValue::map(
            (0..33).map(|index| (format!("field{index}"), ComponentValue::f64(1.0).unwrap())),
        ),
    );
    assert!(Material2dRecord::from_component_value(&value).is_err());
    let mut value = record().to_component_value().unwrap();
    replace_field(
        &mut value,
        &["texture_overrides"],
        ComponentValue::map((0..9).map(|index| {
            (
                format!("image{index}"),
                nara_reflect::encode_asset_reference_value(&AssetRef::path("image.png").unwrap()),
            )
        })),
    );
    assert!(Material2dRecord::from_component_value(&value).is_err());
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut parameters = values();
        parameters.insert(
            "surface.strength".to_owned(),
            MaterialParameterValue::F32(invalid),
        );
        assert!(
            Material2dRecord::new(
                record().material().clone(),
                &declaration("test.surface", 2),
                parameters,
                BTreeMap::new()
            )
            .is_err()
        );
        let mut parameters = values();
        parameters.insert(
            "surface.uv".to_owned(),
            MaterialParameterValue::Vec2([0.0, invalid]),
        );
        assert!(
            Material2d::new(
                allocate_anonymous_handle().unwrap(),
                &declaration("test.surface", 2),
                parameters,
                BTreeMap::new()
            )
            .is_err()
        );
    }
}

#[test]
fn record_uses_exact_shared_materialization_node_byte_and_depth_limits() {
    let record = record();
    let value = record.to_component_value().unwrap();
    let cost = value.cost();
    let exact = ComponentValueMaterializationLimits::new(
        ItemLimit::new(cost.nodes()).unwrap(),
        ByteLimit::new(cost.logical_bytes()).unwrap(),
        DepthLimit::new(5).unwrap(),
    );
    assert_eq!(
        Material2dRecord::from_component_value_with_limits(&value, exact).unwrap(),
        record
    );
    assert_eq!(record.to_component_value_with_limits(exact).unwrap(), value);
    let nodes = exact.remaining(1, 0);
    let bytes = exact.remaining(0, 1);
    assert!(matches!(
        Material2dRecord::from_component_value_with_limits(&value, nodes),
        Err(ComponentCodecError::MaterializationNodeLimit { .. })
    ));
    assert!(matches!(
        record.to_component_value_with_limits(bytes),
        Err(ComponentCodecError::MaterializationByteLimit { .. })
    ));
    let shallow = ComponentValueMaterializationLimits::new(
        ItemLimit::new(cost.nodes()).unwrap(),
        ByteLimit::new(cost.logical_bytes()).unwrap(),
        DepthLimit::new(4).unwrap(),
    );
    assert!(matches!(
        Material2dRecord::from_component_value_with_limits(&value, shallow),
        Err(ComponentCodecError::MaterializationDepthLimit { .. })
    ));
    assert!(
        Material2dRecord::from_component_value_with_limits(
            &value,
            exact.remaining(cost.nodes(), 0)
        )
        .is_err()
    );
}

#[test]
fn frozen_record_decode_preserves_parameters_references_and_exact_admission() {
    let original = Material2dRecord::new(
        AssetRef::Product(material_product()),
        &declaration("test.surface", 2),
        values(),
        BTreeMap::from([(
            "image.detail".to_owned(),
            AssetRef::Product(image_product()),
        )]),
    )
    .unwrap();
    let expected = original.to_component_value().unwrap();
    let mut source = expected.clone();
    for name in ["declaration_id", "declaration_version", "parameters"] {
        source
            .remove_path(&ComponentFieldPath::from_fields([name]))
            .unwrap();
    }
    let before = source.clone();
    let schema = declaration("test.surface", 2)
        .binding_schema(&values())
        .unwrap();
    let frozen = nara_reflect::__private::freeze_component_defaults(
        &schema,
        &source,
        &BTreeMap::new(),
        ComponentValueMaterializationLimits::default(),
    )
    .unwrap()
    .unwrap();
    let value = frozen.read(&source);
    assert!(value.as_value().is_none());
    assert!(value.field("parameters").unwrap().as_value().is_none());
    assert!(std::ptr::eq(
        value.field("material").unwrap().as_value().unwrap(),
        source.field("material").unwrap(),
    ));
    let cost = expected.cost();
    let exact = ComponentValueMaterializationLimits::new(
        ItemLimit::new(cost.nodes()).unwrap(),
        ByteLimit::new(cost.logical_bytes()).unwrap(),
        DepthLimit::new(6).unwrap(),
    );
    assert_eq!(
        Material2dRecord::from_component_value_with_limits(&expected, exact).unwrap(),
        original
    );
    assert_eq!(
        Material2dRecord::from_component_value_with_limits(value, exact).unwrap(),
        original
    );
    for limits in [
        exact.remaining(1, 0),
        exact.remaining(0, 1),
        ComponentValueMaterializationLimits::new(
            ItemLimit::new(cost.nodes()).unwrap(),
            ByteLimit::new(cost.logical_bytes()).unwrap(),
            DepthLimit::new(5).unwrap(),
        ),
    ] {
        assert_eq!(
            Material2dRecord::from_component_value_with_limits(value, limits).unwrap_err(),
            Material2dRecord::from_component_value_with_limits(&expected, limits).unwrap_err(),
        );
    }
    let registry = registry();
    let mut server = AssetServer::new();
    let handle = server
        .reserve_product::<Material2dAsset>(material_product())
        .unwrap();
    let image = server
        .reserve_product::<ImageAsset>(image_product())
        .unwrap();
    let prepared = registry
        .preflight_component_with_context(
            &ComponentTypeId::new("nara.material2d"),
            value,
            &mut ComponentDecodeContext::with_asset_server(&mut server),
        )
        .unwrap();
    let mut world = World::new();
    world.insert_resource(server);
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();
    let material = world.get::<Material2d>(entity).unwrap();
    assert_eq!(material.handle(), handle);
    assert_eq!(material.declaration_id(), original.declaration_id());
    assert_eq!(
        material.declaration_version(),
        original.declaration_version()
    );
    assert_eq!(material.parameters(), original.parameters());
    assert_eq!(material.texture_overrides()["image.detail"], image);
    assert_eq!(source, before);
}

#[test]
fn generated_field_ids_preserve_full_semantic_keys_and_refuse_invalid_identity() {
    let layout = MaterialParameterLayout::new(
        vec![
            MaterialUniformField::new(
                "x".repeat(128),
                "value",
                crate::MaterialParameterKind::F32,
                0,
            )
            .unwrap(),
        ],
        4,
    )
    .unwrap();
    let long_declaration = Material2dDeclaration::new(
        "test.long",
        1,
        AlphaMode2d::Blend,
        layout,
        vec![MaterialTextureDeclaration::new("x".repeat(128), "lookup", 1).unwrap()],
    )
    .unwrap();
    let parameters = BTreeMap::from([("x".repeat(128), MaterialParameterValue::F32(1.0))]);
    let fields = long_declaration.binding_fields(&parameters).unwrap();
    assert_eq!(fields.len(), 2);
    assert_ne!(fields[0].id, fields[1].id);
    assert!(
        fields
            .iter()
            .all(|field| field.id.as_str().len() <= nara_reflect::ComponentFieldId::MAX_BYTES)
    );
    assert_eq!(
        fields[0].path,
        ComponentFieldPath::from_fields(["parameters", "f32", &"x".repeat(128)])
    );
    assert_eq!(
        fields[1].path,
        ComponentFieldPath::from_fields(["texture_overrides", &"x".repeat(128)])
    );
    let projected = long_declaration.binding_schema(&parameters).unwrap();
    let mut only_f32 = Material2dRecord::new(
        AssetRef::Product(material_product()),
        &long_declaration,
        parameters,
        BTreeMap::new(),
    )
    .unwrap()
    .to_component_value()
    .unwrap();
    nara_reflect::validate_component_value(&projected, &only_f32).unwrap();
    let empty_i32 = projected
        .fields
        .iter()
        .find(|field| field.path == ComponentFieldPath::from_fields(["parameters", "i32"]))
        .unwrap();
    assert!(!empty_i32.has_capability(nara_reflect::ComponentCapability::Edit));
    replace_field(
        &mut only_f32,
        &["parameters", "i32"],
        ComponentValue::map([("unselected", ComponentValue::I64(1))]),
    );
    assert!(nara_reflect::validate_component_value(&projected, &only_f32).is_err());
    let mut invalid = record().to_component_value().unwrap();
    replace_field(
        &mut invalid,
        &["parameters", "f32"],
        ComponentValue::map([("bad id", ComponentValue::f64(1.0).unwrap())]),
    );
    assert!(Material2dRecord::from_component_value(&invalid).is_err());
}

#[test]
fn resolved_material_record_distinguishes_primary_alias_and_exact_products() {
    let declaration = declaration("test.surface", 2);
    let primary = material_product();
    let auxiliary = AssetProductRef::new(
        primary.source(),
        ImportedProductId::parse_subproduct("33333333-3333-4333-8333-333333333333").unwrap(),
    )
    .unwrap();
    for (reference, matching, other) in [
        (AssetRef::StableId(primary.source()), primary, auxiliary),
        (AssetRef::Product(primary), primary, auxiliary),
        (AssetRef::Product(auxiliary), auxiliary, primary),
    ] {
        let record =
            Material2dRecord::new(reference, &declaration, values(), BTreeMap::new()).unwrap();
        record
            .validate_resolved_product(matching, &declaration)
            .unwrap();
        assert!(
            record
                .validate_resolved_product(other, &declaration)
                .is_err()
        );
    }
}

#[test]
fn record_validates_actual_source_and_processed_assets_before_binding() {
    let asset = source_asset(declaration("test.surface", 2));
    let record = record();
    record.validate_asset(&asset).unwrap();
    let bytes = Material2dRuntimeCodec::encode_runtime(&asset).unwrap();
    let processed = Material2dRuntimeCodec::decode_runtime(&bytes).unwrap();
    record.validate_asset(&processed).unwrap();
    let foreign_record = Material2dRecord::new(
        AssetRef::Product(image_product()),
        &declaration("test.surface", 2),
        values(),
        BTreeMap::new(),
    )
    .unwrap();
    assert!(foreign_record.validate_asset(&asset).is_err());
    for mismatched in [declaration("test.other", 2), declaration("test.surface", 3)] {
        assert!(record.validate_asset(&source_asset(mismatched)).is_err());
    }
    for (field, from, to, replacement) in [
        ("surface.strength", "f32", "u32", ComponentValue::U64(1)),
        (
            "surface.uv",
            "vec2",
            "vec3",
            ComponentValue::List(vec![ComponentValue::f64(1.0).unwrap(); 3]),
        ),
    ] {
        let mut malformed = record.to_component_value().unwrap();
        for kind in [from, to] {
            let ComponentValue::Map(values) = malformed
                .get_path_mut(&ComponentFieldPath::from_fields(["parameters", kind]))
                .unwrap()
            else {
                panic!("each typed parameter group must be a map");
            };
            if kind == from {
                assert!(values.remove(field).is_some());
            } else {
                values.insert(field.to_owned(), replacement.clone());
            }
        }
        let parsed = Material2dRecord::from_component_value(&malformed).unwrap();
        assert!(parsed.validate_asset(&asset).is_err());
    }
    let mut malformed = record.to_component_value().unwrap();
    replace_field(
        &mut malformed,
        &["texture_overrides"],
        ComponentValue::map([(
            "unknown.texture",
            nara_reflect::encode_asset_reference_value(&AssetRef::path("image.png").unwrap()),
        )]),
    );
    assert!(
        Material2dRecord::from_component_value(&malformed)
            .unwrap()
            .validate_asset(&asset)
            .is_err()
    );
    let mut incomplete = values();
    incomplete.remove("surface.count");
    let mut incomplete_wire = record.to_component_value().unwrap();
    let ComponentValue::Map(parameters) = incomplete_wire
        .get_path_mut(&ComponentFieldPath::from_fields(["parameters", "u32"]))
        .unwrap()
    else {
        panic!("parameters must be a map");
    };
    parameters.remove("surface.count");
    assert!(
        Material2dRecord::from_component_value(&incomplete_wire)
            .unwrap()
            .validate_asset(&asset)
            .is_err()
    );
    assert!(
        Material2d::new(
            allocate_anonymous_handle().unwrap(),
            asset.generation().declaration(),
            incomplete,
            BTreeMap::new(),
        )
        .is_err()
    );
    let override_handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let default_handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let authored = Material2d::from_parameters(
        allocate_anonymous_handle().unwrap(),
        asset.generation().declaration(),
        &typed_parameters(),
        BTreeMap::from([("image.detail".to_owned(), override_handle)]),
    )
    .unwrap();
    let mut resolutions = 0;
    let bound = authored
        .bind(&asset, |reference| {
            resolutions += 1;
            assert_eq!(reference, &AssetRef::Product(image_product()));
            Some(default_handle)
        })
        .unwrap();
    assert_eq!(resolutions, 1);
    assert_eq!(bound.generation(), asset.generation());
    assert_eq!(bound.parameters(), authored.parameters());
    assert_eq!(bound.textures()["image.detail"], override_handle);
    assert_eq!(bound.textures()["image.mask"], default_handle);
    assert!(authored.bind(&asset, |_| None).is_err());
    let incompatible = Material2d::new(
        authored.handle(),
        &declaration("test.other", 2),
        values(),
        BTreeMap::new(),
    )
    .unwrap();
    assert!(
        incompatible
            .bind(&asset, |_| panic!(
                "rejected declarations cannot resolve defaults"
            ))
            .is_err()
    );
}

#[test]
fn deferred_codec_retains_source_scope_and_honors_reference_export_policy() {
    let registry = registry();
    let scope = AssetPathScope::under(AssetSourcePath::new("game/art").unwrap());
    let mut database = ProjectAssetDatabase::default();
    for (reference, path, kind) in [
        (
            material_product(),
            "game/art/materials/surface.material2d",
            AssetSourceKind::Other("material2d".to_owned()),
        ),
        (
            image_product(),
            "game/art/images/detail.png",
            AssetSourceKind::Image,
        ),
    ] {
        database
            .insert(AssetRecord::new(
                reference.source(),
                AssetSourcePath::new(path).unwrap(),
                kind,
            ))
            .unwrap();
    }
    let value = record().to_component_value().unwrap();
    let mut context =
        ComponentDecodeContext::new().with_asset_resolution(database.resolver(&scope));
    let prepared = registry
        .preflight_component_with_context(
            &ComponentTypeId::new("nara.material2d"),
            &value,
            &mut context,
        )
        .unwrap();
    assert!(!context.asset_server_touched());
    let mut world = World::new();
    assert!(!world.contains_resource::<AssetServer>());
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();
    let material = world.get::<Material2d>(entity).unwrap();
    assert_eq!(
        world
            .resource::<AssetServer>()
            .path(material.handle().id())
            .unwrap()
            .as_str(),
        "game/art/materials/surface.material2d"
    );
    let encode = ComponentEncodeContext::new().with_asset_path_scope(&scope);
    let encoded = registry
        .encode_component_with_context(
            &ComponentTypeId::new("nara.material2d"),
            &world,
            entity,
            &encode,
        )
        .unwrap()
        .unwrap();
    assert_eq!(encoded, value);
    let stable = encode.with_asset_ref_export_policy(AssetRefExportPolicy::StableIdWhenKnown);
    let encoded = registry
        .encode_component_with_context(
            &ComponentTypeId::new("nara.material2d"),
            &world,
            entity,
            &stable,
        )
        .unwrap()
        .unwrap();
    let restored = Material2dRecord::from_component_value(&encoded).unwrap();
    assert_eq!(
        restored.material(),
        &AssetRef::StableId(material_product().source())
    );
    assert_eq!(
        restored.texture_overrides()["image.detail"],
        AssetRef::StableId(image_product().source())
    );
}

#[test]
fn processed_codec_uses_existing_typed_products_and_refuses_wrong_source_kinds() {
    let registry = registry();
    let processed_record = Material2dRecord::new(
        AssetRef::Product(material_product()),
        &declaration("test.surface", 2),
        values(),
        BTreeMap::from([(
            "image.detail".to_owned(),
            AssetRef::Product(image_product()),
        )]),
    )
    .unwrap();
    let value = processed_record.to_component_value().unwrap();
    let mut server = AssetServer::new();
    let handle = server
        .reserve_product::<Material2dAsset>(material_product())
        .unwrap();
    let image = server
        .reserve_product::<ImageAsset>(image_product())
        .unwrap();
    let prepared = registry
        .preflight_component_with_context(
            &ComponentTypeId::new("nara.material2d"),
            &value,
            &mut ComponentDecodeContext::with_asset_server(&mut server),
        )
        .unwrap();
    let mut world = World::new();
    world.insert_resource(server);
    let entity = world.spawn_empty().id();
    prepared.apply(&mut world, entity).unwrap();
    let component = world.get::<Material2d>(entity).unwrap();
    assert_eq!(component.handle(), handle);
    assert_eq!(component.texture_overrides()["image.detail"], image);
    assert_eq!(
        registry
            .encode_component(&ComponentTypeId::new("nara.material2d"), &world, entity)
            .unwrap()
            .unwrap(),
        value
    );
    let mut wrong_type = AssetServer::new();
    wrong_type
        .reserve_product::<ImageAsset>(material_product())
        .unwrap();
    assert!(
        registry
            .preflight_component_with_context(
                &ComponentTypeId::new("nara.material2d"),
                &value,
                &mut ComponentDecodeContext::with_asset_server(&mut wrong_type),
            )
            .is_err()
    );
    let mut wrong_kind = ProjectAssetDatabase::default();
    wrong_kind
        .insert(AssetRecord::new(
            material_product().source(),
            AssetSourcePath::new("materials/surface.material2d").unwrap(),
            AssetSourceKind::Image,
        ))
        .unwrap();
    assert!(
        registry
            .preflight_component_with_context(
                &ComponentTypeId::new("nara.material2d"),
                &record().to_component_value().unwrap(),
                &mut ComponentDecodeContext::new()
                    .with_asset_resolution(wrong_kind.resolver(&AssetPathScope::ROOT)),
            )
            .is_err()
    );
}
