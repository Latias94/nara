use super::*;
use crate::gpu::{
    WgpuRenderError,
    quad::{
        WgpuMaterial2d, WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture,
        create_quad_texture_bind_group_layout, draw_quad_batch_buffers,
    },
    readback_tests::{
        TEXTURE_MATERIAL_FRAGMENT, image_versions_with_handle, material_binding,
        material_generation, readback_pixel, readback_viewport, render_to_rgba, two_image_versions,
    },
    stock_quad::StockQuadResources,
    texture::WgpuSpriteTextureCache,
};
use crate::material::{AlphaMode2d, SamplerDescriptor};

#[test]
fn transient_allocations_defer_without_submission_refund_demand_and_recover() {
    let (handle, [snapshot, _]) = image_versions_with_handle(1, 1);
    let batch = WgpuQuadBatch {
        material: WgpuQuadMaterialKey {
            texture: WgpuQuadTexture::Image(snapshot.clone()).key(),
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Opaque,
        },
        texture: WgpuQuadTexture::Image(snapshot.clone()),
        scissor: None,
        instances: vec![WgpuQuadInstance {
            center: [0.0; 2],
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
            color: [1.0; 4],
            uv_min: [0.0; 2],
            uv_size: [1.0; 2],
            texture_mode: 0,
            shape_mode: 0,
            rect_size: [1.0; 2],
            corner_radius: 0.0,
            border_width: 0.0,
        }]
        .into(),
        counts_as_sprites: true,
        material_extension: Some(WgpuMaterial2d {
            view: crate::gpu::view_uniform::WgpuViewUniform::manual(readback_viewport()),
            binding: material_binding(
                &material_generation(
                    "test.checked-allocation",
                    AlphaMode2d::Opaque,
                    TEXTURE_MATERIAL_FRAGMENT,
                ),
                [1.0; 4],
                handle,
            ),
            textures: vec![(1, snapshot.into())],
        }),
    };
    let labels = [
        "nara_wgpu_quad_texture_bind_group_layout",
        "nara_material_2d_bind_group_layout",
        "nara_view_bind_group_layout",
        "nara_wgpu_image_texture",
        "nara_wgpu_texture_upload",
        "nara_wgpu_texture_view",
        "nara_wgpu_quad_sampler",
        "nara_wgpu_quad_texture_binding",
        "nara_material_2d_parameters",
        "nara_material_2d_bind_group",
        "nara_view_uniform",
        "nara_view_bind_group",
        "nara_wgpu_quad_instances",
    ];
    let (pixels, ()) = render_to_rgba(|_, _, format, output, encoder, preparation| {
        for label in labels {
            let mut resources = StockQuadResources::default();
            resources.begin_frame(1);
            let failure = AllocationFailureGuard::at(label);
            let error = resources
                .prepare_batches(
                    preparation,
                    || Ok(()),
                    format,
                    std::slice::from_ref(&batch),
                    1,
                    None,
                )
                .unwrap_err();
            assert!(
                matches!(error, WgpuRenderError::UploadDeferred { .. }),
                "{label}: {error}"
            );
            assert!(
                failure.was_consumed(),
                "{label} must reach its allocation check"
            );
            assert_eq!(resources.instance_charges(), (0, 0), "{label}");
            let binding_was_cached = matches!(
                label,
                "nara_material_2d_parameters"
                    | "nara_material_2d_bind_group"
                    | "nara_view_uniform"
                    | "nara_view_bind_group"
                    | "nara_wgpu_quad_instances"
            );
            assert_eq!(
                resources.texture_stats().image_bindings,
                usize::from(binding_was_cached),
                "{label}"
            );
            if matches!(
                label,
                "nara_wgpu_quad_texture_bind_group_layout"
                    | "nara_material_2d_bind_group_layout"
                    | "nara_view_bind_group_layout"
                    | "nara_wgpu_image_texture"
                    | "nara_wgpu_texture_upload"
                    | "nara_wgpu_texture_view"
            ) {
                assert_eq!(resources.texture_stats().image_textures, 0, "{label}");
            }
            drop(failure);

            let draw = resources.prepare_batches(
                preparation,
                || Ok(()),
                format,
                std::slice::from_ref(&batch),
                1,
                None,
            )?;
            assert_eq!(draw.buffers.len(), 1);
            assert_eq!(resources.texture_stats().image_bindings, 1);
            assert_eq!(resources.texture_stats().image_textures, 1);
            assert_eq!(resources.texture_stats().uploaded_images, 1);
            assert_eq!(
                resources.instance_charges(),
                (std::mem::size_of::<WgpuQuadInstance>() as u64, 0),
            );
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("nara_stock_allocation_recovery"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: output,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                draw_quad_batch_buffers(
                    &mut pass,
                    &draw.pipelines,
                    &draw.buffers,
                    readback_viewport(),
                );
            }
            drop(draw);
            assert_eq!(resources.instance_charges(), (0, 0));
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(readback_pixel(&pixels, 32, 32), &[255, 0, 0, 255]);
}

#[test]
fn invalid_buffer_is_checked_before_mapping_and_allocation_failure_takes_priority() {
    render_to_rgba(|_, _, _, _, _, preparation| {
        let baseline = preparation.work_stats();
        for (attempt, allocation_failed) in [false, true].into_iter().enumerate() {
            let failure =
                allocation_failed.then(|| AllocationFailureGuard::at("test.invalid-buffer"));
            let error = preparation
                .create_buffer_with_data(
                    "test.invalid-buffer",
                    &[1; 16],
                    wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::UNIFORM,
                )
                .unwrap_err();
            let error = WgpuTextureError::from(error);
            let mut expected = baseline;
            for _ in 0..=attempt {
                expected.record_buffer_creation(16);
            }
            assert_eq!(preparation.work_stats(), expected);
            assert_eq!(error.is_deferred(), allocation_failed);
            let rendered_error = WgpuRenderError::from(error.clone());
            if let Some(failure) = failure {
                assert!(failure.was_consumed());
                assert!(matches!(
                    error,
                    WgpuTextureError::NativeBackend(WgpuRenderError::UploadDeferred { .. })
                ));
                assert!(matches!(
                    rendered_error,
                    WgpuRenderError::UploadDeferred { .. }
                ));
            } else {
                assert!(matches!(
                    error,
                    WgpuTextureError::NativeBackend(WgpuRenderError::ContentRejected { .. })
                ));
                assert!(matches!(
                    rendered_error,
                    WgpuRenderError::ContentRejected { .. }
                ));
            }
        }
        preparation
            .create_buffer_with_data("test.valid-buffer", &[1; 16], wgpu::BufferUsages::UNIFORM)
            .unwrap();
        let initialized = preparation.work_stats();
        assert_eq!(
            initialized.created_buffer_calls - baseline.created_buffer_calls,
            3
        );
        assert_eq!(
            initialized.created_buffer_bytes - baseline.created_buffer_bytes,
            48
        );
        assert_eq!(
            initialized.initialized_buffer_calls - baseline.initialized_buffer_calls,
            1
        );
        assert_eq!(
            initialized.initialized_buffer_payload_bytes
                - baseline.initialized_buffer_payload_bytes,
            16
        );
        assert_eq!(
            initialized.initialized_buffer_storage_bytes
                - baseline.initialized_buffer_storage_bytes,
            16
        );
        preparation
            .create_buffer_with_data("test.empty-buffer", &[], wgpu::BufferUsages::UNIFORM)
            .unwrap();
        let mut expected = initialized;
        expected.record_buffer_creation(0);
        assert_eq!(preparation.work_stats(), expected);
        Ok(())
    })
    .unwrap();
}

#[test]
fn invalid_texture_binding_is_rejected_without_caching_and_valid_layout_recovers() {
    let [snapshot, _] = two_image_versions();
    render_to_rgba(|device, _, _, _, _, preparation| {
        let mut cache = WgpuSpriteTextureCache::default();
        let texture = WgpuQuadTexture::Image(snapshot);
        let material = WgpuQuadMaterialKey {
            texture: texture.key(),
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Opaque,
        };
        let invalid_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("test.empty-texture-layout"),
            entries: &[],
        });
        let error = cache
            .bind_group(preparation, &invalid_layout, material, &texture, 1)
            .unwrap_err();
        assert!(matches!(error, WgpuTextureError::ResourceRejected { .. }));
        assert!(!error.is_deferred());
        assert!(matches!(
            WgpuRenderError::from(error),
            WgpuRenderError::ContentRejected { .. }
        ));
        assert_eq!(cache.stats().image_bindings, 0);
        assert_eq!(cache.stats().image_textures, 1);
        assert_eq!(cache.stats().uploaded_images, 1);
        let layout = create_quad_texture_bind_group_layout(device)?;
        cache
            .bind_group(preparation, &layout, material, &texture, 1)
            .unwrap();
        assert_eq!(cache.stats().image_bindings, 1);
        assert_eq!(cache.stats().uploaded_images, 1);
        Ok(())
    })
    .unwrap();
}
