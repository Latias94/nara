use std::{collections::BTreeMap, sync::Arc};

use glam::{Vec2, Vec3, Vec4};
use nara_asset::{AssetPath, Handle, allocate_anonymous_handle};
use nara_render::image::ImageAsset;
use nara_render::material::{
    AlphaMode2d, Material2dDeclaration, Material2dLibrary, MaterialDeclarationError,
    MaterialParameterKind, MaterialParameterValue, MaterialParameterValues, MaterialParameters,
    MaterialShaderError, MaterialTextureDeclaration, MaterialUpdateError,
};

#[derive(Clone, Debug, PartialEq, MaterialParameters)]
#[material(crate = "nara_render::material")]
struct PaletteParameters {
    #[material(id = "palette.colors")]
    colors: Vec4,
}

#[derive(Clone, Debug, PartialEq, MaterialParameters)]
#[material(crate = "nara_render::material")]
struct SurfaceParameters {
    #[material(id = "surface.strength")]
    strength: f32,
    #[material(id = "surface.uv-scale")]
    uv_scale: Vec2,
    #[material(id = "surface.bias")]
    bias: Vec3,
    #[material(id = "surface.opacity")]
    opacity: f32,
}

type ArrayLength = u32;

#[derive(Clone, Debug, PartialEq, MaterialParameters)]
#[material(crate = "nara_render::material")]
struct EncoderParameters {
    #[material(id = "encoder.weight")]
    reader: f32,
    #[material(id = "encoder.tail")]
    tail: u32,
}

#[derive(Clone, Debug, PartialEq, MaterialParameters)]
#[material(crate = "nara_render::material")]
struct AliasedParameters {
    #[material(id = "encoder.count")]
    count: ArrayLength,
}

#[test]
fn supported_field_names_and_aliases_use_semantic_reconstruction() {
    let parameters = EncoderParameters {
        reader: -0.0,
        tail: 7,
    };
    let values = parameters.parameter_values().unwrap();
    let restored = EncoderParameters::from_parameter_values(&values).unwrap();
    assert_eq!(restored.reader.to_bits(), (-0.0_f32).to_bits());
    assert_eq!(restored.tail, 7);
    let bytes = parameters.encode_uniform().unwrap();
    assert_eq!(&bytes[..4], &(-0.0_f32).to_le_bytes());
    assert_eq!(&bytes[4..8], &7_u32.to_le_bytes());
    assert_eq!(
        EncoderParameters::parameter_layout()
            .unwrap()
            .encode(&values)
            .unwrap(),
        bytes
    );
    let aliased = AliasedParameters { count: 11 };
    let values = aliased.parameter_values().unwrap();
    assert_eq!(
        AliasedParameters::from_parameter_values(&values).unwrap(),
        aliased
    );
    assert_eq!(aliased.encode_uniform().unwrap(), 11_u32.to_le_bytes());
}

const PALETTE_FRAGMENT: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette_color = textureLoad(palette_texture, vec2<i32>(0, 0), 0);
    return sampled * tint * material_parameters.colors + palette_color * 0.25;
}
"#;

const SURFACE_FRAGMENT: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let noise = textureLoad(noise_texture, vec2<i32>(0, 0), 0);
    let mask = textureLoad(mask_texture, vec2<i32>(0, 0), 0);
    let scaled_uv = uv * material_parameters.uv_scale;
    let color = sampled.rgb * material_parameters.strength + material_parameters.bias;
    let detail = noise.rgb * scaled_uv.x + mask.rgb * scaled_uv.y;
    return vec4<f32>(color + detail, sampled.a * material_parameters.opacity) * tint;
}
"#;

const SHARED_HELPER: &str = r#"
fn shared_tone(color: vec4<f32>) -> vec4<f32> {
    return color * 0.5;
}
"#;

const SHARED_FRAGMENT: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette_color = textureLoad(palette_texture, vec2<i32>(0, 0), 0);
    return shared_tone(sampled * tint * material_parameters.colors + palette_color);
}
"#;

