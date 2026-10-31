use super::*;

#[test]
fn physical_extents_require_exact_nonempty_equality() {
    let source = Extent2d::new(1920, 1080).unwrap();
    assert!(validate_extents(source, source).is_ok());
    for destination in [
        Extent2d::new(960, 540).unwrap(),
        Extent2d::new(1920, 1081).unwrap(),
        Extent2d::new(1921, 1080).unwrap(),
        Extent2d::new(1080, 1920).unwrap(),
    ] {
        assert!(matches!(
            validate_extents(source, destination),
            Err(WgpuRenderError::ContentRejected { .. })
        ));
    }
    for extent in [
        Extent2d {
            width: 0,
            height: 1,
        },
        Extent2d {
            width: 1,
            height: 0,
        },
        Extent2d {
            width: 0,
            height: 0,
        },
    ] {
        assert!(matches!(
            validate_extents(extent, extent),
            Err(WgpuRenderError::ContentRejected { .. })
        ));
    }
}

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod native {
    use super::*;
    use crate::{
        allocation::AllocationFailureGuard,
        readback_tests::{READBACK_EXTENT, ReadbackGpu},
    };

    fn source_texture(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("surface-presentation-test-source"),
            size: wgpu::Extent3d {
                width: READBACK_EXTENT,
                height: READBACK_EXTENT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    #[test]
    fn exact_source_pixels_survive_successor_preparation_and_cache_reset() {
        let gpu = ReadbackGpu::new();
        let extent = Extent2d::new(READBACK_EXTENT, READBACK_EXTENT).unwrap();
        let mut cache = SurfacePresentationCache::default();
        let expected: Vec<u8> = (0..READBACK_EXTENT)
            .flat_map(|y| {
                (0..READBACK_EXTENT).flat_map(move |x| {
                    [
                        ((x * 17 + y * 3) % 256) as u8,
                        ((y * 19 + x * 5) % 256) as u8,
                        ((x * 7 + y * 11) % 256) as u8,
                        ((x * 13 + y * 23) % 256) as u8,
                    ]
                })
            })
            .collect();
        let (pixels, _custody) = gpu
            .render_to_rgba(|device, queue, format, target, encoder, _| {
                let source = source_texture(device, format);
                queue.write_texture(
                    source.as_image_copy(),
                    &expected,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(READBACK_EXTENT * 4),
                        rows_per_image: Some(READBACK_EXTENT),
                    },
                    source.size(),
                );
                let view = source.create_view(&wgpu::TextureViewDescriptor::default());
                let prepared = cache.prepare(device, 1, format, &view, extent, extent)?;
                drop(view);
                let successor = source_texture(device, format);
                let successor_view = successor.create_view(&wgpu::TextureViewDescriptor::default());
                let next = cache.prepare(device, 1, format, &successor_view, extent, extent)?;
                assert!(Arc::ptr_eq(&prepared.pipeline, &next.pipeline));
                cache.clear();
                prepared.encode(encoder, target);
                // Return the exact storage and prepared draw through the harness's original
                // submission prefix, rather than relying on the source view alone.
                Ok((source, prepared, successor, next))
            })
            .unwrap();
        assert_eq!(
            pixels, expected,
            "all physical pixels and alpha must round-trip"
        );
    }

    #[test]
    fn rekeys_epoch_and_format_and_rejects_mismatch_before_pipeline_allocation() {
        let gpu = ReadbackGpu::new();
        let device = &gpu.device;
        let extent = Extent2d::new(READBACK_EXTENT, READBACK_EXTENT).unwrap();
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let source = source_texture(device, format);
        let view = source.create_view(&wgpu::TextureViewDescriptor::default());
        let mut cache = SurfacePresentationCache::default();
        let failure = AllocationFailureGuard::at("nara_surface_presentation_layout");
        assert!(matches!(
            cache.prepare(
                device,
                1,
                format,
                &view,
                extent,
                Extent2d::new(32, 64).unwrap(),
            ),
            Err(WgpuRenderError::ContentRejected { .. })
        ));
        assert!(cache.pipeline.is_none());
        assert!(!failure.was_consumed());
        assert!(matches!(
            cache.prepare(device, 1, format, &view, extent, extent),
            Err(WgpuRenderError::UploadDeferred { .. })
        ));
        assert!(failure.was_consumed());
        drop(failure);
        let first = cache
            .prepare(device, 1, format, &view, extent, extent)
            .unwrap();
        let next_epoch = cache
            .prepare(device, 2, format, &view, extent, extent)
            .unwrap();
        assert!(!Arc::ptr_eq(&first.pipeline, &next_epoch.pipeline));
        let bgra_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let bgra = source_texture(device, bgra_format);
        let bgra_view = bgra.create_view(&wgpu::TextureViewDescriptor::default());
        let next_format = cache
            .prepare(device, 2, bgra_format, &bgra_view, extent, extent)
            .unwrap();
        assert!(!Arc::ptr_eq(&next_epoch.pipeline, &next_format.pipeline));
        let failure = AllocationFailureGuard::at("nara_surface_presentation_binding");
        assert!(matches!(
            cache.prepare(device, 3, bgra_format, &bgra_view, extent, extent),
            Err(WgpuRenderError::UploadDeferred { .. })
        ));
        drop(failure);
        assert!(Arc::ptr_eq(
            cache.pipeline.as_ref().unwrap(),
            &next_format.pipeline
        ));
        cache.clear();
        assert!(cache.pipeline.is_none());
    }
}
