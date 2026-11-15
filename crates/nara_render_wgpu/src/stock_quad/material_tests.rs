use super::*;
use crate::{
    allocation::AllocationFailureGuard,
    output::OutputBudget,
    quad::{WgpuMaterial2dPipelineKey, WgpuQuadInstance, validate_material_device_limits},
    readback_tests::{
        MIXED_MATERIAL_FRAGMENT, MixedMaterialParameters, ReadbackGpu, TEXTURE_MATERIAL_FRAGMENT,
        TestMaterialParameters, image_versions_with_handle, material_batch, material_generation,
        mixed_material_binding, mixed_material_library, publish_material,
    },
};
use nara_material::{
    AlphaMode2d, Material2dDeclaration, Material2dGeneration, MaterialParameters,
    MaterialTextureDeclaration,
};
use std::sync::Weak;

fn mixed_batch() -> WgpuQuadBatch {
    let (left, [red, _]) = image_versions_with_handle(1, 1);
    let (right, [_, green]) = image_versions_with_handle(1, 1);
    let mut library = mixed_material_library("test.native-mixed");
    let generation = publish_material(&mut library, "test.native-mixed", MIXED_MATERIAL_FRAGMENT);
    material_batch(
        mixed_material_binding(
            &generation,
            &MixedMaterialParameters::default(),
            left,
            right,
        ),
        vec![(2, red), (7, green)],
    )
}