fn asset_path(path: &str) -> AssetPath {
    AssetPath::new(path).unwrap()
}

fn palette_declaration(id: &str) -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<PaletteParameters>(
        id,
        1,
        AlphaMode2d::Blend,
        vec![MaterialTextureDeclaration::new("image.palette", "palette_texture", 1).unwrap()],
    )
    .unwrap()
}

fn surface_declaration(id: &str) -> Material2dDeclaration {
    Material2dDeclaration::from_parameters::<SurfaceParameters>(
        id,
        3,
        AlphaMode2d::Blend,
        vec![
            MaterialTextureDeclaration::new("image.noise", "noise_texture", 1).unwrap(),
            MaterialTextureDeclaration::new("image.mask", "mask_texture", 2).unwrap(),
        ],
    )
    .unwrap()
}

fn read_source(
    sources: &BTreeMap<AssetPath, Arc<str>>,
    path: &AssetPath,
) -> Result<Arc<str>, String> {
    sources
        .get(path)
        .cloned()
        .ok_or_else(|| format!("missing in-memory source: {path}"))
}

fn decode_floats(bytes: &[u8]) -> Vec<f32> {
    assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|word| f32::from_le_bytes(word.try_into().unwrap()))
        .collect()
}

#[test]
fn retained_palette_survives_newer_publication_failed_shader_and_library_drop() {
    let id = "workflow.palette";
    let path = asset_path("materials/palette.wgsl");
    let manifest = [path.clone()];
    let mut sources = BTreeMap::from([(path.clone(), Arc::<str>::from(PALETTE_FRAGMENT))]);
    let declaration = palette_declaration(id);
    let mut library = Material2dLibrary::default();
    library.declare(declaration.clone()).unwrap();

    let prepared_a = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    assert!(library.current(id).is_none());
    let captured_a = library
        .publish(&prepared_a, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(prepared_a.candidate(), &captured_a);
    assert_eq!(captured_a.declaration(), &declaration);
    assert_eq!(captured_a.inputs()[0].source(), PALETTE_FRAGMENT);
    let shader_a = captured_a.shader().clone();
    assert!(shader_a.source().contains(PALETTE_FRAGMENT));

    let mut parameters = PaletteParameters {
        colors: Vec4::new(0.125, 0.25, 0.5, 1.0),
    };
    let original_parameters = parameters.clone();
    let original_values = MaterialParameterValues::from([(
        "palette.colors".to_owned(),
        MaterialParameterValue::Vec4([0.125, 0.25, 0.5, 1.0]),
    )]);
    let image_a: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let textures_a = BTreeMap::from([("image.palette".to_owned(), image_a)]);
    let typed_a = captured_a.bind(&parameters, textures_a.clone()).unwrap();
    let dynamic_a = captured_a
        .bind_values(original_values.clone(), textures_a.clone())
        .unwrap();
    assert_eq!(typed_a, dynamic_a);
    assert_eq!(typed_a.parameters(), &original_values);
    assert_eq!(typed_a.textures(), &textures_a);
    let expected_bytes: Vec<_> = [0.125_f32, 0.25, 0.5, 1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(typed_a.uniform_bytes(), expected_bytes.as_slice());
    assert_eq!(
        PaletteParameters::from_parameter_values(typed_a.parameters()).unwrap(),
        original_parameters
    );
    assert_eq!(
        Vec4::from_slice(&decode_floats(typed_a.uniform_bytes())),
        original_parameters.colors
    );
    let late_a = captured_a.clone();

    let source_b1 = Arc::<str>::from(PALETTE_FRAGMENT.replace("0.25", "0.5"));
    sources.insert(path.clone(), source_b1.clone());
    let prepared_b1 = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    let source_b2 = Arc::<str>::from(PALETTE_FRAGMENT.replace("0.25", "0.75"));
    sources.insert(path.clone(), source_b2.clone());
    let prepared_b2 = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(library.current(id), Some(&captured_a));
    let published_b = library
        .publish(&prepared_b2, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(library.current(id), Some(&published_b));
    assert!(published_b.sequence() > captured_a.sequence());
    assert_eq!(published_b.declaration(), captured_a.declaration());
    assert_eq!(published_b.inputs()[0].source(), source_b2.as_ref());
    let shader_b = published_b.shader().clone();
    assert!(shader_b.source().contains(source_b2.as_ref()));
    assert_ne!(shader_a.digest(), shader_b.digest());
    assert_eq!(shader_a.interface_digest(), shader_b.interface_digest());
    assert_eq!(
        prepared_b1.candidate().inputs()[0].source(),
        source_b1.as_ref()
    );

    let mut stale_reads = 0;
    assert_eq!(
        library.publish(&prepared_b1, |path| {
            stale_reads += 1;
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::StaleUpdate)
    );
    assert_eq!(stale_reads, 0);
    assert_eq!(library.current(id), Some(&published_b));

    let incompatible_source =
        Arc::<str>::from(PALETTE_FRAGMENT.replace("uv: vec2<f32>", "uv: vec3<f32>"));
    sources.insert(path.clone(), incompatible_source.clone());
    let failure = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap_err();
    assert!(matches!(&failure, MaterialUpdateError::Shader(_)));
    assert_eq!(library.last_failure(id), Some(&failure));
    assert_eq!(library.current(id), Some(&published_b));
    let observations = library.observations(id).unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].path(), &path);
    assert_eq!(
        observations[0].digest(),
        Some(*blake3::hash(incompatible_source.as_bytes()).as_bytes())
    );
    assert_eq!(published_b.inputs()[0].source(), source_b2.as_ref());

    parameters.colors = Vec4::new(0.75, 0.5, 0.25, 0.125);
    let image_b: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let textures_b = BTreeMap::from([("image.palette".to_owned(), image_b)]);
    let updated_b = published_b.bind(&parameters, textures_b.clone()).unwrap();
    assert_eq!(updated_b.generation(), &published_b);
    assert_eq!(updated_b.generation().shader(), &shader_b);
    assert_eq!(updated_b.textures(), &textures_b);
    assert_ne!(updated_b.uniform_bytes(), typed_a.uniform_bytes());
    assert_ne!(image_a, image_b);
    assert_eq!(typed_a.parameters(), &original_values);
    assert_eq!(typed_a.textures(), &textures_a);
    assert_eq!(library.current(id), Some(&published_b));

    drop(library);
    drop(prepared_a);
    drop(prepared_b1);
    drop(prepared_b2);
    drop(captured_a);
    drop(sources);
    let late_binding = late_a
        .bind_values(original_values.clone(), textures_a.clone())
        .unwrap();
    assert_eq!(late_binding, typed_a);
    assert_eq!(
        late_a
            .bind(&original_parameters, textures_a.clone())
            .unwrap(),
        typed_a
    );
    assert_eq!(late_binding.parameters(), &original_values);
    assert_eq!(late_binding.textures(), &textures_a);
    assert_eq!(late_binding.uniform_bytes(), expected_bytes.as_slice());
    assert_eq!(
        late_binding.generation().inputs()[0].source(),
        PALETTE_FRAGMENT
    );
    assert_eq!(late_binding.generation().declaration(), &declaration);
    assert_eq!(late_binding.generation().shader(), &shader_a);
}

#[test]
fn mixed_layout_round_trips_exact_bytes_and_images_without_republishing() {
    let id = "workflow.surface";
    let path = asset_path("materials/surface.wgsl");
    let sources = BTreeMap::from([(path.clone(), Arc::<str>::from(SURFACE_FRAGMENT))]);
    let declaration = surface_declaration(id);
    let palette = palette_declaration("workflow.palette");
    assert_eq!(palette.parameters().byte_size(), 16);
    assert_eq!(palette.textures().len(), 1);
    assert_eq!(declaration.parameters().byte_size(), 32);
    assert_eq!(declaration.textures().len(), 2);
    assert_ne!(palette.binding_digest(), declaration.binding_digest());
    assert_eq!(
        declaration
            .parameters()
            .fields()
            .iter()
            .map(|field| (field.id(), field.name(), field.kind(), field.offset()))
            .collect::<Vec<_>>(),
        [
            (
                "surface.strength",
                "strength",
                MaterialParameterKind::F32,
                0
            ),
            (
                "surface.uv-scale",
                "uv_scale",
                MaterialParameterKind::Vec2,
                8
            ),
            ("surface.bias", "bias", MaterialParameterKind::Vec3, 16),
            ("surface.opacity", "opacity", MaterialParameterKind::F32, 28),
        ]
    );

    let mut library = Material2dLibrary::default();
    library.declare(declaration).unwrap();
    let prepared = library
        .prepare_update(id, &[path], |path| read_source(&sources, path))
        .unwrap();
    let generation = library
        .publish(&prepared, |path| read_source(&sources, path))
        .unwrap();
    let parameters = SurfaceParameters {
        strength: 1.25,
        uv_scale: Vec2::new(-0.0, 2.0),
        bias: Vec3::new(-3.5, 0.25, -0.0),
        opacity: 0.75,
    };
    let values = MaterialParameterValues::from([
        (
            "surface.opacity".to_owned(),
            MaterialParameterValue::F32(0.75),
        ),
        (
            "surface.bias".to_owned(),
            MaterialParameterValue::Vec3([-3.5, 0.25, -0.0]),
        ),
        (
            "surface.uv-scale".to_owned(),
            MaterialParameterValue::Vec2([-0.0, 2.0]),
        ),
        (
            "surface.strength".to_owned(),
            MaterialParameterValue::F32(1.25),
        ),
    ]);
    let noise: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let mask: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let textures = BTreeMap::from([
        ("image.noise".to_owned(), noise),
        ("image.mask".to_owned(), mask),
    ]);
    let typed = generation.bind(&parameters, textures.clone()).unwrap();
    let dynamic = generation
        .bind_values(values.clone(), textures.clone())
        .unwrap();
    assert_eq!(typed, dynamic);
    assert_eq!(typed.parameters(), &values);
    assert_eq!(typed.textures(), &textures);
    let expected_words = [1.25_f32, 0.0, -0.0, 2.0, -3.5, 0.25, -0.0, 0.75];
    let expected_bytes: Vec<_> = expected_words
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(typed.uniform_bytes(), expected_bytes.as_slice());
    let decoded = decode_floats(typed.uniform_bytes());
    assert_eq!(decoded, expected_words);
    assert_eq!(decoded[2].to_bits(), (-0.0_f32).to_bits());
    assert_eq!(decoded[6].to_bits(), (-0.0_f32).to_bits());
    assert_eq!(
        SurfaceParameters {
            strength: decoded[0],
            uv_scale: Vec2::new(decoded[2], decoded[3]),
            bias: Vec3::new(decoded[4], decoded[5], decoded[6]),
            opacity: decoded[7],
        },
        parameters
    );
    let reconstructed = SurfaceParameters::from_parameter_values(dynamic.parameters()).unwrap();
    assert_eq!(reconstructed, parameters);
    assert_eq!(reconstructed.encode_uniform().unwrap(), expected_bytes);
    assert_eq!(
        generation.bind(&PaletteParameters { colors: Vec4::ONE }, textures.clone()),
        Err(MaterialUpdateError::ParameterLayout)
    );

    let replacement_noise: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let mut updated_textures = textures.clone();
    updated_textures.insert("image.noise".to_owned(), replacement_noise);
    let updated_parameters = SurfaceParameters {
        strength: 2.5,
        ..parameters.clone()
    };
    let updated = generation
        .bind(&updated_parameters, updated_textures.clone())
        .unwrap();
    assert_eq!(updated.generation(), typed.generation());
    assert_eq!(updated.generation().shader(), typed.generation().shader());
    assert_ne!(updated.uniform_bytes(), typed.uniform_bytes());
    assert_eq!(updated.textures(), &updated_textures);
    assert_eq!(typed.textures().get("image.noise"), Some(&noise));
    assert_eq!(typed.uniform_bytes(), expected_bytes.as_slice());

    let non_finite = SurfaceParameters {
        strength: f32::NAN,
        ..parameters
    };
    assert_eq!(
        generation.bind(&non_finite, textures.clone()),
        Err(MaterialUpdateError::Declaration(
            MaterialDeclarationError::NonFiniteParameter("surface.strength".to_owned())
        ))
    );
    let mut non_finite_values = values.clone();
    non_finite_values.insert(
        "surface.bias".to_owned(),
        MaterialParameterValue::Vec3([0.0, f32::INFINITY, 0.0]),
    );
    assert_eq!(
        generation.bind_values(non_finite_values, textures.clone()),
        Err(MaterialUpdateError::Declaration(
            MaterialDeclarationError::NonFiniteParameter("surface.bias".to_owned())
        ))
    );
    assert_eq!(library.current(id), Some(&generation));
    assert_eq!(typed.parameters(), &values);
    assert_eq!(typed.textures(), &textures);
    assert_eq!(typed.uniform_bytes(), expected_bytes.as_slice());
}

#[test]
fn shared_dependencies_keep_failed_observations_until_repaired_closure_replaces_old_edge() {
    let left = "workflow.palette.left";
    let right = "workflow.palette.right";
    let unrelated = "workflow.palette.unrelated";
    let helper_path = asset_path("materials/z_shared.wgsl");
    let left_path = asset_path("materials/a_left.wgsl");
    let right_path = asset_path("materials/b_right.wgsl");
    let unrelated_path = asset_path("materials/c_unrelated.wgsl");
    let constants_path = asset_path("materials/repair/constants.wgsl");
    let replacement_helper = asset_path("materials/repair/helper.wgsl");
    let mut sources = BTreeMap::from([
        (helper_path.clone(), Arc::<str>::from(SHARED_HELPER)),
        (left_path.clone(), Arc::<str>::from(SHARED_FRAGMENT)),
        (right_path.clone(), Arc::<str>::from(SHARED_FRAGMENT)),
        (unrelated_path.clone(), Arc::<str>::from(PALETTE_FRAGMENT)),
        (
            constants_path.clone(),
            Arc::<str>::from("const palette_exposure: f32 = 0.75;"),
        ),
    ]);
    let mut library = Material2dLibrary::default();
    for id in [left, right, unrelated] {
        library.declare(palette_declaration(id)).unwrap();
    }
    for (id, entry_path) in [(left, left_path.clone()), (right, right_path.clone())] {
        let manifest = [helper_path.clone(), entry_path];
        let mut reads = Vec::new();
        let prepared = library
            .prepare_update(id, &manifest, |path| {
                reads.push(path.clone());
                read_source(&sources, path)
            })
            .unwrap();
        assert_eq!(reads, manifest);
        let generation = library
            .publish(&prepared, |path| read_source(&sources, path))
            .unwrap();
        assert_eq!(
            generation
                .inputs()
                .iter()
                .map(|input| input.path())
                .collect::<Vec<_>>(),
            manifest.iter().collect::<Vec<_>>()
        );
        assert_eq!(generation.inputs()[0].source(), SHARED_HELPER);
        assert_eq!(generation.inputs()[1].source(), SHARED_FRAGMENT);
        let assembled = generation.shader().source();
        assert!(assembled.find(SHARED_HELPER).unwrap() < assembled.find(SHARED_FRAGMENT).unwrap());
    }
    let prepared_unrelated = library
        .prepare_update(unrelated, std::slice::from_ref(&unrelated_path), |path| {
            read_source(&sources, path)
        })
        .unwrap();
    let unrelated_generation = library
        .publish(&prepared_unrelated, |path| read_source(&sources, path))
        .unwrap();
    let original_left = library.current(left).unwrap().clone();
    let original_right = library.current(right).unwrap().clone();
    let mut affected = library.affected_by(&helper_path);
    affected.sort_unstable();
    assert_eq!(affected, [left, right]);
    assert_eq!(library.affected_by(&unrelated_path), [unrelated]);

    let repaired_manifest = [
        constants_path.clone(),
        replacement_helper.clone(),
        left_path.clone(),
    ];
    let mut failed_reads = Vec::new();
    let failure = library
        .prepare_update(left, &repaired_manifest, |path| {
            failed_reads.push(path.clone());
            read_source(&sources, path)
        })
        .unwrap_err();
    assert_eq!(
        failed_reads,
        [constants_path.clone(), replacement_helper.clone()]
    );
    assert_eq!(
        failure,
        MaterialUpdateError::SourceUnavailable {
            path: replacement_helper.clone(),
            message: format!("missing in-memory source: {replacement_helper}"),
        }
    );
    assert_eq!(library.last_failure(left), Some(&failure));
    assert_eq!(library.current(left), Some(&original_left));
    assert_eq!(library.current(right), Some(&original_right));
    assert_eq!(library.current(unrelated), Some(&unrelated_generation));
    let failed_observations = library.observations(left).unwrap().to_vec();
    assert_eq!(failed_observations.len(), 2);
    assert_eq!(failed_observations[0].path(), &constants_path);
    assert_eq!(
        failed_observations[0].digest(),
        Some(*blake3::hash(sources[&constants_path].as_bytes()).as_bytes())
    );
    assert_eq!(failed_observations[1].path(), &replacement_helper);
    assert_eq!(failed_observations[1].digest(), None);
    assert_eq!(library.affected_by(&replacement_helper), [left]);
    assert_eq!(library.affected_by(&constants_path), [left]);
    let mut affected = library.affected_by(&helper_path);
    affected.sort_unstable();
    assert_eq!(affected, [left, right]);
    assert_eq!(
        library.observations(left).unwrap(),
        failed_observations.as_slice()
    );

    sources.insert(
        replacement_helper.clone(),
        Arc::<str>::from(
            "fn shared_tone(color: vec4<f32>) -> vec4<f32> { return color * palette_exposure; }",
        ),
    );
    let repaired = library
        .prepare_update(left, &repaired_manifest, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(library.current(left), Some(&original_left));
    let published_left = library
        .publish(&repaired, |path| read_source(&sources, path))
        .unwrap();
    assert_ne!(published_left, original_left);
    assert_eq!(library.current(left), Some(&published_left));
    assert_eq!(library.last_failure(left), None);
    assert_eq!(library.affected_by(&helper_path), [right]);
    assert_eq!(library.affected_by(&replacement_helper), [left]);
    assert_eq!(library.affected_by(&constants_path), [left]);
    assert_eq!(library.affected_by(&left_path), [left]);
    assert_eq!(library.current(right), Some(&original_right));
    assert_eq!(library.current(unrelated), Some(&unrelated_generation));
    assert_eq!(original_left.inputs()[0].source(), SHARED_HELPER);
    assert_eq!(
        published_left
            .inputs()
            .iter()
            .map(|input| input.path())
            .collect::<Vec<_>>(),
        repaired_manifest.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        library
            .observations(left)
            .unwrap()
            .iter()
            .map(|observation| (observation.path(), observation.digest()))
            .collect::<Vec<_>>(),
        published_left
            .inputs()
            .iter()
            .map(|input| (input.path(), Some(input.digest())))
            .collect::<Vec<_>>()
    );
}

#[test]
fn publication_rejects_foreign_owner_and_source_drift_then_retries_exact_candidate() {
    let id = "workflow.palette";
    let path = asset_path("materials/palette.wgsl");
    let manifest = [path.clone()];
    let declaration = palette_declaration(id);
    let mut sources = BTreeMap::from([(path.clone(), Arc::<str>::from(PALETTE_FRAGMENT))]);
    let mut library = Material2dLibrary::default();
    let mut foreign = Material2dLibrary::default();
    library.declare(declaration.clone()).unwrap();
    foreign.declare(declaration).unwrap();
    let prepared_a = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    let original = library
        .publish(&prepared_a, |path| read_source(&sources, path))
        .unwrap();
    let foreign_prepared = foreign
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    let foreign_original = foreign
        .publish(&foreign_prepared, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(original.declaration(), foreign_original.declaration());
    assert_eq!(original.sequence(), foreign_original.sequence());

    let exact_candidate_source = Arc::<str>::from(PALETTE_FRAGMENT.replace("0.25", "0.75"));
    sources.insert(path.clone(), exact_candidate_source.clone());
    let prepared_b = library
        .prepare_update(id, &manifest, |path| read_source(&sources, path))
        .unwrap();
    let mut foreign_reads = 0;
    assert_eq!(
        foreign.publish(&prepared_b, |path| {
            foreign_reads += 1;
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::ForeignUpdate)
    );
    assert_eq!(foreign_reads, 0);
    assert_eq!(foreign.current(id), Some(&foreign_original));
    assert_eq!(library.current(id), Some(&original));

    sources.insert(
        path.clone(),
        Arc::<str>::from(format!("{exact_candidate_source}\n")),
    );
    let mut drift_reads = Vec::new();
    assert_eq!(
        library.publish(&prepared_b, |path| {
            drift_reads.push(path.clone());
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::SourceChanged(path.clone()))
    );
    assert_eq!(drift_reads, manifest);
    assert_eq!(library.current(id), Some(&original));
    assert_eq!(
        prepared_b.candidate().inputs()[0].source(),
        exact_candidate_source.as_ref()
    );
    assert_eq!(original.inputs()[0].source(), PALETTE_FRAGMENT);

    sources.insert(path, exact_candidate_source.clone());
    let published = library
        .publish(&prepared_b, |path| read_source(&sources, path))
        .unwrap();
    assert_eq!(&published, prepared_b.candidate());
    assert_eq!(library.current(id), Some(&published));
    assert_ne!(published, original);
    assert_eq!(
        published.inputs()[0].source(),
        exact_candidate_source.as_ref()
    );
    assert_eq!(original.inputs()[0].source(), PALETTE_FRAGMENT);
    assert_eq!(foreign.current(id), Some(&foreign_original));
}

#[test]
fn refused_source_manifests_and_wrong_binding_fields_preserve_current_material() {
    let id = "workflow.surface";
    let path = asset_path("materials/surface.wgsl");
    let mut sources = BTreeMap::from([(path.clone(), Arc::<str>::from(SURFACE_FRAGMENT))]);
    let declaration = surface_declaration(id);
    let mut library = Material2dLibrary::default();
    library.declare(declaration.clone()).unwrap();
    let prepared = library
        .prepare_update(id, std::slice::from_ref(&path), |path| {
            read_source(&sources, path)
        })
        .unwrap();
    let current = library
        .publish(&prepared, |path| read_source(&sources, path))
        .unwrap();
    let parameters = SurfaceParameters {
        strength: 1.0,
        uv_scale: Vec2::new(2.0, 3.0),
        bias: Vec3::new(0.25, 0.5, 0.75),
        opacity: 0.5,
    };
    let noise: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let mask: Handle<ImageAsset> = allocate_anonymous_handle().unwrap();
    let textures = BTreeMap::from([
        ("image.noise".to_owned(), noise),
        ("image.mask".to_owned(), mask),
    ]);
    let original_binding = current.bind(&parameters, textures.clone()).unwrap();
    assert_eq!(
        library.declare(declaration),
        Err(MaterialUpdateError::DuplicateDeclaration(id.to_owned()))
    );
    assert_eq!(library.current(id), Some(&current));

    let mut rejected_manifest_reads = 0;
    assert!(matches!(
        library.prepare_update(id, &[], |path| {
            rejected_manifest_reads += 1;
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::SourceLimit)
    ));
    assert!(matches!(
        library.prepare_update(id, &[path.clone(), path.clone()], |path| {
            rejected_manifest_reads += 1;
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::DuplicateSource(duplicate)) if duplicate == path
    ));
    let too_many_inputs: Vec<_> = (0..33)
        .map(|index| asset_path(&format!("materials/part_{index}.wgsl")))
        .collect();
    assert!(matches!(
        library.prepare_update(id, &too_many_inputs, |path| {
            rejected_manifest_reads += 1;
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::SourceLimit)
    ));
    assert_eq!(rejected_manifest_reads, 0);
    assert_eq!(library.current(id), Some(&current));

    let empty_path = asset_path("materials/empty.wgsl");
    sources.insert(empty_path.clone(), Arc::<str>::from(""));
    assert!(matches!(
        library.prepare_update(id, &[empty_path], |path| read_source(&sources, path)),
        Err(MaterialUpdateError::Shader(
            MaterialShaderError::EmptySource
        ))
    ));
    assert_eq!(library.current(id), Some(&current));

    let large_prefix = asset_path("materials/large_prefix.wgsl");
    let large_suffix = asset_path("materials/large_suffix.wgsl");
    sources.insert(
        large_prefix.clone(),
        Arc::<str>::from(" ".repeat(512 * 1024)),
    );
    sources.insert(
        large_suffix.clone(),
        Arc::<str>::from(" ".repeat(512 * 1024 + 1)),
    );
    let oversized_manifest = [large_prefix, large_suffix];
    let mut oversized_reads = Vec::new();
    assert!(matches!(
        library.prepare_update(id, &oversized_manifest, |path| {
            oversized_reads.push(path.clone());
            read_source(&sources, path)
        }),
        Err(MaterialUpdateError::SourceLimit)
    ));
    assert_eq!(oversized_reads, oversized_manifest);
    assert_eq!(
        library.last_failure(id),
        Some(&MaterialUpdateError::SourceLimit)
    );
    assert_eq!(library.current(id), Some(&current));

    let values = parameters.parameter_values().unwrap();
    let mut missing = values.clone();
    missing.remove("surface.strength");
    assert_eq!(
        current.bind_values(missing, textures.clone()),
        Err(MaterialUpdateError::Declaration(
            MaterialDeclarationError::MissingParameter("surface.strength".to_owned())
        ))
    );
    let mut unknown = values.clone();
    unknown.insert("strength".to_owned(), MaterialParameterValue::F32(1.0));
    assert_eq!(
        current.bind_values(unknown, textures.clone()),
        Err(MaterialUpdateError::Declaration(
            MaterialDeclarationError::UnknownParameter("strength".to_owned())
        ))
    );
    let mut wrong_kind = values.clone();
    wrong_kind.insert(
        "surface.bias".to_owned(),
        MaterialParameterValue::Vec2([0.25, 0.5]),
    );
    assert_eq!(
        current.bind_values(wrong_kind, textures.clone()),
        Err(MaterialUpdateError::Declaration(
            MaterialDeclarationError::ParameterKindMismatch {
                id: "surface.bias".to_owned(),
                expected: MaterialParameterKind::Vec3,
                actual: MaterialParameterKind::Vec2,
            }
        ))
    );
    let shader_names_instead_of_ids = BTreeMap::from([
        ("noise_texture".to_owned(), noise),
        ("image.mask".to_owned(), mask),
    ]);
    assert_eq!(
        current.bind_values(values, shader_names_instead_of_ids),
        Err(MaterialUpdateError::TextureFields)
    );
    assert_eq!(library.current(id), Some(&current));
    assert_eq!(
        library
            .current(id)
            .unwrap()
            .bind(&parameters, textures)
            .unwrap(),
        original_binding
    );
}
