use super::*;
use crate::{
    quad::WgpuQuadBatch,
    readback_tests::{
        MIXED_MATERIAL_FRAGMENT, MixedMaterialParameters, TEXTURE_MATERIAL_FRAGMENT,
        image_versions_with_handle, material_batch, material_binding, material_library,
        mixed_material_binding, mixed_material_library, publish_material,
    },
    sprite::{SpriteCapture, SpriteFeature},
};
use nara_material::AlphaMode2d;

const SWAPPED_FRAGMENT: &str = "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { let color = textureLoad(material_texture, vec2<i32>(0, 0), 0).grba; return color * material_parameters.value * tint; }";

fn captured_program(
    instance: &crate::WgpuFeatureInstance<SpriteFeature<Color>>,
    source: RenderFrameSourceGeneration,
    frame: u64,
    batches: Vec<WgpuQuadBatch>,
) -> (WgpuRenderProgram, WgpuProgramTexture<Color>) {
    let mut builder = WgpuRenderProgramBuilder::new(source, frame, GraphLimits::default()).unwrap();
    let target = builder.texture::<Color>(descriptor()).unwrap();
    let viewport =
        nara_render::ViewportRect::from_extent(nara_render::Extent2d::new(4, 4).unwrap()).unwrap();
    builder
        .operation(
            1,
            instance,
            Arc::new(SpriteCapture::from_lowered(
                target.clone(),
                viewport,
                batches.into(),
            )),
            &[],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    (builder.finish(&[target.output()]).unwrap(), target)
}

fn paired_batches(mut left: WgpuQuadBatch, mut right: WgpuQuadBatch) -> Vec<WgpuQuadBatch> {
    left.instances[0].center = [-0.5, 0.0];
    left.instances[0].x_axis = [0.5, 0.0];
    right.instances[0].center = [0.5, 0.0];
    right.instances[0].x_axis = [0.5, 0.0];
    vec![left, right]
}

fn assert_halves(
    host: &mut WgpuRenderHost,
    output: &WgpuProgramTextureOutput<Color>,
    left: [u8; 4],
) {
    let actual = pixels(host, output);
    for row in actual.chunks_exact(4) {
        assert_eq!(row, [left, left, [255, 0, 0, 255], [255, 0, 0, 255]]);
    }
}

fn assert_mixed_halves(
    host: &mut WgpuRenderHost,
    output: &WgpuProgramTextureOutput<Color>,
    right: [u8; 4],
) {
    for (index, actual) in pixels(host, output).iter().enumerate() {
        let expected = if index % 4 < 2 {
            [0, 255, 0, 255]
        } else {
            right
        };
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| { actual.abs_diff(expected) <= 1 }),
            "pixel {index}: expected {expected:?}, got {actual:?}"
        );
    }
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_shader_cache_bounds_edit_history_without_capping_one_working_set_or_retained_outputs() {
    let mut host = WgpuRenderHost::offscreen();
    let current = host.create_producer().unwrap();
    let delayed = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(SpriteFeature::<Color>::new());
    let current_feature = host.create_feature_instance(&current, &definition).unwrap();
    let delayed_feature = host.create_feature_instance(&delayed, &definition).unwrap();
    let frame_source = source();
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    let mut library = material_library("test.cache-edits", AlphaMode2d::Opaque);
    let original = publish_material(&mut library, "test.cache-edits", TEXTURE_MATERIAL_FRAGMENT);
    let original_batch = material_batch(
        material_binding(&original, [1.0; 4], texture),
        vec![(1, red.clone())],
    );
    let (original_program, original_target) = captured_program(
        &current_feature,
        frame_source,
        1,
        vec![original_batch.clone()],
    );
    let (delayed_program, delayed_target) =
        captured_program(&delayed_feature, frame_source, 1, vec![original_batch]);
    host.begin_frame(1).unwrap();
    let original_outputs = host.submit_program(&current, &original_program).unwrap();
    let original_output = original_outputs.texture(&original_target).unwrap();
    assert!(
        pixels(&mut host, &original_output)
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    drain(original_outputs.submitted_prefix());
    drop(original_program);
    let namespace = (host.backend.instance_id(), host.backend.device_epoch());
    let limit = crate::stock_quad::MAX_CACHED_QUAD_PIPELINES;
    let mut batches = Vec::new();
    for revision in 0..limit + 2 {
        let source =
            format!("{TEXTURE_MATERIAL_FRAGMENT}\nconst shader_revision: u32 = {revision}u;");
        let generation = publish_material(&mut library, "test.cache-edits", &source);
        batches.push(material_batch(
            material_binding(&generation, [1.0; 4], texture),
            vec![(1, red.clone())],
        ));
    }
    let (program, target) = captured_program(&current_feature, frame_source, 2, batches);
    host.begin_frame(2).unwrap();
    let outputs = host.submit_program(&current, &program).unwrap();
    let (attempts, entries, layouts) = host.backend.stock_quad_resources().pipeline_stats();
    assert_eq!(attempts, limit + 3);
    assert_eq!((entries, layouts), (limit, 1));
    assert!(
        pixels(&mut host, &outputs.texture(&target).unwrap())
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    drain(outputs.submitted_prefix());
    drop((outputs, program));
    for revision in limit + 2..limit + 6 {
        let source_text =
            format!("{TEXTURE_MATERIAL_FRAGMENT}\nconst shader_revision: u32 = {revision}u;");
        let generation = publish_material(&mut library, "test.cache-edits", &source_text);
        let batch = material_batch(
            material_binding(&generation, [1.0; 4], texture),
            vec![(1, red.clone())],
        );
        let frame = revision as u64 + 1;
        let (program, target) =
            captured_program(&current_feature, frame_source, frame, vec![batch]);
        host.begin_frame(frame).unwrap();
        let outputs = host.submit_program(&current, &program).unwrap();
        let (_, entries, layouts) = host.backend.stock_quad_resources().pipeline_stats();
        assert_eq!((entries, layouts), (limit, 1));
        assert!(
            pixels(&mut host, &outputs.texture(&target).unwrap())
                .iter()
                .all(|pixel| *pixel == [255, 0, 0, 255])
        );
        drain(outputs.submitted_prefix());
    }
    let current_generation = library.current("test.cache-edits").unwrap().clone();
    let (before, _, _) = host.backend.stock_quad_resources().pipeline_stats();
    let delayed_outputs = host.submit_program(&delayed, &delayed_program).unwrap();
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (before + 1, limit, 1)
    );
    assert_eq!(
        library.current("test.cache-edits"),
        Some(&current_generation)
    );
    assert_ne!(original, current_generation);
    assert_eq!(
        (host.backend.instance_id(), host.backend.device_epoch()),
        namespace
    );
    assert!(
        pixels(
            &mut host,
            &delayed_outputs.texture(&delayed_target).unwrap()
        )
        .iter()
        .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    assert!(
        pixels(&mut host, &original_output)
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    drain(delayed_outputs.submitted_prefix());
    drop((
        delayed_outputs,
        original_output,
        original_outputs,
        delayed_program,
    ));
    drain(&host.retire_producer(&current).unwrap());
    drain(&host.retire_producer(&delayed).unwrap());
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_imported_material_source_storage_survives_until_original_submission_settlement() {
    let mut host = WgpuRenderHost::offscreen();
    let source_producer = host.create_producer().unwrap();
    let frame_source = source();
    let (source_program, source_target) = clear(frame_source, 1, wgpu::Color::GREEN);
    host.begin_frame(1).unwrap();
    let source_outputs = host
        .submit_program(&source_producer, &source_program)
        .unwrap();
    let source_output = source_outputs.texture(&source_target).unwrap();
    let weak_source = Arc::downgrade(&source_output.storage);
    drain(source_outputs.submitted_prefix());
    drain(&host.retire_producer(&source_producer).unwrap());

    let importer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(SpriteFeature::<Color>::new());
    let feature = host
        .create_feature_instance(&importer, &definition)
        .unwrap();
    let mut builder =
        WgpuRenderProgramBuilder::new(frame_source, 2, GraphLimits::default()).unwrap();
    let palette = builder.import_texture(&source_output).unwrap();
    let target = builder.texture::<Color>(descriptor()).unwrap();
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    let mut library = material_library("test.host-material-import-custody", AlphaMode2d::Opaque);
    let generation = publish_material(
        &mut library,
        "test.host-material-import-custody",
        TEXTURE_MATERIAL_FRAGMENT,
    );
    let mut batch = material_batch(
        material_binding(&generation, [1.0; 4], texture),
        vec![(1, red)],
    );
    let material = batch.material_extension.as_mut().unwrap();
    material.textures[0].1 = crate::quad::WgpuMaterialTexture::Program(
        crate::WgpuMaterialTextureBinding::new(&material.binding, "texture", &palette)
            .unwrap()
            .source(),
    );
    let viewport =
        nara_render::ViewportRect::from_extent(nara_render::Extent2d::new(4, 4).unwrap()).unwrap();
    let capture = Arc::new(SpriteCapture::from_lowered(
        target.clone(),
        viewport,
        vec![batch].into(),
    ));
    builder
        .operation(
            1,
            &feature,
            Arc::clone(&capture),
            &[palette.read(AccessRole::Sampled)],
            &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
        )
        .unwrap();
    let program = builder.finish(&[target.output()]).unwrap();
    host.begin_frame(2).unwrap();
    let outputs = host.submit_program(&importer, &program).unwrap();
    let destination = outputs.texture(&target).unwrap();
    let prefix = outputs.submitted_prefix().clone();
    let retirement = host.retire_producer(&importer).unwrap();
    assert!(retirement.submission_index().is_some());
    assert!(prefix.submission_index().is_some());
    drop(retirement);
    let complete_before_drop = prefix.is_complete();
    drop((
        source_outputs,
        source_output,
        source_program,
        source_target,
        source_producer,
        outputs,
        program,
        capture,
        palette,
        target,
        feature,
        definition,
        importer,
        library,
        generation,
    ));
    let source_alive_after_drop = weak_source.upgrade().is_some();
    let pending_after_drop = !prefix.is_complete();
    eprintln!(
        "material import custody: complete_before_drop={complete_before_drop}, \
         pending_after_drop={pending_after_drop}, source_alive_after_drop={source_alive_after_drop}"
    );
    if pending_after_drop {
        assert!(
            source_alive_after_drop,
            "the original pending submission must retain the exact imported source storage"
        );
    }
    drain(&prefix);
    assert!(
        weak_source.upgrade().is_none(),
        "the exact imported source storage must release after its original callback"
    );
    assert_eq!(
        pixels(&mut host, &destination),
        vec![[0, 255, 0, 255]; 4 * 4]
    );
    drop((destination, prefix));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !host.poll_retirement().unwrap() {
        assert!(
            std::time::Instant::now() < deadline,
            "the original material Host did not retire"
        );
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.retained_candidate_metadata_bytes(), 0);
    assert_eq!(host.cleanup_fault_count(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_material_generations_keep_two_layouts_last_good_and_late_original_capture_in_one_host() {
    let mut host = WgpuRenderHost::offscreen();
    let current = host.create_producer().unwrap();
    let delayed = host.create_producer().unwrap();
    let controls = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(SpriteFeature::<Color>::new());
    let current_feature = host.create_feature_instance(&current, &definition).unwrap();
    let delayed_feature = host.create_feature_instance(&delayed, &definition).unwrap();
    let controls_feature = host
        .create_feature_instance(&controls, &definition)
        .unwrap();
    let source = source();
    let (texture, [red, green]) = image_versions_with_handle(1, 1);
    let (other_texture, [_, other_green]) = image_versions_with_handle(1, 1);
    let mut library = material_library("test.host-material", AlphaMode2d::Opaque);
    let cold_compile_started = std::time::Instant::now();
    let a = publish_material(
        &mut library,
        "test.host-material",
        TEXTURE_MATERIAL_FRAGMENT,
    );
    let cold_compile = cold_compile_started.elapsed();
    let old = material_batch(
        material_binding(&a, [1.0; 4], texture),
        vec![(1, red.clone())],
    );
    let mut mixed_library = mixed_material_library("test.host-mixed");
    let mixed = publish_material(
        &mut mixed_library,
        "test.host-mixed",
        MIXED_MATERIAL_FRAGMENT,
    );
    assert_ne!(
        a.declaration().binding_digest(),
        mixed.declaration().binding_digest()
    );
    assert_eq!(a.declaration().parameters().byte_size(), 16);
    assert!(mixed.declaration().parameters().byte_size() > 16);
    let mixed_batch = material_batch(
        mixed_material_binding(
            &mixed,
            &MixedMaterialParameters::default(),
            texture,
            other_texture,
        ),
        vec![(2, red.clone()), (7, other_green)],
    );
    let (old_current_program, _) = captured_program(
        &current_feature,
        source,
        1,
        paired_batches(old.clone(), mixed_batch.clone()),
    );
    let (late_a_program, late_a_target) = captured_program(
        &delayed_feature,
        source,
        1,
        paired_batches(old, mixed_batch.clone()),
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (0, 0, 0)
    );
    let edit_compile_started = std::time::Instant::now();
    let b = publish_material(&mut library, "test.host-material", SWAPPED_FRAGMENT);
    let edit_compile = edit_compile_started.elapsed();
    assert_ne!(a, b);
    assert_eq!(
        a.declaration().binding_digest(),
        b.declaration().binding_digest()
    );
    assert_ne!(a.shader().digest(), b.shader().digest());
    let current_batch = material_batch(material_binding(&b, [1.0; 4], texture), vec![(1, red)]);
    let (b_program, b_target) = captured_program(
        &current_feature,
        source,
        2,
        paired_batches(current_batch.clone(), mixed_batch.clone()),
    );
    host.begin_frame(100).unwrap();
    let first_submit_started = std::time::Instant::now();
    let b_outputs = host.submit_program(&current, &b_program).unwrap();
    let first_submit = first_submit_started.elapsed();
    eprintln!(
        "material qualification: adapter={:?}, CPU initial compile={cold_compile:?}, CPU compatible edit={edit_compile:?}, first Host submission including device/pipelines={first_submit:?}",
        host.backend.adapter_summary(),
    );
    let b_output = b_outputs.texture(&b_target).unwrap();
    let namespace = (host.backend.instance_id(), host.backend.device_epoch());
    assert_halves(&mut host, &b_output, [0, 255, 0, 255]);
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    let invalid_path = nara_asset::AssetPath::new("native-material/fragment.wgsl").unwrap();
    assert!(
        library
            .prepare_update("test.host-material", &[invalid_path], |_| Ok(Arc::from(
                "this is not valid WGSL"
            )),)
            .is_err()
    );
    assert_eq!(library.current("test.host-material"), Some(&b));
    assert!(library.last_failure("test.host-material").is_some());
    let incompatible = format!(
        "{SWAPPED_FRAGMENT}\n@group(1) @binding(8) var incompatible_texture: texture_2d<f32>;"
    );
    let failure = library
        .prepare_update(
            "test.host-material",
            &[nara_asset::AssetPath::new("native-material/fragment.wgsl").unwrap()],
            |_| Ok(Arc::from(incompatible.as_str())),
        )
        .unwrap_err();
    assert!(matches!(
        failure,
        nara_material::MaterialUpdateError::Shader(nara_material::MaterialShaderError::Interface(
            _
        ))
    ));
    assert_eq!(library.current("test.host-material"), Some(&b));
    assert_halves(&mut host, &b_output, [0, 255, 0, 255]);
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    let mut mixed_parameters = MixedMaterialParameters {
        gain: 0.375,
        bias: nara_material::Vec3::new(0.125, 0.0625, 0.25),
        mode: 0,
        shift: nara_material::Vec2::new(-0.1875, 0.03125),
    };
    let mut mixed_values = mixed_batch.clone();
    mixed_values.material_extension.as_mut().unwrap().binding =
        mixed_material_binding(&mixed, &mixed_parameters, texture, other_texture);
    let (mixed_values_program, mixed_values_target) = captured_program(
        &controls_feature,
        source,
        1,
        paired_batches(current_batch.clone(), mixed_values),
    );
    let mixed_values_outputs = host
        .submit_program(&controls, &mixed_values_program)
        .unwrap();
    assert_mixed_halves(
        &mut host,
        &mixed_values_outputs.texture(&mixed_values_target).unwrap(),
        [143, 120, 64, 255],
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    mixed_parameters.mode = 1;
    let mut mixed_mode = mixed_batch.clone();
    mixed_mode.material_extension.as_mut().unwrap().binding =
        mixed_material_binding(&mixed, &mixed_parameters, texture, other_texture);
    let (mixed_mode_program, mixed_mode_target) = captured_program(
        &controls_feature,
        source,
        2,
        paired_batches(current_batch.clone(), mixed_mode),
    );
    let mixed_mode_outputs = host.submit_program(&controls, &mixed_mode_program).unwrap();
    assert_mixed_halves(
        &mut host,
        &mixed_mode_outputs.texture(&mixed_mode_target).unwrap(),
        [0, 0, 255, 255],
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    assert_eq!(
        host.views[&controls.id()].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(
        host.views[&current.id()].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(host.views[&delayed.id()].admission.last_frame_index, None);
    let mut values = current_batch.clone();
    values.material_extension.as_mut().unwrap().binding =
        material_binding(&b, [1.0, 0.0, 1.0, 1.0], texture);
    let (values_program, values_target) = captured_program(
        &current_feature,
        source,
        3,
        paired_batches(values, mixed_batch.clone()),
    );
    let values_outputs = host.submit_program(&current, &values_program).unwrap();
    assert_halves(
        &mut host,
        &values_outputs.texture(&values_target).unwrap(),
        [0, 0, 0, 255],
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    let mut textures = current_batch;
    textures.material_extension.as_mut().unwrap().textures = vec![(1, green.into())];
    let (textures_program, textures_target) = captured_program(
        &current_feature,
        source,
        4,
        paired_batches(textures, mixed_batch),
    );
    let textures_outputs = host.submit_program(&current, &textures_program).unwrap();
    assert_halves(
        &mut host,
        &textures_outputs.texture(&textures_target).unwrap(),
        [255, 0, 0, 255],
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 2)
    );
    assert_eq!(
        host.views[&current.id()].admission.last_frame_index,
        Some(4)
    );
    assert!(matches!(
        host.submit_program(&current, &old_current_program),
        Err(WgpuRenderHostError::Backend(
            WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 }
        ))
    ));
    assert_eq!(host.views[&delayed.id()].admission.last_frame_index, None);
    drop((mixed_library, a, mixed));
    let a_outputs = host.submit_program(&delayed, &late_a_program).unwrap();
    let a_output = a_outputs.texture(&late_a_target).unwrap();
    assert_eq!(library.current("test.host-material"), Some(&b));
    drop((library, b));
    assert_halves(&mut host, &a_output, [255, 0, 0, 255]);
    assert_halves(&mut host, &b_output, [0, 255, 0, 255]);
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (3, 3, 2)
    );
    assert_eq!(
        (host.backend.instance_id(), host.backend.device_epoch()),
        namespace
    );
    assert_eq!(
        host.views[&current.id()].admission.last_frame_index,
        Some(4)
    );
    assert_eq!(
        host.views[&delayed.id()].admission.last_frame_index,
        Some(1)
    );
    for outputs in [
        &b_outputs,
        &values_outputs,
        &textures_outputs,
        &a_outputs,
        &mixed_values_outputs,
        &mixed_mode_outputs,
    ] {
        drain(outputs.submitted_prefix());
    }
    drop((
        b_outputs,
        b_output,
        values_outputs,
        textures_outputs,
        a_outputs,
        a_output,
        mixed_values_outputs,
        mixed_mode_outputs,
    ));
    drain(&host.retire_producer(&current).unwrap());
    drain(&host.retire_producer(&delayed).unwrap());
    drain(&host.retire_producer(&controls).unwrap());
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_material_and_graph_allocation_demand_are_admitted_together_before_upload() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let feature = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(SpriteFeature::<Color>::new()),
        )
        .unwrap();
    let (left, [red, _]) = image_versions_with_handle(1, 1);
    let (right, [_, green]) = image_versions_with_handle(1, 1);
    let mut library = mixed_material_library("test.host-budget");
    let generation = publish_material(&mut library, "test.host-budget", MIXED_MATERIAL_FRAGMENT);
    let batch = material_batch(
        mixed_material_binding(
            &generation,
            &MixedMaterialParameters::default(),
            left,
            right,
        ),
        vec![(2, red), (7, green)],
    );
    let stock_bytes = host
        .backend
        .stock_quad_resources()
        .pending_allocation_bytes(std::iter::once(&batch))
        .unwrap();
    let (program, target) = captured_program(&feature, source(), 1, vec![batch]);
    host = host.with_resource_byte_limit(stock_bytes + 4 * 4 * 4 - 1);
    host.begin_frame(1).unwrap();
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(error.is_deferred(), "{error}");
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (0, 0, 0)
    );
    assert_eq!(
        host.backend.texture_stats(),
        crate::WgpuRenderTextureCacheStats::default()
    );
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    host = host.with_resource_byte_limit(stock_bytes + 4 * 4 * 4);
    let outputs = host.submit_program(&producer, &program).unwrap();
    assert!(
        pixels(&mut host, &outputs.texture(&target).unwrap())
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    drain(outputs.submitted_prefix());
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_shared_material_image_deduplicates_across_captures_without_deduplicating_buffers() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(SpriteFeature::<Color>::new());
    let first_feature = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let second_feature = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    let mut library = mixed_material_library("test.shared-material-image");
    let generation = publish_material(
        &mut library,
        "test.shared-material-image",
        MIXED_MATERIAL_FRAGMENT,
    );
    let batch = material_batch(
        mixed_material_binding(
            &generation,
            &MixedMaterialParameters::default(),
            texture,
            texture,
        ),
        vec![(2, red.clone()), (7, red)],
    );
    let uniform_bytes = batch
        .material_extension
        .as_ref()
        .unwrap()
        .binding
        .uniform_bytes()
        .len() as u64;
    let instance_bytes = std::mem::size_of::<crate::quad::WgpuQuadInstance>() as u64;
    let shared: Arc<[WgpuQuadBatch]> = paired_batches(batch.clone(), batch).into();
    let mut preflight = crate::stock_quad::StockImagePreflight::new(2).unwrap();
    preflight.collect(&shared).unwrap();
    preflight.collect(&shared).unwrap();
    assert_eq!(
        preflight.cost().unwrap().custody_entries,
        4 * (3 * 3 + 3 * 2 + 1)
    );
    let stock_bytes = preflight
        .allocation_bytes(host.backend.stock_quad_resources())
        .unwrap();
    let view_bytes = crate::view_uniform::VIEW_UNIFORM_BYTES;
    assert_eq!(
        stock_bytes,
        4 * (uniform_bytes + instance_bytes + view_bytes) + 4 + 4
    );
    let mut builder = WgpuRenderProgramBuilder::new(source(), 1, GraphLimits::default()).unwrap();
    let first = builder.texture::<Color>(descriptor()).unwrap();
    let second = builder.texture::<Color>(descriptor()).unwrap();
    let viewport =
        nara_render::ViewportRect::from_extent(nara_render::Extent2d::new(4, 4).unwrap()).unwrap();
    for (operation, feature, target) in [(1, &first_feature, &first), (2, &second_feature, &second)]
    {
        builder
            .operation(
                operation,
                feature,
                Arc::new(SpriteCapture::from_lowered(
                    target.clone(),
                    viewport,
                    Arc::clone(&shared),
                )),
                &[],
                &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )
            .unwrap();
    }
    let program = builder.finish(&[first.output(), second.output()]).unwrap();
    let output_bytes = 2 * 4 * 4 * 4;
    let total_bytes = stock_bytes + output_bytes;
    host = host.with_resource_byte_limit(total_bytes - 1);
    host.begin_frame(1).unwrap();
    let premature_allocation =
        crate::allocation::AllocationFailureGuard::at("nara_program_texture");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(error.is_deferred(), "{error}");
    assert!(!premature_allocation.was_consumed());
    assert_eq!(
        host.frame_transaction_stats().native_work(),
        crate::WgpuNativeWorkStats::default()
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (0, 0, 0)
    );
    assert_eq!(
        host.backend.texture_stats(),
        crate::WgpuRenderTextureCacheStats::default()
    );
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(host.views[&producer.id()].admission.last_frame_index, None);
    drop(premature_allocation);
    host = host.with_resource_byte_limit(total_bytes);
    let outputs = host.submit_program(&producer, &program).unwrap();
    let actual_bytes = total_bytes - 2 * view_bytes;
    assert_eq!(host.retained_resource_bytes(), actual_bytes);
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (1, 1, 1)
    );
    let textures = host.backend.texture_stats();
    assert_eq!(textures.image_textures, 1);
    assert_eq!(textures.uploaded_images + textures.reuploaded_images, 1);
    assert!(textures.has_fallback_texture);
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(work.issued_texture_write_calls, 2);
    assert_eq!(work.issued_texture_write_bytes, 8);
    assert_eq!(work.initialized_buffer_calls, 4 * 2 + 2);
    assert_eq!(
        work.initialized_buffer_payload_bytes,
        4 * (uniform_bytes + instance_bytes) + 2 * view_bytes
    );
    assert_eq!(
        work.initialized_buffer_storage_bytes,
        4 * (uniform_bytes + instance_bytes) + 2 * view_bytes
    );
    assert_eq!(outputs.report(1, &first_feature).unwrap().sprites, 2);
    assert_eq!(outputs.report(2, &second_feature).unwrap().sprites, 2);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    host.backend.stock_quad_resources().clear();
    assert_eq!(host.retained_resource_bytes(), actual_bytes);
    drain(outputs.submitted_prefix());
    assert_eq!(host.retained_resource_bytes(), output_bytes);
    for target in [&first, &second] {
        assert!(
            pixels(&mut host, &outputs.texture(target).unwrap())
                .iter()
                .all(|pixel| *pixel == [255, 0, 0, 255])
        );
    }
    drop(outputs);
    assert_eq!(host.retained_resource_bytes(), 0);
    drain(&host.retire_producer(&producer).unwrap());
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}

#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn native_partial_material_texture_oom_keeps_original_prefix_and_retries_the_same_frame() {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let feature = host
        .create_feature_instance(
            &producer,
            &WgpuFeatureDefinition::new(SpriteFeature::<Color>::new()),
        )
        .unwrap();
    let (left, [red, _]) = image_versions_with_handle(1, 1);
    let (right, [_, green]) = image_versions_with_handle(1, 1);
    let mut library = mixed_material_library("test.partial-material-oom");
    let generation = publish_material(
        &mut library,
        "test.partial-material-oom",
        MIXED_MATERIAL_FRAGMENT,
    );
    let batch = material_batch(
        mixed_material_binding(
            &generation,
            &MixedMaterialParameters {
                gain: 1.0,
                ..Default::default()
            },
            left,
            right,
        ),
        vec![(2, red.clone()), (7, green)],
    );
    let mut warm = batch.clone();
    warm.material_extension = None;
    warm.texture = crate::quad::WgpuQuadTexture::Image(red);
    warm.material.texture = warm.texture.key();
    let source = source();
    let (warm_program, warm_target) = captured_program(&feature, source, 1, vec![warm]);
    host.begin_frame(100).unwrap();
    let warm_outputs = host.submit_program(&producer, &warm_program).unwrap();
    assert!(
        pixels(&mut host, &warm_outputs.texture(&warm_target).unwrap())
            .iter()
            .all(|pixel| *pixel == [255, 0, 0, 255])
    );
    drain(warm_outputs.submitted_prefix());
    drop(warm_outputs);
    assert_eq!(host.retained_resource_bytes(), 4);
    assert_eq!(host.backend.texture_stats().image_textures, 1);
    assert!(!host.backend.texture_stats().has_fallback_texture);
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (1, 1, 0)
    );
    let namespace = (host.backend.instance_id(), host.backend.device_epoch());
    let (program, target) = captured_program(&feature, source, 2, vec![batch.clone()]);
    host.begin_frame(101).unwrap();
    let failure = crate::allocation::AllocationFailureGuard::at("nara_wgpu_image_texture");
    let error = host.submit_program(&producer, &program).unwrap_err();
    assert!(
        matches!(&error, WgpuRenderHostError::Backend(WgpuRenderError::UploadDeferred { message })
        if message.contains("nara_wgpu_image_texture") && message.contains("injected allocation failure")),
        "{error}"
    );
    assert!(failure.was_consumed());
    drop(failure);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    assert_eq!(
        host.backend.stock_quad_resources().instance_charges(),
        (0, 0)
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (2, 2, 1)
    );
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(work.issued_texture_write_calls, 1);
    assert_eq!(work.issued_texture_write_bytes, 4);
    assert_eq!(work.initialized_buffer_calls, 0);
    assert_eq!(work.initialized_buffer_payload_bytes, 0);
    let textures = host.backend.texture_stats();
    assert_eq!(textures.reused_images, 1);
    assert_eq!(textures.uploaded_images + textures.reuploaded_images, 0);
    assert_eq!(textures.image_textures, 1);
    assert!(textures.has_fallback_texture);
    let failed_prefix = host.views[&producer.id()].submitted_prefix.clone().unwrap();
    let failed_retained_bytes = 4 * 4 * 4 + 4 + 4 + 4;
    assert_eq!(host.retained_resource_bytes(), failed_retained_bytes);
    host.backend.stock_quad_resources().clear();
    assert_eq!(
        host.retained_resource_bytes(),
        failed_retained_bytes,
        "the original prefix owns the cached image view, fallback upload, failed allocation guard and target"
    );
    drain(&failed_prefix);
    assert_eq!(host.retained_resource_bytes(), 0);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(1)
    );
    let retry_bytes = host
        .backend
        .stock_quad_resources()
        .pending_allocation_bytes(std::iter::once(&batch))
        .unwrap()
        + 4 * 4 * 4;
    let outputs = host.submit_program(&producer, &program).unwrap();
    assert_eq!(host.retained_resource_bytes(), retry_bytes);
    assert_eq!(
        host.views[&producer.id()].admission.last_frame_index,
        Some(2)
    );
    assert_eq!(
        host.backend.stock_quad_resources().pipeline_stats(),
        (3, 1, 1)
    );
    assert_eq!(
        (host.backend.instance_id(), host.backend.device_epoch()),
        namespace
    );
    let work = host.frame_transaction_stats().native_work();
    assert_eq!(work.issued_texture_write_calls, 4);
    assert_eq!(work.issued_texture_write_bytes, 16);
    assert_eq!(work.initialized_buffer_calls, 3);
    assert_eq!(
        work.initialized_buffer_payload_bytes,
        batch
            .material_extension
            .as_ref()
            .unwrap()
            .binding
            .uniform_bytes()
            .len() as u64
            + std::mem::size_of::<crate::quad::WgpuQuadInstance>() as u64
            + crate::view_uniform::VIEW_UNIFORM_BYTES
    );
    host.backend.stock_quad_resources().clear();
    assert_eq!(host.retained_resource_bytes(), retry_bytes);
    drain(outputs.submitted_prefix());
    assert_eq!(host.retained_resource_bytes(), 4 * 4 * 4);
    assert!(
        pixels(&mut host, &outputs.texture(&target).unwrap())
            .iter()
            .all(|pixel| *pixel == [0, 255, 0, 255])
    );
    drop(outputs);
    assert_eq!(host.retained_resource_bytes(), 0);
    drain(&host.retire_producer(&producer).unwrap());
    while !host.poll_retirement().unwrap() {
        std::thread::yield_now();
    }
    assert_eq!(host.retained_resource_bytes(), 0);
}
