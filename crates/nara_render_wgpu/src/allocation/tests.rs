use super::*;
use crate::{
    WgpuRenderBackend, WgpuRenderError,
    frame::WgpuFramePayload,
    quad::{
        WgpuMaterial2d, WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture,
        create_quad_texture_bind_group_layout,
    },
    readback_tests::{readback_pixel, readback_viewport, render_to_rgba, two_image_versions},
    texture::WgpuSpriteTextureCache,
};
use nara_material::{AlphaMode2d, Material2dDefinition, Material2dInput, SamplerDescriptor};
use nara_render::RenderTarget;

std::thread_local! {
    static BATCHES: std::cell::RefCell<Option<Vec<WgpuQuadBatch>>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(super) fn take_batches() -> Option<Vec<WgpuQuadBatch>> {
    BATCHES.with(|batches| batches.borrow_mut().take())
}

struct BatchGuard;

impl BatchGuard {
    fn one(batch: WgpuQuadBatch) -> Self {
        BATCHES.with(|batches| assert!(batches.replace(Some(vec![batch])).is_none()));
        Self
    }

    fn was_consumed(&self) -> bool {
        BATCHES.with(|batches| batches.borrow().is_none())
    }
}

impl Drop for BatchGuard {
    fn drop(&mut self) {
        let _ = BATCHES.with(|batches| batches.borrow_mut().take());
    }
}

#[test]
fn transient_allocations_defer_without_submission_refund_demand_and_recover() {
    let [snapshot, _] = two_image_versions();
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
        }],
        counts_as_sprites: true,
        material_extension: Some(WgpuMaterial2d {
            definition: Material2dDefinition {
                id: "test.checked-allocation",
                fragment_shader: "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return textureLoad(material_texture, vec2<i32>(0, 0), 0) * material_parameters * tint; }",
                input: Material2dInput::SPRITE_FRAGMENT_V1,
                alpha_mode: AlphaMode2d::Opaque,
            },
            parameters: [1.0; 4],
            texture: snapshot,
        }),
    };
    let labels = [
        "nara_wgpu_quad_texture_bind_group_layout",
        "nara_material_2d_bind_group_layout",
        "nara_wgpu_image_texture",
        "nara_wgpu_texture_upload",
        "nara_wgpu_texture_view",
        "nara_wgpu_quad_sampler",
        "nara_wgpu_quad_texture_binding",
        "nara_material_2d_parameters",
        "nara_material_2d_bind_group",
        "nara_wgpu_quad_instances",
    ];
    let (pixels, ()) = render_to_rgba(|device, queue, format, output, encoder| {
        for label in labels {
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            backend.begin_frame_transaction(1, None);
            let failure = AllocationFailureGuard::at(label);
            let batches = BatchGuard::one(batch.clone());
            let error = backend
                .prepare_submitter_draw(
                    format,
                    RenderTarget::PrimaryWindow,
                    readback_viewport(),
                    Some(0),
                    &WgpuFramePayload::default(),
                    1,
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
            assert!(batches.was_consumed());
            let stats = backend.frame_transaction_stats();
            assert_eq!(stats.candidate_instance_buffer_demand_bytes(), 0, "{label}");
            assert_eq!(stats.submitted_instance_buffer_demand_bytes(), 0, "{label}");
            assert_eq!(stats.queue_submissions(), 0, "{label}");
            assert_eq!(stats.surface_acquire_attempts(), 0, "{label}");
            assert_eq!(stats.presents(), 0, "{label}");
            if !matches!(
                label,
                "nara_material_2d_parameters"
                    | "nara_material_2d_bind_group"
                    | "nara_wgpu_quad_instances"
            ) {
                assert_eq!(backend.texture_stats().image_bindings, 0, "{label}");
            }
            if matches!(
                label,
                "nara_wgpu_quad_texture_bind_group_layout"
                    | "nara_material_2d_bind_group_layout"
                    | "nara_wgpu_image_texture"
                    | "nara_wgpu_texture_upload"
                    | "nara_wgpu_texture_view"
            ) {
                assert_eq!(backend.texture_stats().image_textures, 0, "{label}");
            }
            drop(failure);
            drop(batches);

            let batches = BatchGuard::one(batch.clone());
            let draw = backend.prepare_submitter_draw(
                format,
                RenderTarget::PrimaryWindow,
                readback_viewport(),
                Some(0),
                &WgpuFramePayload::default(),
                1,
            )?;
            assert!(batches.was_consumed());
            assert_eq!(draw.buffers.len(), 1);
            assert_eq!(backend.texture_stats().image_bindings, 1);
            assert_eq!(backend.texture_stats().image_textures, 1);
            assert_eq!(backend.texture_stats().uploaded_images, 1);
            assert_eq!(
                backend
                    .frame_transaction_stats()
                    .candidate_instance_buffer_demand_bytes(),
                std::mem::size_of::<WgpuQuadInstance>() as u64,
            );
            let mut prepared = crate::PreparedTargetDraw::default();
            prepared.push(
                readback_viewport(),
                nara_render::ViewLoadOp::Clear(nara_render::Color::BLACK),
                draw,
            );
            crate::encode_target_draw(encoder, output, &prepared);
            drop(prepared);
            assert_eq!(
                backend
                    .frame_transaction_stats()
                    .candidate_instance_buffer_demand_bytes(),
                0
            );
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(readback_pixel(&pixels, 32, 32), &[255, 0, 0, 255]);
}

#[test]
fn invalid_buffer_is_checked_before_mapping_and_allocation_failure_takes_priority() {
    render_to_rgba(|device, _, _, _, _| {
        for allocation_failed in [false, true] {
            let failure =
                allocation_failed.then(|| AllocationFailureGuard::at("test.invalid-buffer"));
            let error = create_buffer_with_data(
                device,
                "test.invalid-buffer",
                &[1; 16],
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::UNIFORM,
            )
            .unwrap_err();
            assert_eq!(error.is_deferred(), allocation_failed);
            let rendered_error = WgpuRenderError::from(error.clone());
            if let Some(failure) = failure {
                assert!(failure.was_consumed());
                assert!(matches!(error, WgpuTextureError::AllocationFailed { .. }));
                assert!(matches!(
                    rendered_error,
                    WgpuRenderError::UploadDeferred { .. }
                ));
            } else {
                assert!(matches!(error, WgpuTextureError::ResourceRejected { .. }));
                assert!(matches!(
                    rendered_error,
                    WgpuRenderError::ContentRejected { .. }
                ));
            }
        }
        create_buffer_with_data(
            device,
            "test.valid-buffer",
            &[1; 16],
            wgpu::BufferUsages::UNIFORM,
        )
        .unwrap();
        Ok(())
    })
    .unwrap();
}

#[test]
fn invalid_texture_binding_is_rejected_without_caching_and_valid_layout_recovers() {
    let [snapshot, _] = two_image_versions();
    render_to_rgba(|device, queue, _, _, _| {
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
            .bind_group(device, queue, &invalid_layout, material, &texture, 1)
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
            .bind_group(device, queue, &layout, material, &texture, 1)
            .unwrap();
        assert_eq!(cache.stats().image_bindings, 1);
        assert_eq!(cache.stats().uploaded_images, 1);
        Ok(())
    })
    .unwrap();
}
