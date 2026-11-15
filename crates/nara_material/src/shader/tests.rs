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
            declaration.textures().len() + 4
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
fn stock_view_has_the_exact_naga_uniform_layout_and_period_constant() {
    use naga::{Scalar, VectorSize};

    let declaration = tint_declaration();
    let artifact = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    let module = validated_module(artifact.source());
    let view = module
        .global_variables
        .iter()
        .find_map(|(_, variable)| {
            (variable.name.as_deref() == Some("nara_view")).then_some(variable)
        })
        .unwrap();
    assert_eq!(view.space, AddressSpace::Uniform);
    assert_eq!(
        view.binding,
        Some(naga::ResourceBinding {
            group: 2,
            binding: 0,
        })
    );
    assert_eq!(module.types[view.ty].name.as_deref(), Some("NaraView"));
    let TypeInner::Struct { members, span } = &module.types[view.ty].inner else {
        panic!("NaraView must be a struct");
    };
    assert_eq!(*span, 224);
    assert_eq!(*span, __STOCK_VIEW_UNIFORM_BYTES);
    let observed: Vec<_> = members
        .iter()
        .map(|member| {
            assert!(member.binding.is_none());
            (member.name.as_deref().unwrap(), member.offset)
        })
        .collect();
    assert_eq!(
        observed,
        [
            ("world_to_clip", 0),
            ("clip_to_world", 48),
            ("viewport", 96),
            ("real_elapsed", 112),
            ("real_delta", 116),
            ("virtual_elapsed", 120),
            ("virtual_delta", 124),
            ("fixed_elapsed", 128),
            ("fixed_delta", 132),
            ("fixed_timestep", 136),
            ("interpolation_alpha", 140),
            ("time_scale", 144),
            ("clock_available", 148),
            ("paused", 152),
            ("spatial_interpolated", 156),
            ("frame", 160),
            ("fixed_tick", 168),
            ("real_period", 176),
            ("virtual_period", 184),
            ("fixed_period", 192),
            ("_reserved0", 200),
            ("virtual_frame", 208),
            ("fixed_steps", 216),
            ("_reserved1", 220),
        ]
    );
    let mut layouter = naga::proc::Layouter::default();
    layouter.update(module.to_ctx()).unwrap();
    assert_eq!(layouter[view.ty].size, 224);
    assert_eq!(layouter[view.ty].alignment, naga::proc::Alignment::SIXTEEN);
    for member in &members[..2] {
        assert_eq!(
            module.types[member.ty].inner,
            TypeInner::Matrix {
                columns: VectorSize::Tri,
                rows: VectorSize::Tri,
                scalar: Scalar::F32,
            }
        );
        assert_eq!(layouter[member.ty].size, 48);
        assert_eq!(
            layouter[member.ty].alignment,
            naga::proc::Alignment::SIXTEEN
        );
    }
    assert_eq!(
        module.types[members[2].ty].inner,
        TypeInner::Vector {
            size: VectorSize::Quad,
            scalar: Scalar::F32,
        }
    );
    for member in &members[3..12] {
        assert_eq!(
            module.types[member.ty].inner,
            TypeInner::Scalar(Scalar::F32)
        );
    }
    for member in members[12..15].iter().chain(&members[22..]) {
        assert_eq!(
            module.types[member.ty].inner,
            TypeInner::Scalar(Scalar::U32)
        );
    }
    for member in &members[15..22] {
        assert_eq!(
            module.types[member.ty].inner,
            TypeInner::Vector {
                size: VectorSize::Bi,
                scalar: Scalar::U32,
            }
        );
    }
    let period = module
        .constants
        .iter()
        .find_map(|(_, constant)| {
            (constant.name.as_deref() == Some("NARA_TIME_PERIOD_SECONDS")).then_some(constant)
        })
        .unwrap();
    assert_eq!(
        module.types[period.ty].inner,
        TypeInner::Scalar(Scalar::F32)
    );
    assert!(matches!(
        module.global_expressions[period.init],
        naga::Expression::Literal(naga::Literal::F32(value)) if value == 3600.0
    ));
}

