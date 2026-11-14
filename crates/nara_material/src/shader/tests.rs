use encase::ShaderType;
use glam::{Vec2, Vec3, Vec4};

use super::*;
use crate::AlphaMode2d;
use crate::declaration::MaterialTextureDeclaration;
use crate::parameters::MaterialUniformField;

const PASS_THROUGH: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint;
}
"#;

#[derive(encase::ShaderType)]
struct TintParameters {
    tint: Vec4,
}

#[derive(encase::ShaderType)]
struct MixedParameters {
    exposure: f32,
    direction: Vec3,
    threshold: f32,
    uv_scale: Vec2,
    palette_index: i32,
    flags: u32,
}

fn field(id: &str, name: &str, kind: MaterialParameterKind, offset: u64) -> MaterialUniformField {
    MaterialUniformField::new(id, name, kind, offset.try_into().unwrap()).unwrap()
}

fn declaration(
    layout: MaterialParameterLayout,
    textures: Vec<MaterialTextureDeclaration>,
) -> Material2dDeclaration {
    Material2dDeclaration::new("shader.tests", 1, AlphaMode2d::Blend, layout, textures).unwrap()
}

fn tint_layout() -> MaterialParameterLayout {
    MaterialParameterLayout::new(
        vec![field(
            "color.tint",
            "tint",
            MaterialParameterKind::Vec4,
            TintParameters::METADATA.offset(0),
        )],
        TintParameters::min_size().get().try_into().unwrap(),
    )
    .unwrap()
}

fn tint_declaration() -> Material2dDeclaration {
    declaration(
        tint_layout(),
        vec![MaterialTextureDeclaration::new("image.palette", "palette", 1).unwrap()],
    )
}

fn mixed_declaration() -> Material2dDeclaration {
    let layout = MaterialParameterLayout::new(
        vec![
            field(
                "light.exposure",
                "exposure",
                MaterialParameterKind::F32,
                MixedParameters::METADATA.offset(0),
            ),
            field(
                "light.direction",
                "direction",
                MaterialParameterKind::Vec3,
                MixedParameters::METADATA.offset(1),
            ),
            field(
                "mask.threshold",
                "threshold",
                MaterialParameterKind::F32,
                MixedParameters::METADATA.offset(2),
            ),
            field(
                "image.scale",
                "uv_scale",
                MaterialParameterKind::Vec2,
                MixedParameters::METADATA.offset(3),
            ),
            field(
                "palette.index",
                "palette_index",
                MaterialParameterKind::I32,
                MixedParameters::METADATA.offset(4),
            ),
            field(
                "material.flags",
                "flags",
                MaterialParameterKind::U32,
                MixedParameters::METADATA.offset(5),
            ),
        ],
        MixedParameters::min_size().get().try_into().unwrap(),
    )
    .unwrap();
    declaration(
        layout,
        vec![
            MaterialTextureDeclaration::new("image.detail", "detail", 8).unwrap(),
            MaterialTextureDeclaration::new("image.mask", "mask", 2).unwrap(),
        ],
    )
}

fn validated_module(source: &str) -> naga::Module {
    let module = naga::front::wgsl::parse_str(source).unwrap();
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(&module)
        .unwrap();
    module
}

