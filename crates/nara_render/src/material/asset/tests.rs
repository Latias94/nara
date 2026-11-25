use std::{convert::Infallible, mem::size_of, sync::Arc};

use nara_asset::{
    AssetDomain, AssetImportError, AssetImportSource, AssetMeta, AssetMetaIdentity,
    AssetMetaIdentityCandidate, AssetPath, AssetReferenceDemand, AssetSourceKind,
    ImportedProductId, RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
    allocate_anonymous_handle,
};
use nara_core::ByteLimit;
use serde_json::{Value, json};

use super::*;
use crate::material::{
    AlphaMode2d, MaterialParameterValue, MaterialParameters, MaterialShaderInput,
    MaterialTextureDeclaration, Vec2, Vec3, Vec4,
};

const MATERIAL_SOURCE: &str = "919617ea-c89c-4f83-9cce-b084de435a83";
const PALETTE_SOURCE: &str = "7214561b-ae28-454f-888b-fd049235d212";
const DETAIL_SOURCE: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const MASK_SOURCE: &str = "61ff338c-385e-4586-b87b-cbf698b5ec14";
const AUXILIARY: &str = "011a8cc0-343a-4b95-8adb-3aa6d81977ed";
const HELPER: &str = "fn shared_tone(color: vec4<f32>) -> vec4<f32> { return color * 0.5; }";
const FRAGMENT: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette = textureLoad(palette_texture, vec2<i32>(0, 0), 0);
    return shared_tone(sampled * tint * material_parameters.color + palette);
}
"#;

#[derive(MaterialParameters)]
#[material(crate = crate::material)]
struct Parameters {
    #[material(id = "surface.exposure")]
    exposure: f32,
    #[material(id = "palette.index")]
    index: i32,
    #[material(id = "surface.flags")]
    flags: u32,
    #[material(id = "surface.scale")]
    scale: Vec2,
    #[material(id = "surface.bias")]
    bias: Vec3,
    #[material(id = "surface.color")]
    color: Vec4,
}

fn reference(source: &str, product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(source).unwrap(), product).unwrap()
}

fn auxiliary() -> ImportedProductId {
    ImportedProductId::parse_subproduct(AUXILIARY).unwrap()
}

fn declaration() -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<Parameters>(
        "surface.example",
        3,
        AlphaMode2d::Blend,
        vec![
            MaterialTextureDeclaration::new("palette.map", "palette_texture", 1).unwrap(),
            MaterialTextureDeclaration::new("detail.map", "detail_texture", 3).unwrap(),
            MaterialTextureDeclaration::new("mask.map", "mask_texture", 8).unwrap(),
        ],
    )
    .unwrap()
}

fn parameters() -> MaterialParameterValues {
    Parameters {
        exposure: -0.0,
        index: -7,
        flags: u32::MAX,
        scale: Vec2::new(0.5, 2.0),
        bias: Vec3::new(-1.0, 0.0, 1.0),
        color: Vec4::new(0.25, 0.5, 0.75, 1.0),
    }
    .parameter_values()
    .unwrap()
}

fn source() -> Material2dSource {
    Material2dSource::new(
        declaration(),
        vec![
            AssetPath::new("shaders/shared.wgsl").unwrap(),
            AssetPath::new("shaders/surface.wgsl").unwrap(),
        ],
        parameters(),
        BTreeMap::from([
            (
                "palette.map".to_owned(),
                AssetRef::path("images/palette.png").unwrap(),
            ),
            (
                "detail.map".to_owned(),
                AssetRef::stable_id(DETAIL_SOURCE).unwrap(),
            ),
            (
                "mask.map".to_owned(),
                AssetRef::Product(reference(MASK_SOURCE, auxiliary())),
            ),
        ]),
    )
    .unwrap()
}

fn metadata(kind: AssetSourceKind) -> AssetMetaIdentityCandidate {
    let encoded = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(MATERIAL_SOURCE).unwrap(),
            AssetPath::new("materials/surface.material").unwrap(),
            kind,
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap()
}

fn import_with<E>(
    material: &Material2dSource,
    mut read: impl FnMut(&AssetPath, ByteLimit) -> Result<Arc<[u8]>, E>,
) -> Result<Material2dAsset, AssetImportError<E>> {
    let metadata = metadata(AssetSourceKind::Other(
        Material2dDomain::SOURCE_KIND.to_owned(),
    ));
    let mut source =
        AssetImportSource::new(&metadata, material.encode().unwrap().into(), &mut read);
    Material2dDomain::prepare(&mut source)
}

