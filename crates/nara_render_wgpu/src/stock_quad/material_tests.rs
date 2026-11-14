use super::*;
use crate::{
    allocation::AllocationFailureGuard,
    quad::{WgpuMaterial2dPipelineKey, WgpuQuadInstance, validate_material_device_limits},
    readback_tests::{
        MIXED_MATERIAL_FRAGMENT, MixedMaterialParameters, ReadbackGpu, TEXTURE_MATERIAL_FRAGMENT,
        TestMaterialParameters, image_versions_with_handle, material_batch, material_generation,
        mixed_material_binding, mixed_material_library, publish_material,
    },
};
use nara_material::{AlphaMode2d, MaterialParameters};

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
    let limits = wgpu::Limits {
        max_uniform_buffer_binding_size: size,
        max_buffer_size: size,
        max_sampled_textures_per_shader_stage: 3,
        max_bindings_per_bind_group: 8,
        max_bind_groups: 2,
        max_bind_groups_plus_vertex_buffers: 3,
        max_uniform_buffers_per_shader_stage: 1,
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
                max_bind_groups: 1,
                ..limits.clone()
            },
        ),
        (
            "max_bind_groups_plus_vertex_buffers",
            wgpu::Limits {
                max_bind_groups_plus_vertex_buffers: 2,
                ..limits.clone()
            },
        ),
        (
            "max_uniform_buffers_per_shader_stage",
            wgpu::Limits {
                max_uniform_buffers_per_shader_stage: 0,
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
        7
    );
    assert_eq!(
        resources
            .pending_allocation_bytes(std::iter::once(&batch))
            .unwrap(),
        size_of::<WgpuQuadInstance>() as u64 + 16 + 4
    );
}

#[test]
fn variable_material_cost_counts_each_texture_and_uniform_before_native_work() {
    let batch = mixed_batch();
    let material = batch.material_extension.as_ref().unwrap();
    let cost = StockPreparationCost::for_batches(std::iter::once(&batch)).unwrap();
    assert_eq!(cost.custody_entries, 3 * 3 + 2 * 2);
    assert!(cost.modeled_metadata_bytes >= material.binding.uniform_bytes().len());
    let resources = StockQuadResources::default();
    let expected = size_of::<WgpuQuadInstance>() as u64
        + material.binding.uniform_bytes().len() as u64
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
    let first = &shared[0].material_extension.as_ref().unwrap().textures[0].1;
    let last = &shared[1].material_extension.as_ref().unwrap().textures[1].1;
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
    assert_eq!(cost.custody_entries, 4 * (3 * 3 + 2 * 2));
    let expected = 4 * (size_of::<WgpuQuadInstance>() as u64 + uniform_bytes) + 4 + 4;
    assert_eq!(preflight.allocation_bytes(&resources).unwrap(), expected);
    let mut different_version = shared.to_vec();
    different_version[1]
        .material_extension
        .as_mut()
        .unwrap()
        .textures[1]
        .1 = green;
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
                std::slice::from_ref(&batch), 1,
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
        )?;
        assert_eq!(draw.buffers.len(), 1);
        assert_eq!(preparation.work_stats().issued_texture_write_calls, 3);
        assert_eq!(
            preparation.work_stats().initialized_buffer_payload_bytes,
            size_of::<WgpuQuadInstance>() as u64
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