#[test]
fn compiles_two_encase_layouts_and_preserves_the_complete_stock_shader() {
    let tint = tint_declaration();
    let tint_shader = MaterialShaderArtifact::compile(
        &tint,
        r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette_color = textureSample(palette, sprite_sampler, uv);
    return sampled * tint * material_parameters.tint * palette_color;
}
"#,
    )
    .unwrap();
    let mixed = mixed_declaration();
    let mixed_shader = MaterialShaderArtifact::compile(
        &mixed,
        r#"
fn material_light() -> vec4<f32> {
    return vec4<f32>(
        material_parameters.direction * material_parameters.exposure,
        material_parameters.threshold,
    );
}

fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let detail_color = textureSample(detail, sprite_sampler, uv * material_parameters.uv_scale);
    let mask_color = textureSample(mask, sprite_sampler, uv);
    let scale = f32(material_parameters.palette_index) + f32(material_parameters.flags);
    return sampled * tint * detail_color + material_light() * mask_color * scale;
}
"#,
    )
    .unwrap();
    assert_eq!(tint.parameters().byte_size(), 16);
    assert_eq!(mixed.parameters().byte_size(), 48);
    assert_eq!(mixed.parameters().fields()[2].offset(), 28);
    for (declaration, shader) in [(&tint, &tint_shader), (&mixed, &mixed_shader)] {
        assert!(shader.source().starts_with(STOCK_FRAGMENT));
        let module = validated_module(shader.source());
        assert_eq!(module.entry_points.len(), STOCK_ENTRY_POINTS.len());
        assert_eq!(
            module.global_variables.len(),
            declaration.textures().len() + 3
        );
        assert_eq!(
            shader.digest(),
            *blake3::hash(shader.source().as_bytes()).as_bytes()
        );
    }
    assert_ne!(
        tint_shader.interface_digest(),
        mixed_shader.interface_digest()
    );
}

