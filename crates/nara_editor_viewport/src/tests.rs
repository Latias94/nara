use std::time::Duration;

use nara_core::Vec2;
use nara_editor_core::viewport::{PanelSize, WorldPoint};
use nara_editor_gizmo::OverlayGrid;
use nara_scene::authoring::{SceneEntity, SpriteSource};

use super::*;

#[test]
#[ignore = "requires an available native WGPU adapter"]
fn imported_output_reuses_targets_and_buffers_after_warmup_and_resize() {
    let shared = pollster::block_on(async {
        let mut instance = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        instance.backends &= wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(instance);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .unwrap();
        SharedWgpuContext::request_device(instance, adapter, wgpu::DeviceDescriptor::default())
            .await
            .unwrap()
    });
    let atlas = Arc::new(WgpuAtlas::from_context(&shared));
    let foreign_atlas = Arc::new(WgpuAtlas::from_context(&shared));
    let mut viewport = EditorViewport::new(&shared).unwrap();
    let mut document = SceneDocument::new("bfe78212-36e8-44b8-8e98-9b98b2fc71fb".parse().unwrap());
    let mut entity = SceneEntity::new(
        "bfe78212-36e8-44b8-8e98-9b98b2fc71fc".parse().unwrap(),
        "Sprite",
    );
    entity
        .set(&SpriteSource {
            size: Vec2::splat(24.0),
            ..SpriteSource::default()
        })
        .unwrap();
    document.entities.push(entity);
    viewport.set_scene(&document, None).unwrap();
    viewport.overlay.update(OverlayFrame {
        grid: Some(OverlayGrid::default()),
        ..OverlayFrame::default()
    });

    let device = shared.device();
    let mut last_frame = None;
    for (width, height) in [(64, 64), (93, 68)] {
        let extent = Extent2d::new(width, height).unwrap();
        let camera = ViewportCamera2D::new(
            PanelSize::new(width as f32, height as f32).unwrap(),
            WorldPoint(Vec2::ZERO),
            1.0,
        )
        .unwrap();
        let input = viewport.scene.render_input(camera, extent).unwrap();
        let (output, report) = viewport.renderer.render_to_image(&input).unwrap();
        let native_texture = output.native_texture().unwrap().clone();
        if let Some((old_image, _)) = last_frame.take() {
            let wrong_extent = import_descriptor(output.clone()).unwrap();
            assert!(atlas.update_image(&old_image, wrong_extent).is_err());
        }
        let descriptor = import_descriptor(output).unwrap();
        assert_eq!(descriptor.texture, native_texture);
        assert_eq!(descriptor.transfer, ImageTextureTransfer::Srgb);
        assert_eq!(descriptor.alpha, ImageTextureAlpha::Premultiplied);
        let owner = Arc::downgrade(descriptor.owner.as_ref().unwrap());
        let image = atlas.import_image(descriptor).unwrap();
        assert!(owner.upgrade().is_some());
        assert_eq!(
            image.size(),
            size(DevicePixels(width as i32), DevicePixels(height as i32))
        );
        wait_for(device, report.submission);

        for frame in 0..4 {
            let (output, report) = viewport.renderer.render_to_image(&input).unwrap();
            if frame > 0 {
                assert_eq!(report.work.created_texture_calls, 0);
                assert_eq!(report.work.created_buffer_calls, 0);
                assert_eq!(report.work.issued_texture_write_calls, 0);
            }
            let descriptor = import_descriptor(output).unwrap();
            if frame == 0 {
                assert!(
                    foreign_atlas
                        .update_image(&image, descriptor.clone())
                        .is_err()
                );
            }
            atlas.update_image(&image, descriptor).unwrap();
            wait_for(device, report.submission);
        }
        assert!(owner.upgrade().is_none());
        last_frame = Some((image, input));
    }

    let (image, input) = last_frame.unwrap();
    let mut held = Vec::new();
    let busy = (0..64).any(|_| match viewport.renderer.render_to_image(&input) {
        Ok((output, _)) => {
            held.push(output);
            false
        }
        Err(WgpuRenderError::TargetsBusy) => true,
        Err(error) => panic!("unexpected render failure: {error}"),
    });
    assert!(busy);
    assert!(image.validate().is_ok());
    drop(held);
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
    let (output, report) = viewport.renderer.render_to_image(&input).unwrap();
    atlas
        .update_image(&image, import_descriptor(output).unwrap())
        .unwrap();
    wait_for(device, report.submission);
}

fn wait_for(device: &wgpu::Device, submission: wgpu::SubmissionIndex) {
    device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(5)),
        })
        .unwrap();
}
