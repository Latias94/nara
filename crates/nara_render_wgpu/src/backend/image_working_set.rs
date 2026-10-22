use super::*;
use crate::readback_tests::render_to_rgba;
use nara_app::{App, RuntimeInstance};
use nara_asset::{AssetEvents, AssetStates, Assets};
use nara_core::Vec2;
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_image::{ImageAsset, PreparedImageResource, PreparedImageSnapshot};
use nara_render::{Camera2d, ExtractedViews, PreparedRenderResources, RenderFrame, RenderImage2d};
use nara_sprite::Sprite;
use nara_transform::Transform2d;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiImageId, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};

fn captured_images(editor: bool, shared: bool) -> WgpuCapturedFrame {
    let mut app = App::new();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    let target = if editor {
        RenderTarget::Image(RenderImage2d::allocate_handle().unwrap())
    } else {
        RenderTarget::Window(WindowId::PRIMARY)
    };
    let mut assets = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let handles = crate::readback_tests::image_assets(1, 1).map(|image| {
        let handle = nara_asset::allocate_anonymous_handle().unwrap();
        let source = image.source().source_hash();
        let artifact = image.source().artifact().key().digest();
        assets
            .commit_loaded(
                handle,
                image,
                &mut states,
                &mut events,
                Some(source),
                Some(artifact),
            )
            .unwrap();
        handle
    });
    let world = app.world_mut().unwrap();
    world.insert_resource(assets);
    world.insert_resource(states);
    let mut entities = Vec::new();
    for (index, x) in [0.0, 100.0].into_iter().enumerate() {
        entities.push(
            world
                .spawn((
                    Camera2d {
                        target,
                        viewport: Some(ViewportRect::new(index as u32 * 32, 0, 32, 64).unwrap()),
                        viewport_height: 64.0,
                        order: index as i32,
                        ..Default::default()
                    },
                    Transform2d::from_translation(Vec2::new(x, 0.0)),
                ))
                .id(),
        );
        entities.push(
            world
                .spawn((
                    Sprite::from_texture(handles[if shared { 0 } else { index }], Vec2::splat(8.0)),
                    Transform2d::from_translation(Vec2::new(x, 0.0)),
                ))
                .id(),
        );
    }
    HierarchyWriter::new(world)
        .apply_batch(
            &entities
                .into_iter()
                .map(|entity| HierarchyOperation::Place {
                    entity,
                    placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let driven = runtime.drive(Duration::ZERO);
    let captured = driven.as_ref().ok().map(|_| {
        let world = runtime.world();
        let views = world.resource::<ExtractedViews>();
        let frame = world.resource::<RenderFrame>().index;
        let topology = if let RenderTarget::Image(image) = target {
            nara_render::__private::build_editor_render_frame_packet(
                nara_render::__private::EditorRenderGeneration::try_acquire().unwrap(),
                frame,
                views,
                nara_render::__private::editor_viewport_image_target(
                    image,
                    Extent2d::new(64, 64).unwrap(),
                )
                .unwrap(),
            )
            .unwrap()
            .unwrap()
        } else {
            let window = nara_window::Window::new(
                "image working set",
                nara_window::WindowResolution::new(64, 64),
            );
            nara_render::build_render_frame_packet(
                runtime.generation(),
                frame,
                Some(WindowId::PRIMARY),
                [&window],
                views,
            )
            .unwrap()
            .unwrap()
        };
        let image = if shared {
            let record = world
                .resource::<PreparedRenderResources<PreparedImageResource>>()
                .get(nara_image::image_resource_key(handles[0]))
                .unwrap();
            PreparedImageSnapshot::new(
                record.snapshot().clone(),
                record.resource().unwrap().clone(),
            )
        } else {
            crate::readback_tests::two_image_versions()[0].clone()
        };
        let image_id = UiImageId::new("project://working-set/ui.png").unwrap();
        let authoring = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("working-set", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(2.0, 2.0, 8.0, 8.0))
                        .with_key(UiLocalKey::runtime("working-set", "image").unwrap())
                        .with_visual(UiVisual::image(
                            image_id.clone(),
                            UiColor::rgba(1.0, 1.0, 1.0, 1.0),
                        )),
                ),
            )
            .build()
            .unwrap();
        let environment =
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.0, 1, 1, 1, 1).unwrap();
        let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&authoring, environment, UiTheme::default())
            .unwrap();
        let mut images = nara_ui_render::UiPreparedImages::default();
        images.insert(image_id, image);
        let ui = nara_ui_render::UiPresentationPlanner::default()
            .plan(frame, target, &images)
            .unwrap();
        WgpuCapturedFrame {
            topology,
            payload: WgpuFramePayload {
                primary_window_id: Some(WindowId::PRIMARY),
                sprite_batches: Some(
                    world
                        .resource::<nara_sprite_render::__private::SpriteBatches>()
                        .clone(),
                ),
                ui_packets: vec![ui],
                ..Default::default()
            },
            app_frame_index: 1,
            work_started_at: std::time::Instant::now(),
        }
    });
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != nara_app::RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
    driven.unwrap();
    captured.unwrap()
}