fn import(material: &Material2dSource) -> Material2dAsset {
    import_with(material, |path, _| {
        Ok::<_, Infallible>(Arc::from(if path.as_str() == "shaders/shared.wgsl" {
            HELPER.as_bytes()
        } else {
            FRAGMENT.as_bytes()
        }))
    })
    .unwrap()
}

fn canonical(product: &Material2dAsset) -> Material2dAsset {
    product
        .resolve_texture_references(|reference| {
            Ok::<_, Infallible>(match reference {
                AssetRef::Path(_) => self::reference(PALETTE_SOURCE, ImportedProductId::PRIMARY),
                AssetRef::StableId(id) => {
                    AssetProductRef::new(*id, ImportedProductId::PRIMARY).unwrap()
                }
                AssetRef::Product(reference) => *reference,
            })
        })
        .unwrap()
}

fn runtime_json() -> Value {
    serde_json::from_slice(
        &Material2dRuntimeCodec::encode_runtime(&canonical(&import(&source()))).unwrap(),
    )
    .unwrap()
}

#[test]
fn source_uses_the_real_declaration_and_single_key_semantic_values() {
    let source = source();
    let bytes = source.encode().unwrap();
    assert_eq!(bytes, source.encode().unwrap());
    assert_eq!(Material2dSource::decode(&bytes).unwrap(), source);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(wire["params"]["surface.exposure"], json!({"f32": -0.0}));
    assert_eq!(wire["params"]["palette.index"], json!({"i32": -7}));
    assert_eq!(wire["params"]["surface.flags"], json!({"u32": u32::MAX}));
    assert_eq!(wire["params"]["surface.scale"], json!({"vec2": [0.5, 2.0]}));
    assert_eq!(
        wire["params"]["surface.bias"],
        json!({"vec3": [-1.0, 0.0, 1.0]})
    );
    assert_eq!(
        wire["params"]["surface.color"],
        json!({"vec4": [0.25, 0.5, 0.75, 1.0]})
    );
    for (wire, field) in wire["declaration"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .zip(source.declaration().parameters().fields())
    {
        assert_eq!(wire["id"], field.id());
        assert_eq!(wire["name"], field.name());
        assert_eq!(wire["offset"], field.offset());
    }
    assert_eq!(
        wire["declaration"]["uniform_size"],
        source.declaration().parameters().byte_size()
    );
    assert_eq!(wire["declaration"]["textures"][2]["binding"], 8);
}

#[test]
fn source_constructor_rejects_incomplete_defaults_and_invalid_inputs() {
    let original = source();
    let mut missing = original.default_parameters().clone();
    missing.remove("surface.bias");
    assert!(matches!(
        Material2dSource::new(
            declaration(),
            original.shader_paths().to_vec(),
            missing,
            original.textures().clone()
        ),
        Err(MaterialAssetError::Declaration(
            MaterialDeclarationError::MissingParameter(_)
        ))
    ));
    let mut nonfinite = parameters();
    nonfinite.insert(
        "surface.exposure".to_owned(),
        MaterialParameterValue::F32(f32::NAN),
    );
    assert!(
        Material2dSource::new(
            declaration(),
            original.shader_paths().to_vec(),
            nonfinite,
            original.textures().clone()
        )
        .is_err()
    );
    let mut unknown = parameters();
    unknown.insert("extra".to_owned(), MaterialParameterValue::F32(1.0));
    assert!(
        Material2dSource::new(
            declaration(),
            original.shader_paths().to_vec(),
            unknown,
            original.textures().clone()
        )
        .is_err()
    );
    let mut textures = original.textures().clone();
    textures.remove("mask.map");
    assert!(matches!(
        Material2dSource::new(
            declaration(),
            original.shader_paths().to_vec(),
            parameters(),
            textures
        ),
        Err(MaterialAssetError::TextureFields)
    ));
    for paths in [
        Vec::new(),
        vec![original.shader_paths()[0].clone(); 2],
        vec![original.shader_paths()[0].clone(); MAX_SHADER_INPUTS + 1],
    ] {
        assert!(matches!(
            Material2dSource::new(
                declaration(),
                paths,
                parameters(),
                original.textures().clone()
            ),
            Err(MaterialAssetError::ShaderInputs)
        ));
    }
}

#[test]
fn importer_reads_ordered_actual_sources_and_retains_exact_observations() {
    let material = source();
    let mut reads = Vec::new();
    let product = import_with(&material, |path, limit| {
        reads.push((path.clone(), limit.get()));
        Ok::<_, Infallible>(Arc::from(if reads.len() == 1 {
            HELPER.as_bytes()
        } else {
            FRAGMENT.as_bytes()
        }))
    })
    .unwrap();
    assert_eq!(
        product.product(),
        reference(MATERIAL_SOURCE, ImportedProductId::PRIMARY)
    );
    assert_eq!(product.generation().sequence(), 0);
    assert_eq!(reads[0].0, material.shader_paths()[0]);
    assert_eq!(reads[1].0, material.shader_paths()[1]);
    assert_eq!(reads[1].1, reads[0].1 - HELPER.len());
    for (input, actual) in product.generation().inputs().iter().zip([HELPER, FRAGMENT]) {
        assert_eq!(input.source(), actual);
        assert_eq!(input.digest(), *blake3::hash(actual.as_bytes()).as_bytes());
    }
    assert_eq!(
        product.generation().shader().user_source(),
        format!("{HELPER}\n{FRAGMENT}\n")
    );
}

#[test]
fn importer_preserves_the_exact_typed_reader_failure_and_stops() {
    #[derive(Debug)]
    struct ReadFailure(u32);

    let mut failure = Some(Box::new(ReadFailure(41)));
    let original = std::ptr::from_ref(failure.as_ref().unwrap().as_ref());
    let mut reads = 0;
    let result = import_with(&source(), |_, _| {
        reads += 1;
        if reads == 1 {
            Ok(Arc::from(HELPER.as_bytes()))
        } else {
            Err(failure.take().unwrap())
        }
    });
    let AssetImportError::Source(returned) = result.unwrap_err() else {
        panic!("a caller read failure must not become a domain diagnostic");
    };
    assert_eq!(returned.0, 41);
    assert!(std::ptr::eq(returned.as_ref(), original));
    assert_eq!(reads, 2);
}

#[test]
fn importer_rejects_wrong_metadata_before_reading_shaders() {
    for kind in [
        AssetSourceKind::Image,
        AssetSourceKind::Other("wrong.material".to_owned()),
    ] {
        let metadata = metadata(kind);
        let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, Infallible> {
            panic!("invalid material metadata must reject before any dependency read")
        };
        let mut source = AssetImportSource::new(
            &metadata,
            self::source().encode().unwrap().into(),
            &mut reader,
        );
        assert!(matches!(
            Material2dDomain::prepare(&mut source),
            Err(AssetImportError::Domain(_))
        ));
    }
}

#[test]
fn source_to_canonical_runtime_roundtrip_preserves_exact_artifact_and_identity() {
    let source_product = import(&source());
    assert!(Material2dRuntimeCodec::encode_runtime(&source_product).is_err());
    let canonical = canonical(&source_product);
    assert_eq!(canonical.generation(), source_product.generation());
    let bytes = Material2dRuntimeCodec::encode_runtime(&canonical).unwrap();
    assert_eq!(
        bytes,
        Material2dRuntimeCodec::encode_runtime(&canonical).unwrap()
    );
    let runtime = Material2dRuntimeCodec::decode_runtime(&bytes).unwrap();
    assert_eq!(runtime.product(), canonical.product());
    assert_eq!(
        runtime.generation().declaration(),
        canonical.generation().declaration()
    );
    assert_eq!(
        runtime.generation().shader(),
        canonical.generation().shader()
    );
    assert_eq!(runtime.textures(), canonical.textures());
    assert_eq!(runtime.default_parameters(), canonical.default_parameters());
    assert!(runtime.generation().inputs().is_empty());
    assert_eq!(runtime.generation().sequence(), 0);
    assert_eq!(
        Material2dRuntimeCodec::encode_runtime(&runtime).unwrap(),
        bytes
    );
    let MaterialParameterValue::F32(exposure) = runtime.default_parameters()["surface.exposure"]
    else {
        panic!("the semantic scalar type must survive Runtime decode");
    };
    assert_eq!(exposure.to_bits(), (-0.0_f32).to_bits());
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(wire.get("sequence").is_none());
    assert!(wire.get("inputs").is_none());
    assert!(!wire["user_body"].as_str().unwrap().contains("@vertex"));
}

#[test]
fn canonical_resolution_borrows_original_references_and_propagates_failure() {
    let product = import(&source());
    let expected = product.textures().values().collect::<Vec<_>>();
    let mut visits = 0;
    let error = Box::new(19);
    let original = std::ptr::from_ref(error.as_ref());
    let mut error = Some(error);
    let result = product.resolve_texture_references(|reference| {
        assert!(std::ptr::eq(reference, expected[visits]));
        visits += 1;
        if visits == 2 {
            Err(error.take().unwrap())
        } else {
            Ok(self::reference(PALETTE_SOURCE, ImportedProductId::PRIMARY))
        }
    });
    let returned = result.unwrap_err();
    assert!(std::ptr::eq(returned.as_ref(), original));
    assert_eq!(visits, 2);
    assert!(matches!(
        product.textures()["palette.map"],
        AssetRef::Path(_)
    ));
}

#[test]
fn binding_merges_defaults_and_valid_overrides_without_a_current_lookup() {
    let product = import(&source());
    let palette = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let detail = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mask = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let mut resolves = 0;
    let binding = product
        .bind_values(
            BTreeMap::from([(
                "surface.exposure".to_owned(),
                MaterialParameterValue::F32(2.0),
            )]),
            BTreeMap::from([("palette.map".to_owned(), palette)]),
            |reference| {
                resolves += 1;
                match reference {
                    AssetRef::StableId(_) => Some(detail),
                    AssetRef::Product(_) => Some(mask),
                    AssetRef::Path(_) => panic!("an explicit override must suppress resolution"),
                }
            },
        )
        .unwrap();
    assert_eq!(resolves, 2);
    assert_eq!(binding.generation(), product.generation());
    assert_eq!(
        binding.parameters()["surface.exposure"],
        MaterialParameterValue::F32(2.0)
    );
    assert_eq!(
        binding.parameters()["surface.color"],
        product.default_parameters()["surface.color"]
    );
    assert_eq!(binding.textures()["palette.map"], palette);
    assert_eq!(binding.textures()["detail.map"], detail);
    assert_eq!(binding.textures()["mask.map"], mask);
    assert_eq!(
        binding.uniform_bytes(),
        product
            .generation()
            .declaration()
            .parameters()
            .encode(binding.parameters())
            .unwrap()
    );
}

#[test]
fn binding_validation_rejects_invalid_values_and_texture_ids_without_resolving() {
    let product = import(&source());
    for parameters in [
        BTreeMap::from([("unknown".to_owned(), MaterialParameterValue::F32(1.0))]),
        BTreeMap::from([(
            "surface.exposure".to_owned(),
            MaterialParameterValue::U32(1),
        )]),
        BTreeMap::from([(
            "surface.exposure".to_owned(),
            MaterialParameterValue::F32(f32::INFINITY),
        )]),
    ] {
        assert!(
            product
                .bind_values(parameters, BTreeMap::new(), |_| {
                    panic!("semantic validation must precede resource resolution")
                })
                .is_err()
        );
    }
    assert!(matches!(
        product.validate_binding(&BTreeMap::new(), ["unknown"]),
        Err(MaterialAssetError::UnknownTexture(_))
    ));
    assert!(
        product
            .validate_binding(&BTreeMap::new(), ["palette.map"])
            .is_ok()
    );
    assert!(matches!(
        product.bind_values(BTreeMap::new(), BTreeMap::new(), |_| None),
        Err(MaterialAssetError::UnresolvedTexture(_))
    ));
}

#[test]
fn source_and_runtime_dependency_inventories_are_exact_and_stop_on_failure() {
    let source_product = import(&source());
    let mut visits = 0;
    Material2dDomain::visit_source_dependencies(&source_product, |reference, kind, demand| {
        assert!(std::ptr::eq(
            reference,
            source_product.textures().values().nth(visits).unwrap()
        ));
        assert_eq!(*kind, AssetSourceKind::Image);
        assert_eq!(demand, AssetReferenceDemand::Immediate);
        visits += 1;
        Ok::<_, Infallible>(())
    })
    .unwrap();
    assert_eq!(visits, 3);
    let canonical = canonical(&source_product);
    let bytes = Material2dRuntimeCodec::encode_runtime(&canonical).unwrap();
    let product = Material2dRuntimeCodec::decode_runtime(&bytes).unwrap();
    let mut actual = Vec::new();
    Material2dRuntimeCodec::visit_runtime_dependencies(&product, |reference, demand| {
        actual.push((reference, demand));
        Ok::<_, Infallible>(())
    })
    .unwrap();
    assert_eq!(
        actual,
        product
            .textures()
            .values()
            .map(|reference| (
                reference.as_product().unwrap(),
                AssetReferenceDemand::Immediate
            ))
            .collect::<Vec<_>>()
    );
    for runtime in [false, true] {
        let mut calls = 0;
        let failure = Box::new(29);
        let original = std::ptr::from_ref(failure.as_ref());
        let mut failure = Some(failure);
        let mut visit = || {
            calls += 1;
            Err(failure.take().unwrap())
        };
        let returned = if runtime {
            Material2dRuntimeCodec::visit_runtime_dependencies(&product, |_, _| visit())
                .unwrap_err()
        } else {
            Material2dDomain::visit_source_dependencies(&source_product, |_, _, _| visit())
                .unwrap_err()
        };
        assert!(std::ptr::eq(returned.as_ref(), original));
        assert_eq!(calls, 1);
    }
}

#[test]
fn runtime_membership_uses_both_source_and_product_including_auxiliary_identity() {
    let mut wire = runtime_json();
    let exact = reference(MATERIAL_SOURCE, auxiliary());
    wire["product"] = serde_json::to_value(exact).unwrap();
    let product =
        Material2dRuntimeCodec::decode_runtime(&serde_json::to_vec(&wire).unwrap()).unwrap();
    assert!(Material2dDomain::contains_product(&product, auxiliary()));
    assert!(!Material2dDomain::contains_product(
        &product,
        ImportedProductId::PRIMARY
    ));
    assert!(Material2dRuntimeCodec::contains_runtime_product(
        &product, exact
    ));
    assert!(!Material2dRuntimeCodec::contains_runtime_product(
        &product,
        reference(PALETTE_SOURCE, auxiliary())
    ));
    assert!(!Material2dRuntimeCodec::contains_runtime_product(
        &product,
        reference(MATERIAL_SOURCE, ImportedProductId::PRIMARY)
    ));
}

#[test]
fn importer_uses_actual_naga_for_invalid_missing_and_incompatible_shaders() {
    let unauthorized =
        format!("{FRAGMENT}\n@group(2) @binding(0) var unauthorized: texture_2d<f32>;");
    for fragment in [
        "this is not WGSL",
        "fn helper_only() {}",
        "fn material_fragment() -> vec4<f32> { return vec4<f32>(1.0); }",
        unauthorized.as_str(),
    ] {
        let result = import_with(&source(), |path, _| {
            Ok::<_, Infallible>(Arc::from(if path.as_str() == "shaders/shared.wgsl" {
                HELPER.as_bytes()
            } else {
                fragment.as_bytes()
            }))
        });
        assert!(
            matches!(result, Err(AssetImportError::Domain(_))),
            "{fragment}"
        );
    }
    assert!(matches!(
        import_with(&source(), |_, _| Ok::<_, Infallible>(Arc::from(
            &[0xff][..]
        ))),
        Err(AssetImportError::Domain(_))
    ));
}

#[test]
fn runtime_revalidates_values_texture_sets_declarations_and_naga_interfaces() {
    let original = runtime_json();
    let mut cases = Vec::new();
    let mut wrong = original.clone();
    wrong["params"]["surface.exposure"] = json!({"u32": 1});
    cases.push(wrong);
    let mut missing = original.clone();
    missing["params"]
        .as_object_mut()
        .unwrap()
        .remove("surface.bias");
    cases.push(missing);
    let mut extra = original.clone();
    extra["textures"]["unknown"] = extra["textures"]["palette.map"].clone();
    cases.push(extra);
    let mut missing = original.clone();
    missing["textures"]
        .as_object_mut()
        .unwrap()
        .remove("palette.map");
    cases.push(missing);
    let mut path = original.clone();
    path["textures"]["palette.map"] = json!({"kind": "path", "value": "image.png"});
    cases.push(path);
    let mut size = original.clone();
    size["declaration"]["uniform_size"] = json!(declaration().parameters().byte_size() + 16);
    cases.push(size);
    let mut missing_function = original.clone();
    missing_function["user_body"] = json!(HELPER);
    cases.push(missing_function);
    let mut extra_resource = original.clone();
    extra_resource["user_body"] = json!(format!(
        "{HELPER}\n{FRAGMENT}\n@group(2) @binding(0) var unexpected: texture_2d<f32>;"
    ));
    cases.push(extra_resource);
    let mut assembled = original.clone();
    assembled["user_body"] = json!(import(&source()).generation().shader().source());
    cases.push(assembled);
    for wire in cases {
        assert!(
            Material2dRuntimeCodec::decode_runtime(&serde_json::to_vec(&wire).unwrap()).is_err()
        );
    }
}

#[test]
fn json_preflight_rejects_ambiguity_unknown_fields_wrong_versions_and_records_as_arrays() {
    let original = runtime_json();
    let mut cases = vec![json!([]), Value::Null];
    for (field, value) in [
        ("unknown", json!(true)),
        ("kind", json!("wrong")),
        ("version", json!(2)),
    ] {
        let mut wire = original.clone();
        wire[field] = value;
        cases.push(wire);
    }
    let mut field = original.clone();
    field["declaration"]["fields"][0]["unknown"] = json!(true);
    cases.push(field);
    let mut field = original.clone();
    field["declaration"]["fields"][0] = json!(["id", "exposure", "f32", 0]);
    cases.push(field);
    let mut value = original.clone();
    value["params"]["surface.exposure"] = json!({"f32": 1.0, "u32": 1});
    cases.push(value);
    let mut value = original.clone();
    value["params"]["surface.scale"] = json!({"vec2": [1.0, 2.0, 3.0]});
    cases.push(value);
    let mut nil = original.clone();
    nil["product"]["source"] = json!("00000000-0000-0000-0000-000000000000");
    cases.push(nil);
    for wire in cases {
        assert!(
            Material2dRuntimeCodec::decode_runtime(&serde_json::to_vec(&wire).unwrap()).is_err()
        );
    }
    let encoded = serde_json::to_string(&original).unwrap();
    let duplicate = encoded.replacen("{", "{\"version\":1,", 1);
    assert!(Material2dRuntimeCodec::decode_runtime(duplicate.as_bytes()).is_err());
    assert!(Material2dRuntimeCodec::decode_runtime(format!("{encoded}{{}}").as_bytes()).is_err());
    assert!(Material2dRuntimeCodec::decode_runtime(b"{\"kind\":").is_err());
    let mut source_wire: Value = serde_json::from_slice(&source().encode().unwrap()).unwrap();
    source_wire["unexpected"] = json!(true);
    assert!(Material2dSource::decode(&serde_json::to_vec(&source_wire).unwrap()).is_err());
}

#[test]
fn input_shape_and_dependency_byte_bounds_reject_before_unbounded_work() {
    assert!(matches!(
        Material2dSource::decode(&vec![b' '; MAX_SOURCE_FILE_BYTES + 1]),
        Err(MaterialAssetError::Limit)
    ));
    assert!(
        Material2dRuntimeCodec::decode_runtime(&vec![b' '; MAX_RUNTIME_FILE_BYTES + 1]).is_err()
    );
    let mut wire = runtime_json();
    wire["user_body"] = json!("x".repeat(MAX_SHADER_BYTES + 1));
    assert!(Material2dRuntimeCodec::decode_runtime(&serde_json::to_vec(&wire).unwrap()).is_err());
    let mut wire = runtime_json();
    wire["declaration"]["id"] = json!("x".repeat(MAX_TEXT_BYTES + 1));
    assert!(Material2dRuntimeCodec::decode_runtime(&serde_json::to_vec(&wire).unwrap()).is_err());
    let deep = format!("{}0{}", "[".repeat(13), "]".repeat(13));
    assert!(Material2dRuntimeCodec::decode_runtime(deep.as_bytes()).is_err());
    let result = import_with(&source(), |_, limit| {
        Ok::<_, Infallible>(Arc::from(vec![b' '; limit.get() + 1]))
    });
    assert!(matches!(result, Err(AssetImportError::Domain(_))));
}

#[test]
fn retained_model_charges_original_body_assembled_source_maps_and_actual_source_observations() {
    let source = canonical(&import(&source()));
    let runtime = Material2dRuntimeCodec::decode_runtime(
        &Material2dRuntimeCodec::encode_runtime(&source).unwrap(),
    )
    .unwrap();
    let source_cost = Material2dRuntimeCodec::runtime_retained_bytes(&source).unwrap();
    let runtime_cost = Material2dRuntimeCodec::runtime_retained_bytes(&runtime).unwrap();
    let observations: usize = source
        .generation()
        .inputs()
        .iter()
        .map(|input| {
            size_of::<MaterialShaderInput>()
                + input.path().as_str().len()
                + input.source().len()
                + 2 * size_of::<usize>()
        })
        .sum();
    assert_eq!(source_cost - runtime_cost, observations);
    assert!(
        runtime_cost
            > size_of::<Material2dAsset>()
                + runtime.generation().shader().user_source().len()
                + runtime.generation().shader().source().len()
    );
    assert!(source_cost <= Material2dDomain::limits().retained_bytes().get());
    assert!(
        runtime_cost
            <= Material2dRuntimeCodec::runtime_limits()
                .retained_bytes()
                .get()
    );
    let clone = runtime.clone();
    assert_eq!(
        Material2dRuntimeCodec::runtime_retained_bytes(&clone).unwrap(),
        runtime_cost
    );
}

#[test]
fn pure_generation_factory_has_no_fabricated_inputs_or_local_publication_sequence() {
    let body = format!("{HELPER}\n{FRAGMENT}\n");
    let generation = Material2dGeneration::from_shader_source(declaration(), &body).unwrap();
    assert!(generation.inputs().is_empty());
    assert_eq!(generation.sequence(), 0);
    assert_eq!(generation.shader().user_source(), body);
    let imported = import(&source());
    assert_eq!(generation.shader(), imported.generation().shader());
}

#[test]
fn bindings_retain_original_source_and_runtime_acquisitions_without_an_owner_cycle() {
    struct Acquisition {
        _original: Material2dAsset,
    }

    let imported = canonical(&import(&source()));
    let decoded = Material2dRuntimeCodec::decode_runtime(
        &Material2dRuntimeCodec::encode_runtime(&imported).unwrap(),
    )
    .unwrap();
    for original in [imported, decoded] {
        let owner = Arc::new(Acquisition {
            _original: original.clone(),
        });
        let weak = Arc::downgrade(&owner);
        let retained = original.with_lifetime_owner(Arc::clone(&owner));
        assert_eq!(retained.generation(), original.generation());
        assert!(std::ptr::eq(
            retained.default_parameters(),
            original.default_parameters()
        ));
        assert!(std::ptr::eq(retained.textures(), original.textures()));
        assert!(Material2dRuntimeCodec::runtime_retained_bytes(&retained).is_err());
        let handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
        let binding = retained
            .bind_values(BTreeMap::new(), BTreeMap::new(), |_| Some(handle))
            .unwrap();
        let generation = binding.generation().clone();
        drop(owner);
        drop(retained);
        drop(original);
        assert!(weak.upgrade().is_some());
        drop(binding);
        assert!(weak.upgrade().is_some());
        assert!(generation.shader().source().contains("material_fragment"));
        drop(generation);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn attaching_another_lifetime_owner_keeps_both_original_charges_and_generation_identity() {
    let original = import(&source());
    let retention_bytes = original
        .generation()
        .lifetime_owner_retained_bytes()
        .unwrap();
    let first = Arc::new(());
    let second = Arc::new(());
    let weak_first = Arc::downgrade(&first);
    let weak_second = Arc::downgrade(&second);
    let once = original
        .generation()
        .with_lifetime_owner(Arc::clone(&first));
    let twice = once.with_lifetime_owner(Arc::clone(&second));
    assert_eq!(&twice, original.generation());
    assert_eq!(twice.shader(), original.generation().shader());
    drop(first);
    drop(second);
    drop(once);
    drop(original);
    assert!(weak_first.upgrade().is_some());
    assert!(weak_second.upgrade().is_some());
    drop(twice);
    assert!(weak_first.upgrade().is_none());
    assert!(weak_second.upgrade().is_none());
    assert!(retention_bytes > 5 * size_of::<usize>());
}

#[test]
fn importer_rejects_unadmitted_settings_without_shader_reads() {
    let meta = metadata(AssetSourceKind::Other(
        Material2dDomain::SOURCE_KIND.to_owned(),
    ));
    let encoded = AssetMeta::new(meta.identity().clone(), json!({ "unknown": true }))
        .to_json_string()
        .unwrap();
    let meta = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, Infallible> {
        panic!("unit settings admission must precede Shader input reads")
    };
    let mut input = AssetImportSource::new(&meta, source().encode().unwrap().into(), &mut reader);
    assert!(matches!(
        Material2dDomain::prepare(&mut input),
        Err(AssetImportError::Domain(_))
    ));
}

struct SnapshotAcquisition {
    _original: Material2dAsset,
}

#[test]
fn escaped_shader_snapshots_retain_original_source_and_runtime_acquisitions() {
    let imported = canonical(&import(&source()));
    let decoded = Material2dRuntimeCodec::decode_runtime(
        &Material2dRuntimeCodec::encode_runtime(&imported).unwrap(),
    )
    .unwrap();
    for original in [imported, decoded] {
        let owner = Arc::new(SnapshotAcquisition {
            _original: original.clone(),
        });
        let weak = Arc::downgrade(&owner);
        let retained = original.with_lifetime_owner(Arc::clone(&owner));
        let handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
        let binding = retained
            .bind_values(BTreeMap::new(), BTreeMap::new(), |_| Some(handle))
            .unwrap();
        let generation = binding.generation().clone();
        let shader = generation.shader().clone();
        assert_eq!(&generation, original.generation());
        assert_eq!(&shader, original.generation().shader());
        assert!(std::ptr::eq(
            shader.source(),
            original.generation().shader().source()
        ));
        assert!(std::ptr::eq(
            shader.user_source(),
            original.generation().shader().user_source()
        ));
        drop(owner);
        drop(original);
        drop(retained);
        drop(binding);
        drop(generation);
        assert!(weak.upgrade().is_some());
        assert!(shader.source().contains("material_fragment"));
        assert!(shader.user_source().contains(HELPER));
        let clone = shader.clone();
        drop(shader);
        assert!(weak.upgrade().is_some());
        drop(clone);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn escaped_imported_input_snapshots_retain_the_original_source_acquisition() {
    let original = import(&source());
    let owner = Arc::new(SnapshotAcquisition {
        _original: original.clone(),
    });
    let weak = Arc::downgrade(&owner);
    let retained = original.with_lifetime_owner(Arc::clone(&owner));
    let handle = allocate_anonymous_handle::<ImageAsset>().unwrap();
    let binding = retained
        .bind_values(BTreeMap::new(), BTreeMap::new(), |_| Some(handle))
        .unwrap();
    let generation = binding.generation().clone();
    let input = generation.inputs()[0].clone();
    assert_eq!(&generation, original.generation());
    assert_eq!(&input, &original.generation().inputs()[0]);
    assert!(std::ptr::eq(
        input.source(),
        original.generation().inputs()[0].source()
    ));
    drop(owner);
    drop(original);
    drop(retained);
    drop(binding);
    drop(generation);
    assert!(weak.upgrade().is_some());
    assert_eq!(input.path().as_str(), "shaders/shared.wgsl");
    assert_eq!(input.source(), HELPER);
    assert_eq!(input.digest(), *blake3::hash(HELPER.as_bytes()).as_bytes());
    let clone = input.clone();
    drop(input);
    assert!(weak.upgrade().is_some());
    drop(clone);
    assert!(weak.upgrade().is_none());
}

#[test]
fn escaped_snapshots_chain_all_lifetime_owners_without_a_cycle_or_identity_change() {
    let original = import(&source());
    let mut retained = original.clone();
    let mut owners = Vec::new();
    let retention_bytes = original
        .generation()
        .lifetime_owner_retained_bytes()
        .unwrap();
    for _ in 0..3 {
        let owner = Arc::new(SnapshotAcquisition {
            _original: original.clone(),
        });
        owners.push(Arc::downgrade(&owner));
        let next = retained.with_lifetime_owner(owner);
        assert_eq!(next.generation(), original.generation());
        assert_eq!(next.generation().shader(), retained.generation().shader());
        assert_eq!(next.generation().inputs(), retained.generation().inputs());
        assert_eq!(
            next.generation().lifetime_owner_retained_bytes(),
            Some(retention_bytes)
        );
        retained = next;
    }
    let shader = retained.generation().shader().clone();
    let input = retained.generation().inputs()[1].clone();
    drop(original);
    drop(retained);
    assert!(owners.iter().all(|owner| owner.upgrade().is_some()));
    drop(shader);
    assert!(owners.iter().all(|owner| owner.upgrade().is_some()));
    assert_eq!(input.source(), FRAGMENT);
    drop(input);
    assert!(owners.iter().all(|owner| owner.upgrade().is_none()));
}

#[test]
fn lifetime_owner_model_includes_shared_view_input_storage_and_cloned_path_lengths() {
    let imported = canonical(&import(&source()));
    let runtime = Material2dRuntimeCodec::decode_runtime(
        &Material2dRuntimeCodec::encode_runtime(&imported).unwrap(),
    )
    .unwrap();
    assert!(runtime.generation().inputs().is_empty());
    let base = runtime
        .generation()
        .lifetime_owner_retained_bytes()
        .unwrap();
    let owner = size_of::<(Option<Arc<dyn std::any::Any + Send + Sync>>, Arc<()>)>();
    let view = size_of::<(
        Arc<dyn std::any::Any + Send + Sync>,
        crate::material::MaterialShaderArtifact,
        Box<[MaterialShaderInput]>,
    )>();
    assert_eq!(base, owner + view + 4 * size_of::<usize>());
    let inputs = imported.generation().inputs();
    let paths: usize = inputs.iter().map(|input| input.path().as_str().len()).sum();
    let imported_bytes = imported
        .generation()
        .lifetime_owner_retained_bytes()
        .unwrap();
    assert_eq!(imported_bytes, base + std::mem::size_of_val(inputs) + paths);
    let original_source = source();
    let longer_path =
        AssetPath::new(format!("shaders/{}surface.wgsl", "nested/".repeat(32))).unwrap();
    let longer_source = Material2dSource::new(
        original_source.declaration().clone(),
        vec![
            original_source.shader_paths()[0].clone(),
            longer_path.clone(),
        ],
        original_source.default_parameters().clone(),
        original_source.textures().clone(),
    )
    .unwrap();
    let longer = import(&longer_source);
    assert_eq!(longer.generation().shader(), imported.generation().shader());
    assert_eq!(
        longer.generation().lifetime_owner_retained_bytes().unwrap() - imported_bytes,
        longer_path.as_str().len() - original_source.shader_paths()[1].as_str().len()
    );
    assert!(imported_bytes < Material2dDomain::limits().retained_bytes().get());
}
