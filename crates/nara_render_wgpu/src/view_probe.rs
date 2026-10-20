#[cfg(feature = "ui-submitter")]
use std::sync::Arc;
use std::time::{Duration, Instant};

use nara_app::{App, RuntimeInstance};
use nara_core::{Color, Vec2};
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_render::{
    __private::{
        EditorRenderGeneration, build_editor_render_frame_packet, editor_viewport_image_target,
    },
    Camera2d, Extent2d, ExtractedViews, RenderFrame, RenderImage2d, RenderTarget, ViewLoadOp,
    ViewportRect,
};
use nara_sprite::Sprite;
use nara_sprite_render::__private::SpriteBatches;
use nara_transform::Transform2d;

use crate::{
    WgpuRenderBackend,
    frame::WgpuFramePacketSlot,
    readback_tests::{readback_pixel, render_to_rgba},
};

fn captured_split_frame() -> crate::frame::WgpuCapturedFrame {
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
    let image = RenderImage2d::allocate_handle().unwrap();
    let target = RenderTarget::Image(image);
    let descriptor = editor_viewport_image_target(image, Extent2d::new(64, 64).unwrap()).unwrap();
    let generation = EditorRenderGeneration::try_acquire().unwrap();
    #[cfg(feature = "ui-submitter")]
    let overlay = overlay_packet(target);
    let world = app.world_mut().unwrap();
    let mut entities = Vec::new();
    entities.push(
        world
            .spawn((
                Camera2d {
                    target,
                    viewport: Some(ViewportRect::new(32, 0, 32, 64).unwrap()),
                    viewport_height: 64.0,
                    order: 1,
                    ..Default::default()
                },
                Transform2d {
                    translation: Vec2::new(100.0, 0.0),
                    rotation: std::f32::consts::FRAC_PI_2,
                    ..Default::default()
                },
            ))
            .id(),
    );
    entities.push(
        world
            .spawn((
                Camera2d {
                    target,
                    viewport: Some(ViewportRect::new(0, 0, 32, 64).unwrap()),
                    viewport_height: 64.0,
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id(),
    );
    for (x, color) in [
        (0.0, Color::rgb(1.0, 0.0, 0.0)),
        (100.0, Color::rgb(0.0, 1.0, 0.0)),
    ] {
        entities.push(
            world
                .spawn((
                    Sprite::from_color(Vec2::new(24.0, 8.0), color),
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
    let snapshot = if driven.is_ok() {
        let views = runtime.world().resource::<ExtractedViews>();
        let loads = views
            .as_slice()
            .iter()
            .map(|view| view.load)
            .collect::<Vec<_>>();
        let packet = build_editor_render_frame_packet(
            generation,
            runtime.world().resource::<RenderFrame>().index,
            views,
            descriptor,
        );
        let mut slot = WgpuFramePacketSlot::default();
        slot.collect_sprite(runtime.world().resource::<SpriteBatches>().clone());
        #[cfg(feature = "ui-submitter")]
        slot.collect_ui(vec![overlay], Vec::new());
        let topology_matches = slot.payload_matches_topology(
            views,
            #[cfg(feature = "ui-submitter")]
            None,
        );
        slot.seal(
            #[cfg(feature = "ui-submitter")]
            None,
            packet,
            1,
            Instant::now(),
        );
        Some((loads, topology_matches, slot.take_capture()))
    } else {
        None
    };
    let mut retirement = runtime.begin_retirement();
    while retirement.retirement_state() != nara_app::RuntimeCandidateRetirementState::Retired {
        retirement.drive_retirement();
    }
    driven.unwrap();
    let (loads, topology_matches, captured) = snapshot.unwrap();
    assert_eq!(loads, [ViewLoadOp::Clear(Color::BLACK), ViewLoadOp::Load]);
    assert!(topology_matches);
    captured.unwrap().unwrap().unwrap()
}

#[test]
fn native_packet_executes_split_affine_views_with_one_ui_overlay() {
    let captured = captured_split_frame();
    let (pixels, view_count) = render_to_rgba(|device, queue, format, output, encoder| {
        let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
        backend.admit_packet(
            captured.topology.source_generation(),
            captured.topology.frame_index(),
        )?;
        let prepared = backend.prepare_target_draw(
            format,
            &captured.topology,
            &captured.payload,
            captured.topology.frame_index(),
        )?;
        #[cfg(feature = "ui-submitter")]
        assert_eq!(prepared.ui_packets.len(), 1);
        crate::encode_target_draw(encoder, output, &prepared);
        Ok(prepared.views.len())
    })
    .unwrap();
    assert_eq!(view_count, 2 + usize::from(cfg!(feature = "ui-submitter")));
    assert_pixel(&pixels, 16, 32, [255, 0, 0, 255]);
    assert_pixel(&pixels, 48, 32, [0, 255, 0, 255]);
    assert_pixel(&pixels, 24, 32, [255, 0, 0, 255]);
    assert_pixel(&pixels, 16, 42, [0, 0, 0, 255]);
    assert_pixel(&pixels, 48, 42, [0, 255, 0, 255]);
    assert_pixel(&pixels, 58, 32, [0, 0, 0, 255]);
    assert_pixel(&pixels, 2, 2, [0, 0, 0, 255]);
    #[cfg(feature = "ui-submitter")]
    for x in [29, 34] {
        assert_pixel(&pixels, x, 8, [188, 188, 188, 255]);
    }
}

#[cfg(feature = "ui-submitter")]
#[test]
fn mixed_sprite_ui_scope_rejection_refunds_prepared_views_and_recovers() {
    let captured = captured_split_frame();
    let instance_bytes = std::mem::size_of::<crate::quad::WgpuQuadInstance>() as u64;
    let sprite_bytes = captured
        .payload
        .sprite_batches
        .as_ref()
        .unwrap()
        .total_instances() as u64
        * instance_bytes;
    let total_bytes = sprite_bytes + instance_bytes;
    let (pixels, ()) = render_to_rgba(|device, queue, format, output, encoder| {
        let mut backend = WgpuRenderBackend::with_probe_device(device.clone(), queue.clone());
        backend.begin_frame_transaction(1, None);
        backend.set_instance_budget_limit(sprite_bytes);
        let error = backend
            .prepare_target_draw(format, &captured.topology, &captured.payload, 1)
            .expect_err("the UI overlay must share the Sprite views' scope budget");
        assert!(matches!(
            error,
            crate::WgpuRenderError::ContentRejected { .. }
        ));
        assert_eq!(
            backend
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            0
        );
        assert_eq!(
            backend
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            0
        );

        let sprite_payload = crate::frame::WgpuFramePayload {
            sprite_batches: captured.payload.sprite_batches.clone(),
            ..Default::default()
        };
        let sprites =
            backend.prepare_target_draw(format, &captured.topology, &sprite_payload, 1)?;
        assert_eq!(
            backend
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            sprite_bytes
        );
        drop(sprites);
        assert_eq!(
            backend
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            0
        );

        backend.set_instance_budget_limit(total_bytes);
        let recovered =
            backend.prepare_target_draw(format, &captured.topology, &captured.payload, 1)?;
        assert_eq!(recovered.instance_buffer_bytes(), total_bytes);
        assert_eq!(
            backend
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            total_bytes
        );
        crate::encode_target_draw(encoder, output, &recovered);
        Ok(())
    })
    .unwrap();
    assert_pixel(&pixels, 16, 32, [255, 0, 0, 255]);
    assert_pixel(&pixels, 48, 32, [0, 255, 0, 255]);
    assert_pixel(&pixels, 29, 8, [188, 188, 188, 255]);
}

fn assert_pixel(pixels: &[u8], x: u32, y: u32, expected: [u8; 4]) {
    let actual = readback_pixel(pixels, x, y);
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&actual, expected)| actual.abs_diff(expected) <= 2),
        "pixel ({x}, {y}): expected {expected:?}, got {actual:?}"
    );
}

#[cfg(feature = "ui-submitter")]
pub(crate) fn overlay_packet(target: RenderTarget) -> Arc<nara_ui_render::UiPaintPacket> {
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
        UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
    };
    let node = UiNode::new(UiStyle::absolute(28.0, 4.0, 8.0, 8.0))
        .with_key(UiLocalKey::runtime("split-view-probe", "overlay").unwrap())
        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 0.5)));
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("split-view-probe", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(node),
        )
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.0, 1, 1, 1, 1).unwrap();
    let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    nara_ui_render::UiPresentationPlanner::default()
        .plan(frame, target, &Default::default())
        .unwrap()
}