#[test]
fn compiles_both_clocks_and_the_original_projection_and_frame_facts() {
    let source = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let world = nara_view.clip_to_world * vec3<f32>(uv * 2.0 - vec2<f32>(1.0), 1.0);
    let clip = nara_view.world_to_clip * world;
    let pixel = nara_view.viewport.xy + uv * nara_view.viewport.zw;
    let available = nara_view.clock_available != 0u;
    let real_time = select(0.0, nara_view.real_elapsed, available);
    let virtual_time = select(0.0, nara_view.virtual_elapsed, available);
    let elapsed = vec4<f32>(real_time, virtual_time, nara_view.fixed_elapsed, clip.x);
    let deltas = vec4<f32>(
        nara_view.real_delta, nara_view.virtual_delta,
        nara_view.fixed_delta, nara_view.fixed_timestep,
    );
    let state = vec4<f32>(
        nara_view.interpolation_alpha, nara_view.time_scale,
        f32(nara_view.paused), f32(nara_view.spatial_interpolated),
    );
    let words = nara_view.frame + nara_view.virtual_frame + nara_view.fixed_tick
        + nara_view.real_period + nara_view.virtual_period + nara_view.fixed_period;
    let counts = vec4<f32>(vec2<f32>(words), f32(nara_view.fixed_steps), 0.0);
    return sampled * tint * material_parameters.tint
        + elapsed / NARA_TIME_PERIOD_SECONDS + deltas + state + counts
        + vec4<f32>(pixel, world.xy);
}
"#;
    MaterialShaderArtifact::compile(&tint_declaration(), source).unwrap();
}

#[test]
fn real_or_virtual_clock_selection_preserves_the_material_and_interface_abi() {
    let declaration = tint_declaration();
    let source = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let elapsed = select(0.0, nara_view.real_elapsed, nara_view.clock_available != 0u);
    return sampled * tint * material_parameters.tint * (elapsed / NARA_TIME_PERIOD_SECONDS);
}
"#;
    let real = MaterialShaderArtifact::compile(&declaration, source).unwrap();
    let virtual_source = source.replace("nara_view.real_elapsed", "nara_view.virtual_elapsed");
    let virtual_time = MaterialShaderArtifact::compile(&declaration, &virtual_source).unwrap();
    assert_ne!(real.digest(), virtual_time.digest());
    assert_eq!(real.interface_digest(), virtual_time.interface_digest());
    assert_eq!(declaration.parameters().byte_size(), 16);
}

#[test]
fn rejects_compiler_observed_view_name_type_and_layout_mismatches() {
    let declaration = tint_declaration();
    let artifact = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    let source = artifact.source().replace("\r\n", "\n");
    for (original, replacement) in [
        ("NaraView", "ForeignView"),
        ("@group(2) @binding(0)", "@group(2) @binding(2)"),
        (
            "@group(2) @binding(0)\nvar<uniform> nara_view: NaraView;",
            "",
        ),
        ("var<uniform> nara_view:", "var<uniform> foreign_view:"),
        ("var<uniform> nara_view:", "var<storage, read> nara_view:"),
        ("nara_view: NaraView;", "nara_view: vec4<f32>;"),
        ("world_to_clip: mat3x3<f32>", "world_to_clip: mat3x4<f32>"),
        ("clip_to_world: mat3x3<f32>", "clip_to_world: mat4x3<f32>"),
        ("viewport: vec4<f32>", "viewport: vec4<u32>"),
        ("real_elapsed: f32", "real_elapsed: i32"),
        ("fixed_delta: f32", "fixed_delta: u32"),
        ("clock_available: u32", "clock_available: f32"),
        ("    frame: vec2<u32>", "    frame: vec2<f32>"),
        ("_reserved0: vec2<u32>", "_reserved0: vec2<f32>"),
        ("fixed_steps: u32", "fixed_steps: f32"),
        ("_reserved1: u32", "@size(32) _reserved1: u32"),
        ("    _reserved0: vec2<u32>,\n", ""),
        (
            "    real_elapsed: f32,\n    real_delta: f32,",
            "    real_delta: f32,\n    real_elapsed: f32,",
        ),
    ] {
        let altered = source.replace(original, replacement);
        assert_ne!(altered, source, "{original}");
        let module = validated_module(&altered);
        assert!(
            matches!(
                validate_interface(&declaration, &module),
                Err(MaterialShaderError::Interface(_))
            ),
            "{replacement}"
        );
    }
}