#[test]
fn complete_target_image_working_set_rejects_before_upload_across_views_and_ui() {
    for editor in [false, true] {
        let captured = captured_images(editor, false);
        assert_eq!(captured.topology.views().len(), 2);
        for (versions, bytes) in [(2, 12), (3, 8)] {
            render_to_rgba(|device, queue, format, _, _| {
                let mut backend =
                    WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
                backend.quad_textures =
                    WgpuSpriteTextureCache::with_image_residency_limits(versions, bytes);
                backend.begin_frame_transaction(1, None);
                let error = backend
                    .prepare_target_draw(format, &captured.topology, &captured.payload, 1)
                    .unwrap_err();
                assert!(
                    matches!(error, WgpuRenderError::ContentRejected { .. }),
                    "{error}"
                );
                assert_eq!(backend.texture_stats().uploaded_images, 0);
                assert_eq!(backend.texture_stats().image_textures, 0);
                assert_eq!(
                    backend
                        .frame_transaction_stats()
                        .candidate_instance_buffer_demand_bytes(),
                    0
                );
                Ok(())
            })
            .unwrap();
        }
    }
}

#[test]
fn complete_target_image_working_set_rejects_with_partial_cache() {
    for editor in [false, true] {
        let captured = captured_images(editor, false);
        for (versions, bytes) in [(2, 12), (3, 8)] {
            render_to_rgba(|device, queue, format, _, _| {
                let mut backend =
                    WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
                backend.quad_textures =
                    WgpuSpriteTextureCache::with_image_residency_limits(versions, bytes);
                backend.begin_frame_transaction(1, None);
                let first_view = &captured.topology.views()[0];
                let first = backend.prepare_submitter_draw(
                    format,
                    first_view.target,
                    first_view.viewport,
                    Some(0),
                    &captured.payload,
                    1,
                )?;
                assert_eq!(backend.texture_stats().uploaded_images, 1);
                drop(first);

                let error = backend
                    .prepare_target_draw(format, &captured.topology, &captured.payload, 1)
                    .unwrap_err();
                assert!(
                    matches!(error, WgpuRenderError::ContentRejected { .. }),
                    "{error}"
                );
                assert_eq!(backend.texture_stats().uploaded_images, 1);
                assert_eq!(backend.texture_stats().image_textures, 1);
                assert_eq!(
                    backend
                        .frame_transaction_stats()
                        .candidate_instance_buffer_demand_bytes(),
                    0
                );
                Ok(())
            })
            .unwrap();
        }
    }
}

#[test]
fn complete_target_image_working_set_defers_cleared_live_namespace_and_recovers() {
    for editor in [false, true] {
        let captured = captured_images(editor, true);
        render_to_rgba(|device, queue, format, output, encoder| {
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            backend.quad_textures = WgpuSpriteTextureCache::with_image_residency_limits(1, 4);
            backend.begin_frame_transaction(1, None);
            let old =
                backend.prepare_target_draw(format, &captured.topology, &captured.payload, 1)?;
            backend.quad_textures.clear();
            let error = backend
                .prepare_target_draw(format, &captured.topology, &captured.payload, 2)
                .unwrap_err();
            assert!(
                matches!(error, WgpuRenderError::UploadDeferred { .. }),
                "{error}"
            );
            assert_eq!(backend.texture_stats().uploaded_images, 0);
            assert_eq!(backend.texture_stats().image_textures, 1);
            drop(old);

            let recovered =
                backend.prepare_target_draw(format, &captured.topology, &captured.payload, 3)?;
            assert_eq!(backend.texture_stats().uploaded_images, 1);
            assert_eq!(backend.texture_stats().image_textures, 1);
            crate::encode_target_draw(encoder, output, &recovered);
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn complete_target_image_working_set_deduplicates_shared_exact_snapshot() {
    for editor in [false, true] {
        let captured = captured_images(editor, true);
        render_to_rgba(|device, queue, format, output, encoder| {
            let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
            backend.quad_textures = WgpuSpriteTextureCache::with_image_residency_limits(1, 4);
            backend.begin_frame_transaction(1, None);
            let prepared =
                backend.prepare_target_draw(format, &captured.topology, &captured.payload, 1)?;
            assert_eq!(backend.texture_stats().uploaded_images, 1);
            assert_eq!(backend.texture_stats().image_textures, 1);
            crate::encode_target_draw(encoder, output, &prepared);
            Ok(())
        })
        .unwrap();
    }
}