#[test]
fn rejects_missing_or_inexact_material_fragment_signatures() {
    let declaration = tint_declaration();
    for source in [
        "fn unrelated() {}",
        "fn material_fragment() -> vec4<f32> { return vec4<f32>(1.0); }",
        "fn material_fragment(sampled: vec3<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return tint; }",
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec3<f32>) -> vec4<f32> { return tint; }",
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec3<f32> { return tint.rgb; }",
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) {}",
    ] {
        assert!(
            MaterialShaderArtifact::compile(&declaration, source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn reports_real_parse_and_validation_failures() {
    let declaration = tint_declaration();
    assert!(matches!(
        MaterialShaderArtifact::compile(&declaration, "fn material_fragment("),
        Err(MaterialShaderError::Parse(_))
    ));
    let source = format!("{PASS_THROUGH}\nfn invalid_return() -> f32 {{}}\n");
    assert!(matches!(
        MaterialShaderArtifact::compile(&declaration, &source),
        Err(MaterialShaderError::Validation(_))
    ));
}

#[test]
fn rejects_undeclared_resources_even_when_unused() {
    let declaration = tint_declaration();
    for resource in [
        "@group(2) @binding(0) var hidden: texture_2d<f32>;",
        "@group(0) @binding(2) var hidden: sampler;",
        "@group(1) @binding(7) var hidden: texture_2d<f32>;",
        "@group(1) @binding(7) var<storage, read_write> hidden: array<u32>;",
        "@group(1) @binding(1) var duplicate: texture_2d<f32>;",
    ] {
        let source = format!("{PASS_THROUGH}\n{resource}\n");
        assert!(
            matches!(
                MaterialShaderArtifact::compile(&declaration, &source),
                Err(MaterialShaderError::Interface(_))
            ),
            "{resource}"
        );
    }
}

#[test]
fn rejects_extra_entry_points_and_stock_replacement() {
    let declaration = tint_declaration();
    let extra = format!(
        "{PASS_THROUGH}\n@fragment fn extra() -> @location(0) vec4<f32> {{ return vec4<f32>(1.0); }}\n"
    );
    assert!(matches!(
        MaterialShaderArtifact::compile(&declaration, &extra),
        Err(MaterialShaderError::Interface(_))
    ));
    let replacement = format!("{PASS_THROUGH}\n@vertex fn vs_main() {{}}\n");
    assert!(MaterialShaderArtifact::compile(&declaration, &replacement).is_err());
}

#[test]
fn permits_private_state_but_rejects_workgroup_and_undeclared_pipeline_inputs() {
    let declaration = tint_declaration();
    let private = r#"
var<private> local_tint: vec4<f32>;

fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    local_tint = tint;
    return sampled * local_tint;
}
"#;
    MaterialShaderArtifact::compile(&declaration, private).unwrap();
    for extra in [
        "var<workgroup> shared_data: u32;",
        "override external_scale: f32 = 1.0;",
    ] {
        let source = format!("{PASS_THROUGH}\n{extra}\n");
        assert!(matches!(
            MaterialShaderArtifact::compile(&declaration, &source),
            Err(MaterialShaderError::Interface(_))
        ));
    }
}

#[test]
fn checks_the_complete_compiler_observed_texture_type() {
    let declaration = tint_declaration();
    let artifact = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    for wrong_type in [
        "texture_2d<u32>",
        "texture_2d<i32>",
        "texture_2d_array<f32>",
        "texture_multisampled_2d<f32>",
        "texture_depth_2d",
        "texture_cube<f32>",
        "texture_storage_2d<rgba8unorm, write>",
        "sampler_comparison",
    ] {
        let altered = artifact.source().replace(
            "var palette: texture_2d<f32>;",
            &format!("var palette: {wrong_type};"),
        );
        let module = validated_module(&altered);
        assert!(
            matches!(
                validate_interface(&declaration, &module),
                Err(MaterialShaderError::Interface(_))
            ),
            "{wrong_type}"
        );
    }
}

#[test]
fn rejects_a_declared_offset_that_differs_from_the_actual_uniform() {
    let layout = MaterialParameterLayout::new(
        vec![
            field("first.id", "first", MaterialParameterKind::F32, 4),
            field("second.id", "second", MaterialParameterKind::Vec4, 16),
        ],
        32,
    )
    .unwrap();
    let error =
        MaterialShaderArtifact::compile(&declaration(layout, vec![]), PASS_THROUGH).unwrap_err();
    assert!(matches!(
        error,
        MaterialShaderError::Interface(message) if message.contains("offset")
    ));
}

#[test]
fn rejects_a_declared_span_that_differs_from_the_actual_uniform() {
    let layout = MaterialParameterLayout::new(tint_layout().fields().to_vec(), 32).unwrap();
    let error =
        MaterialShaderArtifact::compile(&declaration(layout, vec![]), PASS_THROUGH).unwrap_err();
    assert!(matches!(
        error,
        MaterialShaderError::Interface(message) if message.contains("span")
    ));
}

#[test]
fn matches_same_type_members_by_their_explicit_names_and_offsets() {
    let layout = MaterialParameterLayout::new(
        vec![
            field("first.id", "first", MaterialParameterKind::F32, 0),
            field("second.id", "second", MaterialParameterKind::F32, 4),
        ],
        8,
    )
    .unwrap();
    let declaration = declaration(layout, vec![]);
    let artifact = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    for replacement in [
        "    second: f32,\n    first: f32,",
        "    first: i32,\n    second: f32,",
    ] {
        let altered = artifact
            .source()
            .replace("    first: f32,\n    second: f32,", replacement);
        assert!(matches!(
            validate_interface(&declaration, &validated_module(&altered)),
            Err(MaterialShaderError::Interface(_))
        ));
    }
}

#[test]
fn fingerprints_the_explicit_semantic_mapping_between_same_type_members() {
    let first = declaration(
        MaterialParameterLayout::new(
            vec![
                field("red", "first", MaterialParameterKind::F32, 0),
                field("blue", "second", MaterialParameterKind::F32, 4),
            ],
            8,
        )
        .unwrap(),
        vec![],
    );
    let second = declaration(
        MaterialParameterLayout::new(
            vec![
                field("blue", "first", MaterialParameterKind::F32, 0),
                field("red", "second", MaterialParameterKind::F32, 4),
            ],
            8,
        )
        .unwrap(),
        vec![],
    );
    assert_eq!(first.semantic_digest(), second.semantic_digest());
    assert_eq!(first.binding_digest(), second.binding_digest());
    let first = MaterialShaderArtifact::compile(&first, PASS_THROUGH).unwrap();
    let second = MaterialShaderArtifact::compile(&second, PASS_THROUGH).unwrap();
    assert_eq!(first.digest(), second.digest());
    assert_ne!(first.interface_digest(), second.interface_digest());
}

#[test]
fn fingerprints_the_explicit_semantic_mapping_between_textures() {
    let first = declaration(
        tint_layout(),
        vec![
            MaterialTextureDeclaration::new("image.red", "first_image", 1).unwrap(),
            MaterialTextureDeclaration::new("image.blue", "second_image", 2).unwrap(),
        ],
    );
    let second = declaration(
        tint_layout(),
        vec![
            MaterialTextureDeclaration::new("image.blue", "first_image", 1).unwrap(),
            MaterialTextureDeclaration::new("image.red", "second_image", 2).unwrap(),
        ],
    );
    assert_eq!(first.semantic_digest(), second.semantic_digest());
    assert_eq!(first.binding_digest(), second.binding_digest());
    let first = MaterialShaderArtifact::compile(&first, PASS_THROUGH).unwrap();
    let second = MaterialShaderArtifact::compile(&second, PASS_THROUGH).unwrap();
    assert_eq!(first.digest(), second.digest());
    assert_ne!(first.interface_digest(), second.interface_digest());
}

#[test]
fn body_edits_and_helper_types_preserve_the_interface_digest() {
    let declaration = tint_declaration();
    let first = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    let edited = PASS_THROUGH.replace("sampled * tint", "sampled * tint * 0.5");
    let second = MaterialShaderArtifact::compile(
        &declaration,
        &format!("struct HelperData {{ unused: vec3<f32>, }};\n{edited}"),
    )
    .unwrap();
    assert_ne!(first.digest(), second.digest());
    assert_eq!(first.interface_digest(), second.interface_digest());
    assert_eq!(first, first.clone());
}

#[test]
fn canonical_interface_order_is_independent_of_texture_declaration_order() {
    let first = mixed_declaration();
    let mut reversed_textures = first.textures().to_vec();
    reversed_textures.reverse();
    let second = declaration(first.parameters().clone(), reversed_textures);
    let first = MaterialShaderArtifact::compile(&first, PASS_THROUGH).unwrap();
    let second = MaterialShaderArtifact::compile(&second, PASS_THROUGH).unwrap();
    assert_ne!(first.digest(), second.digest());
    assert_eq!(first.interface_digest(), second.interface_digest());
}

#[test]
fn bounds_supplied_and_assembled_source_before_assembly_allocation() {
    let declaration = tint_declaration();
    assert_eq!(
        MaterialShaderArtifact::compile(&declaration, " \r\n\t"),
        Err(MaterialShaderError::EmptySource)
    );
    let oversized = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert!(matches!(
        MaterialShaderArtifact::compile(&declaration, &oversized),
        Err(MaterialShaderError::SourceLimit(_))
    ));
    let mut counter = SourceByteCount(0);
    write_shader(&mut counter, &declaration, PASS_THROUGH).unwrap();
    let padding = MAX_SOURCE_BYTES - counter.0;
    let mut source = format!("{PASS_THROUGH}{}", " ".repeat(padding));
    let artifact = MaterialShaderArtifact::compile(&declaration, &source).unwrap();
    assert_eq!(artifact.source().len(), MAX_SOURCE_BYTES);
    source.push(' ');
    assert!(source.len() < MAX_SOURCE_BYTES);
    assert!(matches!(
        MaterialShaderArtifact::compile(&declaration, &source),
        Err(MaterialShaderError::SourceLimit(_))
    ));
}

#[test]
fn compiler_diagnostics_are_finite_and_preserve_utf8_boundaries() {
    let truncated = bounded_diagnostic("界".repeat(MAX_DIAGNOSTIC_BYTES));
    assert!(truncated.len() <= MAX_DIAGNOSTIC_BYTES);
    assert!(truncated.ends_with("[diagnostic truncated]"));
    let source = format!("fn material_fragment {} {{}}", "a".repeat(32_768));
    let error = MaterialShaderArtifact::compile(&tint_declaration(), &source).unwrap_err();
    let MaterialShaderError::Parse(diagnostic) = error else {
        panic!("expected a real Naga parse error");
    };
    assert!(!diagnostic.is_empty());
    assert!(diagnostic.len() <= MAX_DIAGNOSTIC_BYTES);
}