#[test]
fn native_immutable_view_bytes_have_distinct_bindings_and_stable_pipeline_cache() {
    let gpu = ReadbackGpu::new();
    let declaration = Material2dDeclaration::from_parameters::<TestMaterialParameters>(
        "test.immutable-view",
        1,
        AlphaMode2d::Opaque,
        Vec::new(),
    )
    .unwrap();
    let generation = Material2dGeneration::from_shader_source(
        declaration,
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(nara_view.viewport.x / 32.0, nara_view.real_elapsed, nara_view.virtual_elapsed, 1.0); }",
    )
    .unwrap();
    let binding = generation
        .bind(
            &TestMaterialParameters {
                value: nara_material::Vec4::ONE,
            },
            std::collections::BTreeMap::new(),
        )
        .unwrap();
    let mut original = material_batch(binding, Vec::new());
    original
        .material_extension
        .as_mut()
        .unwrap()
        .view
        .virtual_elapsed = 1.0;
    let mut later = original.clone();
    let later_view = &mut later.material_extension.as_mut().unwrap().view;
    later_view.viewport[0] = 32.0;
    later_view.real_elapsed = 1.0;
    later_view.virtual_elapsed = 0.0;
    let mut negative_zero = original.clone();
    negative_zero
        .material_extension
        .as_mut()
        .unwrap()
        .view
        .time_scale = -0.0;
    let mut paired = [
        original.clone(),
        later.clone(),
        original.clone(),
        negative_zero,
    ];
    paired[0].instances[0].center[0] = -0.5;
    paired[0].instances[0].x_axis[0] = 0.5;
    paired[1].instances[0].center[0] = 0.5;
    paired[1].instances[0].x_axis[0] = 0.5;
    paired[2].instances = paired[0].instances.clone();
    paired[3].instances = paired[0].instances.clone();
    let mut resources = StockQuadResources::default();
    for (frame, batches, left, right) in [
        (
            1,
            std::slice::from_ref(&later),
            [255, 255, 0, 255],
            [255, 255, 0, 255],
        ),
        (2, paired.as_slice(), [0, 0, 255, 255], [255, 255, 0, 255]),
        (
            3,
            std::slice::from_ref(&original),
            [0, 0, 255, 255],
            [0, 0, 255, 255],
        ),
        (4, paired.as_slice(), [0, 0, 255, 255], [255, 255, 0, 255]),
    ] {
        let (pixels, ()) = gpu
            .render_to_rgba(|_, _, format, output, encoder, preparation| {
                let draw = resources.prepare_batches(
                    preparation,
                    || Ok(()),
                    format,
                    batches,
                    frame,
                    None,
                )?;
                if draw.buffers.len() == 4 {
                    assert!(draw.buffers[0].captured_view_bind_group().is_some());
                    assert_ne!(
                        draw.buffers[0].captured_view_bind_group(),
                        draw.buffers[1].captured_view_bind_group(),
                    );
                    assert_eq!(
                        draw.buffers[0].captured_view_bind_group(),
                        draw.buffers[2].captured_view_bind_group(),
                    );
                    assert_ne!(
                        draw.buffers[0].captured_view_bind_group(),
                        draw.buffers[3].captured_view_bind_group(),
                    );
                    assert_eq!(preparation.work_stats().initialized_buffer_calls, 4 * 2 + 3);
                }
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("immutable-view-packing"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: output,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                crate::quad::draw_quad_batch_buffers(
                    &mut pass,
                    &draw.pipelines,
                    &draw.buffers,
                    crate::readback_tests::readback_viewport(),
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            crate::readback_tests::readback_pixel(&pixels, 16, 32),
            &left
        );
        assert_eq!(
            crate::readback_tests::readback_pixel(&pixels, 48, 32),
            &right
        );
        assert_eq!(resources.pipeline_stats(), (1, 1, 1));
    }
    resources.clear();
    assert_eq!(gpu.budget.retained(), 0);
}

#[test]
fn native_view_sharing_table_is_bounded_and_original_prefix_owns_every_unique_allocation() {
    let gpu = ReadbackGpu::new();
    let base = mixed_batch();
    let limit = crate::quad::MAX_SHARED_VIEW_BINDINGS;
    let mut batches = (0..limit + 2)
        .map(|index| {
            let mut batch = base.clone();
            batch.material_extension.as_mut().unwrap().view.frame = [index as u32, 0];
            batch
        })
        .collect::<Vec<_>>();
    batches.push(batches[0].clone());
    batches.push(batches[limit + 1].clone());
    let unique_allocations = limit + 3;
    let uniform_bytes = base
        .material_extension
        .as_ref()
        .unwrap()
        .binding
        .uniform_bytes()
        .len() as u64;
    let buffer_bytes = batches.len() as u64
        * (size_of::<WgpuQuadInstance>() as u64 + uniform_bytes)
        + unique_allocations as u64 * VIEW_UNIFORM_BYTES;
    let mut resources = StockQuadResources::default();
    let conservative = resources.pending_allocation_bytes(batches.iter()).unwrap();
    let actual = conservative - VIEW_UNIFORM_BYTES;
    let cost = StockPreparationCost::for_batches(batches.iter()).unwrap();
    let (result, prefix) = gpu.with_preparation(|preparation| {
        preparation.reserve_entries(cost.custody_entries)?;
        let draw = resources.prepare_batches(
            preparation,
            || Ok(()),
            wgpu::TextureFormat::Rgba8Unorm,
            &batches,
            1,
            None,
        )?;
        assert_eq!(draw.buffers.len(), batches.len());
        assert_eq!(
            draw.buffers[0].captured_view_bind_group(),
            draw.buffers[limit + 2].captured_view_bind_group(),
        );
        assert_ne!(
            draw.buffers[limit + 1].captured_view_bind_group(),
            draw.buffers[limit + 3].captured_view_bind_group(),
        );
        let work = preparation.work_stats();
        assert_eq!(
            work.initialized_buffer_calls as usize,
            2 * batches.len() + unique_allocations
        );
        assert_eq!(work.initialized_buffer_payload_bytes, buffer_bytes);
        assert_eq!(work.initialized_buffer_storage_bytes, buffer_bytes);
        assert_eq!(work.issued_texture_write_calls, 3);
        assert_eq!(work.issued_texture_write_bytes, 12);
        assert_eq!(gpu.budget.retained(), actual);
        resources.clear();
        drop(draw);
        assert_eq!(gpu.budget.retained(), actual);
        Ok(((), gpu.queue.submit([])))
    });
    result.unwrap();
    assert_eq!(gpu.budget.retained(), actual);
    gpu.wait_for_prefix(&prefix);
    assert_eq!(gpu.budget.retained(), 0);
}

fn source_retained_generation(
    source_generations: &Arc<OutputBudget>,
    fragment: &str,
) -> (Material2dGeneration, Weak<OutputReservation>) {
    let owner = Arc::new(source_generations.reserve(1).unwrap());
    let weak = Arc::downgrade(&owner);
    let declaration = Material2dDeclaration::from_parameters::<TestMaterialParameters>(
        "test.cache-source-retention",
        1,
        AlphaMode2d::Opaque,
        vec![MaterialTextureDeclaration::new("texture", "material_texture", 1).unwrap()],
    )
    .unwrap();
    let generation = Material2dGeneration::from_shader_source(declaration, fragment)
        .unwrap()
        .with_lifetime_owner(owner);
    (generation, weak)
}

#[test]
fn native_ready_cache_releases_source_generations_and_draw_refs_keep_actual_captures() {
    let gpu = ReadbackGpu::new();
    let mut resources = StockQuadResources::default();
    let source_generations = Arc::new(OutputBudget::new(2));
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    for revision in 0..3 {
        let fragment = format!(
            "{TEXTURE_MATERIAL_FRAGMENT}\nconst source_retention_revision: u32 = {revision}u;"
        );
        let mut draws = Vec::new();
        let mut owners = Vec::new();
        for _ in 0..2 {
            let (generation, owner) = source_retained_generation(&source_generations, &fragment);
            let binding = crate::readback_tests::material_binding(&generation, [1.0; 4], texture);
            let batches = [material_batch(binding.clone(), vec![(1, red.clone())])];
            let keys =
                crate::required_quad_pipeline_keys(&batches, wgpu::TextureFormat::Rgba8Unorm);
            assert_eq!(keys.len(), 1);
            resources
                .ensure_pipeline(&gpu.device, &|| Ok(()), &keys[0])
                .unwrap();
            let prepared = resources.pipeline_draw_refs(&keys).unwrap();
            assert_eq!(prepared.len(), 1);
            drop(keys);
            drop(batches);
            drop(binding);
            drop(generation);
            assert!(owner.upgrade().is_some());
            draws.push(prepared);
            owners.push(owner);
        }
        assert_eq!(resources.pipeline_stats(), (revision + 1, revision + 1, 1));
        assert!(
            resources
                .pipelines
                .iter()
                .all(|entry| matches!(entry, CachedQuadPipeline::Ready { .. }))
        );
        assert_eq!(source_generations.retained(), 2);
        assert!(source_generations.reserve(1).is_none());
        let second = draws.pop().unwrap();
        drop(draws);
        assert!(owners[0].upgrade().is_none());
        assert!(owners[1].upgrade().is_some());
        assert_eq!(source_generations.retained(), 1);
        drop(second);
        assert!(owners[1].upgrade().is_none());
        assert_eq!(source_generations.retained(), 0);
        resources.trim_pipeline_cache();
        assert_eq!(resources.pipeline_stats(), (revision + 1, revision + 1, 1));
        assert_eq!(gpu.budget.retained(), 0);
    }
}

#[test]
fn native_rejected_cache_releases_source_generations_with_two_exact_live_leases() {
    let gpu = ReadbackGpu::new();
    let mut resources = StockQuadResources::default();
    let source_generations = Arc::new(OutputBudget::new(2));
    let (retained, retained_owner) =
        source_retained_generation(&source_generations, TEXTURE_MATERIAL_FRAGMENT);
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    for revision in 0..3 {
        let fragment = format!(
            "{TEXTURE_MATERIAL_FRAGMENT}\nconst source_retention_revision: u32 = {revision}u;"
        );
        let (generation, owner) = source_retained_generation(&source_generations, &fragment);
        let binding = crate::readback_tests::material_binding(&generation, [1.0; 4], texture);
        let batches = [material_batch(binding.clone(), vec![(1, red.clone())])];
        let keys = crate::required_quad_pipeline_keys(&batches, wgpu::TextureFormat::Rgba8Unorm);
        assert_eq!(keys.len(), 1);
        let invalid =
            crate::quad::QuadShaderSourceGuard::invalid(keys[0].material.as_ref().unwrap());
        assert_eq!(source_generations.retained(), 2);
        assert!(source_generations.reserve(1).is_none());
        for _ in 0..2 {
            let (result, prefix) = gpu.with_preparation::<()>(|preparation| {
                let error = resources
                    .prepare_batches(
                        preparation,
                        || Ok(()),
                        wgpu::TextureFormat::Rgba8Unorm,
                        &batches,
                        revision as u64,
                        None,
                    )
                    .unwrap_err();
                assert!(matches!(error, WgpuRenderError::ContentRejected { .. }));
                assert_eq!(
                    preparation.work_stats(),
                    crate::WgpuNativeWorkStats::default()
                );
                Err(error)
            });
            assert!(result.is_err());
            gpu.wait_for_prefix(&prefix);
        }
        drop(invalid);
        drop(keys);
        drop(batches);
        drop(binding);
        drop(generation);
        assert!(owner.upgrade().is_none());
        assert!(retained_owner.upgrade().is_some());
        assert_eq!(source_generations.retained(), 1);
        assert_eq!(resources.pipeline_stats(), (revision + 1, revision + 1, 1));
        assert!(
            resources
                .pipelines
                .iter()
                .all(|entry| matches!(entry, CachedQuadPipeline::Rejected { .. }))
        );
        assert_eq!(resources.instance_charges(), (0, 0));
        assert_eq!(gpu.budget.retained(), 0);
    }
    drop(retained);
    assert!(retained_owner.upgrade().is_none());
    assert_eq!(source_generations.retained(), 0);
    assert_eq!(resources.pipeline_stats(), (3, 3, 1));
}

#[test]
fn native_rejected_shader_history_uses_the_same_bounded_cache_and_never_uploads() {
    let gpu = ReadbackGpu::new();
    let mut resources = StockQuadResources::default();
    let mut library =
        crate::readback_tests::material_library("test.failed-edits", AlphaMode2d::Opaque);
    let (texture, [red, _]) = image_versions_with_handle(1, 1);
    let limit = MAX_CACHED_QUAD_PIPELINES;
    let mut first = None;
    for revision in 0..limit + 2 {
        let generation = publish_material(
            &mut library,
            "test.failed-edits",
            &format!(
                "{TEXTURE_MATERIAL_FRAGMENT}\nconst native_rejection_revision: u32 = {revision}u;"
            ),
        );
        let batch = material_batch(
            crate::readback_tests::material_binding(&generation, [1.0; 4], texture),
            vec![(1, red.clone())],
        );
        first.get_or_insert_with(|| batch.clone());
        let key = WgpuMaterial2dPipelineKey::new(&generation);
        let _invalid = crate::quad::QuadShaderSourceGuard::invalid(&key);
        let (result, prefix) = gpu.with_preparation::<()>(|preparation| {
            let error = resources
                .prepare_batches(
                    preparation,
                    || Ok(()),
                    wgpu::TextureFormat::Rgba8Unorm,
                    std::slice::from_ref(&batch),
                    revision as u64,
                    None,
                )
                .unwrap_err();
            assert!(matches!(error, WgpuRenderError::ContentRejected { .. }));
            assert_eq!(
                preparation.work_stats(),
                crate::WgpuNativeWorkStats::default()
            );
            Err(error)
        });
        assert!(result.is_err());
        gpu.wait_for_prefix(&prefix);
        assert_eq!(
            resources.pipeline_stats(),
            (revision + 1, (revision + 1).min(limit), 1)
        );
        assert!(
            resources
                .pipelines
                .iter()
                .all(|entry| matches!(entry, CachedQuadPipeline::Rejected { .. }))
        );
        assert!(resources.material_layouts.len() <= resources.pipelines.len());
        assert_eq!(gpu.budget.retained(), 0);
    }
    let first = first.unwrap();
    let key = WgpuMaterial2dPipelineKey::new(
        first
            .material_extension
            .as_ref()
            .unwrap()
            .binding
            .generation(),
    );
    let _invalid = crate::quad::QuadShaderSourceGuard::invalid(&key);
    let (result, prefix) = gpu.with_preparation::<()>(|preparation| {
        let error = resources
            .prepare_batches(
                preparation,
                || Ok(()),
                wgpu::TextureFormat::Rgba8Unorm,
                std::slice::from_ref(&first),
                100,
                None,
            )
            .unwrap_err();
        assert!(matches!(error, WgpuRenderError::ContentRejected { .. }));
        assert_eq!(
            preparation.work_stats(),
            crate::WgpuNativeWorkStats::default()
        );
        Err(error)
    });
    assert!(result.is_err());
    gpu.wait_for_prefix(&prefix);
    assert_eq!(resources.pipeline_stats(), (limit + 3, limit, 1));
    assert_eq!(gpu.budget.retained(), 0);
}

#[test]
fn pipeline_identity_uses_artifact_and_abi_instead_of_library_or_semantic_identity() {
    let a = material_generation(
        "test.identity-a",
        AlphaMode2d::Opaque,
        TEXTURE_MATERIAL_FRAGMENT,
    );
    let b = material_generation(
        "test.identity-b",
        AlphaMode2d::Opaque,
        TEXTURE_MATERIAL_FRAGMENT,
    );
    assert_ne!(a, b);
    assert_ne!(
        a.declaration().semantic_digest(),
        b.declaration().semantic_digest()
    );
    assert_eq!(a.shader().source(), b.shader().source());
    assert_eq!(
        WgpuMaterial2dPipelineKey::new(&a),
        WgpuMaterial2dPipelineKey::new(&b)
    );
    let remapped_layout = nara_material::MaterialParameterLayout::new(
        vec![
            nara_material::MaterialUniformField::new(
                "semantic.other-value",
                "value",
                nara_material::MaterialParameterKind::Vec4,
                0,
            )
            .unwrap(),
        ],
        TestMaterialParameters::parameter_layout()
            .unwrap()
            .byte_size(),
    )
    .unwrap();
    let mut library = nara_material::Material2dLibrary::default();
    library
        .declare(
            nara_material::Material2dDeclaration::new(
                "test.identity-a",
                1,
                AlphaMode2d::Opaque,
                remapped_layout,
                vec![
                    nara_material::MaterialTextureDeclaration::new(
                        "semantic.other-texture",
                        "material_texture",
                        1,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let remapped = publish_material(&mut library, "test.identity-a", TEXTURE_MATERIAL_FRAGMENT);
    assert_ne!(
        a.shader().interface_digest(),
        remapped.shader().interface_digest()
    );
    assert_eq!(a.shader().digest(), remapped.shader().digest());
    assert_eq!(
        WgpuMaterial2dPipelineKey::new(&a),
        WgpuMaterial2dPipelineKey::new(&remapped)
    );
    let c = material_generation(
        "test.identity-a",
        AlphaMode2d::Opaque,
        &format!("{TEXTURE_MATERIAL_FRAGMENT}\n"),
    );
    assert_eq!(
        a.declaration().binding_digest(),
        c.declaration().binding_digest()
    );
    assert_ne!(
        WgpuMaterial2dPipelineKey::new(&a),
        WgpuMaterial2dPipelineKey::new(&c)
    );
}

#[test]
fn material_device_limits_admit_equality_and_reject_each_actual_upper_bound() {
    let batch = mixed_batch();
    let declaration = batch
        .material_extension
        .as_ref()
        .unwrap()
        .binding
        .generation()
        .declaration();
    let size = u64::from(declaration.parameters().byte_size());
    assert!(size > 16);
    let size = size.max(VIEW_UNIFORM_BYTES);
    let limits = wgpu::Limits {
        max_uniform_buffer_binding_size: size,
        max_buffer_size: size,
        max_sampled_textures_per_shader_stage: 3,
        max_bindings_per_bind_group: 8,
        max_bind_groups: 3,
        max_bind_groups_plus_vertex_buffers: 4,
        max_uniform_buffers_per_shader_stage: 2,
        max_samplers_per_shader_stage: 1,
        ..Default::default()
    };
    validate_material_device_limits(declaration, &limits).unwrap();
    for (name, reduced) in [
        (
            "max_uniform_buffer_binding_size",
            wgpu::Limits {
                max_uniform_buffer_binding_size: size - 1,
                ..limits.clone()
            },
        ),
        (
            "max_buffer_size",
            wgpu::Limits {
                max_buffer_size: size - 1,
                ..limits.clone()
            },
        ),
        (
            "max_sampled_textures_per_shader_stage",
            wgpu::Limits {
                max_sampled_textures_per_shader_stage: 2,
                ..limits.clone()
            },
        ),
        (
            "max_bindings_per_bind_group",
            wgpu::Limits {
                max_bindings_per_bind_group: 7,
                ..limits.clone()
            },
        ),
        (
            "max_bind_groups",
            wgpu::Limits {
                max_bind_groups: 2,
                ..limits.clone()
            },
        ),
        (
            "max_bind_groups_plus_vertex_buffers",
            wgpu::Limits {
                max_bind_groups_plus_vertex_buffers: 3,
                ..limits.clone()
            },
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            wgpu::Limits {
                max_uniform_buffers_per_shader_stage: 1,
                ..limits.clone()
            },
        ),
        (
            "max_samplers_per_shader_stage",
            wgpu::Limits {
                max_samplers_per_shader_stage: 0,
                ..limits.clone()
            },
        ),
    ] {
        let error = validate_material_device_limits(declaration, &reduced).unwrap_err();
        assert!(!error.is_deferred());
        assert!(error.to_string().contains(name), "{error}");
    }
}

#[test]
fn material_limits_include_the_primary_texture_and_zero_texture_uniforms_keep_custody() {
    let declaration =
        nara_material::Material2dDeclaration::from_parameters::<TestMaterialParameters>(
            "test.eight-textures",
            1,
            AlphaMode2d::Opaque,
            (1..=8)
                .map(|binding| {
                    nara_material::MaterialTextureDeclaration::new(
                        format!("texture-{binding}"),
                        format!("material_texture_{binding}"),
                        binding,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap();
    let mut limits = wgpu::Limits {
        max_sampled_textures_per_shader_stage: 9,
        max_bindings_per_bind_group: 9,
        ..Default::default()
    };
    validate_material_device_limits(&declaration, &limits).unwrap();
    limits.max_sampled_textures_per_shader_stage = 8;
    assert!(validate_material_device_limits(&declaration, &limits).is_err());
    let mut library = nara_material::Material2dLibrary::default();
    library
        .declare(
            nara_material::Material2dDeclaration::from_parameters::<TestMaterialParameters>(
                "test.uniform-only",
                1,
                AlphaMode2d::Opaque,
                Vec::new(),
            )
            .unwrap(),
        )
        .unwrap();
    let generation = publish_material(
        &mut library,
        "test.uniform-only",
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return material_parameters.value * tint; }",
    );
    let binding = generation
        .bind(
            &TestMaterialParameters {
                value: nara_material::Vec4::ONE,
            },
            std::collections::BTreeMap::new(),
        )
        .unwrap();
    let batch = material_batch(binding, Vec::new());
    let resources = StockQuadResources::default();
    resources
        .preflight_material_device_limits(std::iter::once(&batch), &limits)
        .unwrap();
    assert_eq!(
        StockPreparationCost::for_batches(std::iter::once(&batch))
            .unwrap()
            .custody_entries,
        10
    );
    assert_eq!(
        resources
            .pending_allocation_bytes(std::iter::once(&batch))
            .unwrap(),
        size_of::<WgpuQuadInstance>() as u64 + 16 + VIEW_UNIFORM_BYTES + 4
    );
}

#[test]
fn variable_material_cost_counts_each_texture_and_uniform_before_native_work() {
    let batch = mixed_batch();
    let material = batch.material_extension.as_ref().unwrap();
    let cost = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
    assert_eq!(cost.custody_entries, 3 * 3 + 3 * 2 + 1);
    assert!(cost.modeled_metadata_bytes >= material.binding.uniform_bytes().len());
    let resources = StockQuadResources::default();
    let expected = size_of::<WgpuQuadInstance>() as u64
        + material.binding.uniform_bytes().len() as u64
        + VIEW_UNIFORM_BYTES
        + 4
        + 4
        + 4;
    assert_eq!(
        resources
            .pending_allocation_bytes(std::iter::once(&batch))
            .unwrap(),
        expected
    );
    let mut duplicated = batch.clone();
    duplicated
        .material_extension
        .as_mut()
        .unwrap()
        .textures
        .reverse();
    assert!(
        resources
            .preflight_material_device_limits(
                std::iter::once(&duplicated),
                &wgpu::Limits::default()
            )
            .is_err()
    );
}

#[test]
fn shared_snapshot_preflight_deduplicates_textures_but_keeps_every_capture_occurrence() {
    let (texture, [red, green]) = image_versions_with_handle(1, 1);
    let mut library = mixed_material_library("test.shared-capture-cost");
    let generation = publish_material(
        &mut library,
        "test.shared-capture-cost",
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
    let shared: Arc<[WgpuQuadBatch]> = Arc::from([batch.clone(), batch]);
    let first = shared[0].material_extension.as_ref().unwrap().textures[0]
        .1
        .image()
        .unwrap();
    let last = shared[1].material_extension.as_ref().unwrap().textures[1]
        .1
        .image()
        .unwrap();
    assert_eq!(first.identity(), last.identity());
    assert!(std::ptr::eq(
        first.resource().pixels(),
        last.resource().pixels()
    ));
    let resources = StockQuadResources::default();
    let mut preflight = StockImagePreflight::new(2).unwrap();
    preflight.collect(&shared).unwrap();
    preflight.collect(&shared).unwrap();
    let cost = preflight.cost().unwrap();
    assert_eq!(cost.custody_entries, 4 * (3 * 3 + 3 * 2 + 1));
    let expected =
        4 * (size_of::<WgpuQuadInstance>() as u64 + uniform_bytes + VIEW_UNIFORM_BYTES) + 4 + 4;
    assert_eq!(preflight.allocation_bytes(&resources).unwrap(), expected);
    let mut different_version = shared.to_vec();
    different_version[1]
        .material_extension
        .as_mut()
        .unwrap()
        .textures[1]
        .1 = green.into();
    assert_eq!(
        StockPreparationCost::for_batches(shared.iter().chain(different_version.iter()),).unwrap(),
        cost
    );
    assert_eq!(
        resources
            .pending_allocation_bytes(shared.iter().chain(different_version.iter()),)
            .unwrap(),
        expected + 4
    );
}

#[test]
fn native_enabled_material_limits_reject_before_layout_and_upload() {
    for (name, limits) in [
        (
            "max_uniform_buffer_binding_size",
            wgpu::Limits {
                max_uniform_buffer_binding_size: 16,
                ..Default::default()
            },
        ),
        (
            "max_sampled_textures_per_shader_stage",
            wgpu::Limits {
                max_sampled_textures_per_shader_stage: 2,
                ..Default::default()
            },
        ),
        (
            "max_bindings_per_bind_group",
            wgpu::Limits {
                max_bindings_per_bind_group: 7,
                ..Default::default()
            },
        ),
    ] {
        let gpu = ReadbackGpu::with_limits(limits);
        let batch = mixed_batch();
        let mut resources = StockQuadResources::default();
        let failure = AllocationFailureGuard::at("nara_wgpu_quad_texture_bind_group_layout");
        let (result, prefix) = gpu.with_preparation::<()>(|preparation| {
            let error = resources.prepare_batches(
                preparation, || Ok(()), wgpu::TextureFormat::Rgba8Unorm,
                std::slice::from_ref(&batch), 1, None,
            ).unwrap_err();
            assert!(matches!(&error, WgpuRenderError::ContentRejected { message } if message.contains(name)), "{error}");
            assert_eq!(preparation.work_stats(), crate::WgpuNativeWorkStats::default());
            Err(error)
        });
        assert!(result.is_err());
        assert!(!failure.was_consumed());
        assert_eq!(resources.pipeline_stats(), (0, 0, 0));
        assert_eq!(
            resources.texture_stats(),
            crate::WgpuRenderTextureCacheStats::default()
        );
        assert_eq!(resources.instance_charges(), (0, 0));
        assert_eq!(gpu.budget.retained(), 0);
        gpu.wait_for_prefix(&prefix);
    }
}

#[test]
fn native_variable_material_budget_refuses_before_partial_upload_and_original_prefix_owns_all() {
    let gpu = ReadbackGpu::new();
    let batch = mixed_batch();
    let expected = StockQuadResources::default()
        .pending_allocation_bytes(std::iter::once(&batch))
        .unwrap();
    let mut resources = StockQuadResources::default();
    gpu.budget.set_limit(expected - 1);
    let (rejected, prefix) = gpu.with_preparation::<()>(|preparation| {
        let error = resources
            .prepare_batches(
                preparation,
                || Ok(()),
                wgpu::TextureFormat::Rgba8Unorm,
                std::slice::from_ref(&batch),
                1,
                None,
            )
            .unwrap_err();
        assert!(matches!(error, WgpuRenderError::UploadDeferred { .. }));
        assert_eq!(
            preparation.work_stats(),
            crate::WgpuNativeWorkStats::default()
        );
        Err(error)
    });
    assert!(rejected.is_err());
    assert_eq!(resources.pipeline_stats(), (0, 0, 0));
    assert_eq!(gpu.budget.retained(), 0);
    gpu.wait_for_prefix(&prefix);
    gpu.budget.set_limit(expected);
    let (prepared, prefix) = gpu.with_preparation(|preparation| {
        let draw = resources.prepare_batches(
            preparation,
            || Ok(()),
            wgpu::TextureFormat::Rgba8Unorm,
            std::slice::from_ref(&batch),
            2,
            None,
        )?;
        assert_eq!(draw.buffers.len(), 1);
        assert_eq!(preparation.work_stats().issued_texture_write_calls, 3);
        assert_eq!(
            preparation.work_stats().initialized_buffer_payload_bytes,
            size_of::<WgpuQuadInstance>() as u64
                + VIEW_UNIFORM_BYTES
                + MixedMaterialParameters::parameter_layout()
                    .unwrap()
                    .byte_size() as u64
        );
        assert_eq!(gpu.budget.retained(), expected);
        resources.clear();
        drop(draw);
        assert_eq!(gpu.budget.retained(), expected);
        Ok(((), gpu.queue.submit([])))
    });
    prepared.unwrap();
    assert_eq!(
        gpu.budget.retained(),
        expected,
        "only original prefix custody remains"
    );
    gpu.wait_for_prefix(&prefix);
    assert_eq!(gpu.budget.retained(), 0);
}

#[cfg(feature = "stock-sprite")]
fn material_program_descriptor() -> crate::WgpuProgramTextureDescriptor {
    crate::WgpuProgramTextureDescriptor {
        native: wgpu::TextureDescriptor {
            label: Some("test-material-program-texture"),
            size: wgpu::Extent3d {
                width: 3,
                height: 2,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        },
        transfer: crate::TextureTransfer::Linear,
        alpha: crate::TextureAlpha::Opaque,
    }
}

#[cfg(feature = "stock-sprite")]
fn material_program_source(
    material: &nara_material::Material2dBinding,
    texture_id: &str,
) -> crate::material_textures::WgpuMaterialTextureSource {
    let mut builder = crate::WgpuRenderProgramBuilder::new(
        nara_render::RenderFrameSourceGeneration::try_acquire().unwrap(),
        1,
        crate::GraphLimits::default(),
    )
    .unwrap();
    let texture = builder
        .texture::<()>(material_program_descriptor())
        .unwrap();
    crate::WgpuMaterialTextureBinding::new(material, texture_id, &texture)
        .unwrap()
        .source()
}

#[cfg(feature = "stock-sprite")]
#[test]
fn program_material_textures_replace_only_image_demand_and_preserve_capture_validation() {
    let (left, [red, _]) = image_versions_with_handle(64, 1);
    let (right, [_, green]) = image_versions_with_handle(32, 1);
    let mut library = mixed_material_library("test.program-material-cost");
    let generation = publish_material(
        &mut library,
        "test.program-material-cost",
        MIXED_MATERIAL_FRAGMENT,
    );
    let binding = mixed_material_binding(
        &generation,
        &MixedMaterialParameters::default(),
        left,
        right,
    );
    let source = material_program_source(&binding, "left");
    let mut batch = material_batch(binding.clone(), vec![(2, red), (7, green)]);
    let resources = StockQuadResources::default();
    let images = crate::texture::WgpuSpriteTextureCache::with_image_residency_limits(0, 0);
    let limits = wgpu::Limits {
        max_texture_dimension_2d: 8,
        ..Default::default()
    };
    let original_bytes = resources
        .pending_allocation_bytes(std::iter::once(&batch))
        .unwrap();
    let original_cost = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
    assert!(
        resources
            .preflight_material_device_limits(std::iter::once(&batch), &limits)
            .is_err()
    );
    assert!(
        images
            .preflight_image_residency(std::iter::once(&batch))
            .is_err()
    );
    batch.material_extension.as_mut().unwrap().textures[0].1 =
        crate::quad::WgpuMaterialTexture::Program(source.clone());
    assert_eq!(
        StockPreparationCost::for_batches(std::iter::once(&batch))
            .unwrap()
            .custody_entries,
        original_cost.custody_entries - 3,
    );
    assert_eq!(
        resources
            .pending_allocation_bytes(std::iter::once(&batch))
            .unwrap(),
        original_bytes - 64 * 4,
    );
    assert!(
        resources
            .preflight_material_device_limits(std::iter::once(&batch), &limits)
            .is_err()
    );
    batch.material_extension.as_mut().unwrap().textures[1].1 =
        crate::quad::WgpuMaterialTexture::Program(source);
    resources
        .preflight_material_device_limits(std::iter::once(&batch), &limits)
        .unwrap();
    images
        .preflight_image_residency(std::iter::once(&batch))
        .unwrap();
    let cost = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
    assert_eq!(cost.custody_entries, 10);
    assert!(cost.modeled_metadata_bytes < original_cost.modeled_metadata_bytes);
    assert_eq!(
        resources
            .pending_allocation_bytes(std::iter::once(&batch))
            .unwrap(),
        original_bytes - (64 + 32) * 4,
    );
    let material = batch.material_extension.as_mut().unwrap();
    assert_eq!(material.binding, binding);
    assert!(
        material
            .textures
            .iter()
            .all(|(_, texture)| texture.image().is_none())
    );
    crate::quad::validate_material_capture(material).unwrap();
    material.textures[0].0 = 3;
    assert!(crate::quad::validate_material_capture(material).is_err());
    material.textures[0].0 = 2;
    let [foreign_image, _] = crate::readback_tests::two_image_versions();
    material.textures[1].1 = foreign_image.into();
    assert!(crate::quad::validate_material_capture(material).is_err());
}

#[cfg(feature = "stock-sprite")]
#[test]
fn program_material_without_original_operation_resources_rejects_before_native_work() {
    let gpu = ReadbackGpu::new();
    let mut batch = mixed_batch();
    let material = batch.material_extension.as_mut().unwrap();
    material.textures[0].1 = crate::quad::WgpuMaterialTexture::Program(material_program_source(
        &material.binding,
        "left",
    ));
    let mut resources = StockQuadResources::default();
    let failure = AllocationFailureGuard::at("nara_wgpu_quad_texture_bind_group_layout");
    let (result, prefix) = gpu.with_preparation::<()>(|preparation| {
        let error = resources
            .prepare_batches(
                preparation,
                || Ok(()),
                wgpu::TextureFormat::Rgba8Unorm,
                std::slice::from_ref(&batch),
                1,
                None,
            )
            .unwrap_err();
        assert!(
            matches!(&error, WgpuRenderError::ContentRejected { message }
            if message.contains("original operation resources"))
        );
        assert_eq!(
            preparation.work_stats(),
            crate::WgpuNativeWorkStats::default()
        );
        Err(error)
    });
    assert!(result.is_err());
    assert!(!failure.was_consumed());
    assert_eq!(resources.pipeline_stats(), (0, 0, 0));
    assert_eq!(
        resources.texture_stats(),
        crate::WgpuRenderTextureCacheStats::default()
    );
    assert_eq!(resources.instance_charges(), (0, 0));
    assert_eq!(gpu.budget.retained(), 0);
    gpu.wait_for_prefix(&prefix);
}

#[cfg(feature = "stock-sprite")]
#[test]
#[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
fn program_material_requires_the_exact_sampled_read_before_any_program_allocation() {
    use crate::sprite::{SpriteCapture, SpriteFeature};
    use crate::{AccessRole, WgpuFeatureDefinition, WgpuRenderHost, WgpuRenderProgramBuilder};

    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let definition = WgpuFeatureDefinition::new(SpriteFeature::<()>::new());
    let feature = host
        .create_feature_instance(&producer, &definition)
        .unwrap();
    let source = nara_render::RenderFrameSourceGeneration::try_acquire().unwrap();
    for (index, role) in [None, Some(AccessRole::CopySource)].into_iter().enumerate() {
        let frame = index as u64 + 1;
        let mut builder =
            WgpuRenderProgramBuilder::new(source, frame, crate::GraphLimits::default()).unwrap();
        let palette = builder
            .texture::<()>(material_program_descriptor())
            .unwrap();
        let target = builder
            .texture::<()>(material_program_descriptor())
            .unwrap();
        builder
            .initialize(
                1,
                &[palette.clear(wgpu::Color::GREEN, AccessRole::Attachment)],
            )
            .unwrap();
        let mut batch = mixed_batch();
        let material = batch.material_extension.as_mut().unwrap();
        material.textures[0].1 = crate::quad::WgpuMaterialTexture::Program(
            crate::WgpuMaterialTextureBinding::new(&material.binding, "left", &palette)
                .unwrap()
                .source(),
        );
        let reads = role
            .map(|role| palette.read(role))
            .into_iter()
            .collect::<Vec<_>>();
        builder
            .operation(
                2,
                &feature,
                Arc::new(SpriteCapture::from_lowered(
                    target.clone(),
                    nara_render::ViewportRect::new(0, 0, 3, 2).unwrap(),
                    Arc::from([batch]),
                )),
                &reads,
                &[target.clear(wgpu::Color::BLACK, AccessRole::Attachment)],
            )
            .unwrap();
        let program = builder.finish(&[target.output()]).unwrap();
        host.begin_frame(frame).unwrap();
        let failure = AllocationFailureGuard::at("nara_program_texture");
        assert!(matches!(
            host.submit_program(&producer, &program),
            Err(crate::WgpuRenderHostError::Program(
                WgpuRenderProgramError::UndeclaredResource
            )),
        ));
        assert!(!failure.was_consumed());
        assert_eq!(
            host.frame_transaction_stats().native_work(),
            crate::WgpuNativeWorkStats::default()
        );
        assert_eq!(host.retained_resource_bytes(), 0);
    }
}