#[test]
fn view_types_and_layout_contribute_to_the_canonical_interface_digest() {
    let declaration = tint_declaration();
    let artifact = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    for (original, replacement) in [
        ("world_to_clip: mat3x3<f32>", "world_to_clip: mat3x4<f32>"),
        ("clip_to_world: mat3x3<f32>", "clip_to_world: mat4x3<f32>"),
        ("    frame: vec2<u32>", "    frame: vec2<f32>"),
        ("viewport: vec4<f32>", "viewport: vec4<u32>"),
        ("_reserved1: u32", "@size(32) _reserved1: u32"),
        ("real_elapsed: f32", "renamed_elapsed: f32"),
    ] {
        let altered = artifact.source().replace(original, replacement);
        let module = validated_module(&altered);
        assert_ne!(
            artifact.interface_digest(),
            interface_digest(&declaration, &module).unwrap(),
            "{replacement}"
        );
    }
}

#[test]
fn rejects_redeclarations_of_engine_view_symbols() {
    let material = tint_declaration();
    for extra in [
        "var<private> nara_view: vec4<f32>;",
        "@group(2) @binding(0) var<uniform> nara_view: vec4<f32>;",
        "struct NaraView { impostor: vec4<f32>, };",
        "alias NaraView = vec4<f32>;",
        "const NARA_TIME_PERIOD_SECONDS: f32 = 1.0;",
    ] {
        let source = format!("{PASS_THROUGH}\n{extra}\n");
        assert!(
            MaterialShaderArtifact::compile(&material, &source).is_err(),
            "{extra}"
        );
    }
    let texture_shadow = declaration(
        tint_layout(),
        vec![MaterialTextureDeclaration::new("image.shadow", "nara_view", 1).unwrap()],
    );
    assert!(MaterialShaderArtifact::compile(&texture_shadow, PASS_THROUGH).is_err());
}

#[test]
fn local_view_names_follow_wgsl_scope_and_keep_the_engine_interface() {
    let declaration = tint_declaration();
    let original = MaterialShaderArtifact::compile(&declaration, PASS_THROUGH).unwrap();
    for function in [
        "fn shadow(nara_view: f32) -> f32 { return nara_view; }",
        "fn shadow() -> f32 { var nara_view: f32 = 1.0; return nara_view; }",
        "fn shadow() -> f32 { let nara_view = 1.0; return nara_view; }",
        "fn shadow(NaraView: f32) -> f32 { return NaraView; }",
        "fn shadow() -> f32 { let NARA_TIME_PERIOD_SECONDS = 1.0; return NARA_TIME_PERIOD_SECONDS; }",
        "fn shadow() -> f32 { const NARA_TIME_PERIOD_SECONDS: f32 = 1.0; return NARA_TIME_PERIOD_SECONDS; }",
        "fn shadow() -> f32 { let NARA_TIME_PERIOD_SECONDS = 1.0; let copy = NARA_TIME_PERIOD_SECONDS; return copy; }",
    ] {
        let source = format!("{PASS_THROUGH}\n{function}\n");
        let local = MaterialShaderArtifact::compile(&declaration, &source).unwrap();
        assert_eq!(
            original.interface_digest(),
            local.interface_digest(),
            "local scope cannot alter the engine resource ABI: {function}"
        );
    }
}

#[test]
fn engine_view_symbols_do_not_reserve_authored_material_member_names() {
    let layout = MaterialParameterLayout::new(
        vec![field(
            "color.tint",
            "nara_view",
            MaterialParameterKind::Vec4,
            0,
        )],
        16,
    )
    .unwrap();
    let declaration = declaration(layout, vec![]);
    MaterialShaderArtifact::compile(
        &declaration,
        r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.nara_view * nara_view.real_delta;
}
"#,
    )
    .unwrap();
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
        "@group(2) @binding(0) var<uniform> hidden: NaraView;",
        "@group(2) @binding(1) var hidden: texture_2d<f32>;",
        "@group(2) @binding(1) var hidden: sampler;",
        "@group(2) @binding(1) var<uniform> hidden: NaraView;",
        "@group(2) @binding(1) var<storage, read> hidden: array<u32>;",
        "@group(3) @binding(0) var hidden: texture_2d<f32>;",
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
