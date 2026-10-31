use super::*;
use nara_app::{App, RuntimeInstance};
use nara_ecs::{Entity, World, bevy_ecs::system::SystemState};
use nara_render::{
    Color, ExtractedView, ExtractedViews, RenderFramePacket, RenderTarget, ViewLoadOp,
    ViewMapping2d, ViewportRect, WorldPointer, build_render_frame_packet,
};
use nara_window::{Window, WindowResolution};

struct PresentedWorld {
    world: World,
    camera: Entity,
    window: Window,
    packet: RenderFramePacket,
}

impl PresentedWorld {
    fn new(runtime: &RuntimeInstance, window_id: WindowId, frame: u64) -> Self {
        Self::with_scale(runtime, window_id, frame, 1.0)
    }

    fn with_scale(runtime: &RuntimeInstance, window_id: WindowId, frame: u64, scale: f64) -> Self {
        let mut world = World::new();
        let mut resolution = WindowResolution::new(64, 64);
        resolution.scale_factor = scale;
        let window = Window {
            id: window_id,
            resolution,
            ..Default::default()
        };
        world.spawn(window.clone());
        world.insert_resource(runtime.generation());
        let camera = world.spawn_empty().id();
        let viewport = ViewportRect::new(0, 0, 64, 64).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: camera,
            target: RenderTarget::Window(window.id),
            viewport,
            mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 64.0, viewport)
                .unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let packet = build_render_frame_packet(
            runtime.generation(),
            frame,
            Some(window.id),
            [&window],
            &views,
        )
        .unwrap()
        .unwrap();
        Self {
            world,
            camera,
            window,
            packet,
        }
    }

    fn can_pick(&mut self) -> bool {
        SystemState::<WorldPointer>::new(&mut self.world)
            .get(&self.world)
            .unwrap()
            .window_to_world(self.camera, nara_core::Vec2::splat(32.0))
            .is_some()
    }

    // These CPU state-transition cases deliberately do not claim native presentation.
    fn seed(&mut self, host: &mut WgpuRenderHost, producer: &super::super::WgpuRenderProducer) {
        let (snapshot, guard) =
            nara_render::__private::prepare_world_presentation(&self.packet).unwrap();
        guard.mark_presented();
        let view = host.views.get_mut(&producer.id()).unwrap();
        view.admission.source_generation = Some(self.packet.source_generation());
        view.world_presentation
            .replace(&view.closed, guard.revoker())
            .unwrap();
        view.runtime_presentation = Some(RuntimeWorldPresentation {
            _guard: guard,
            window: nara_render::RenderWindowPacket {
                id: self.window.id,
                resolution: self.window.resolution,
                present_mode: self.window.present_mode,
            },
            canvas: None,
        });
        self.world.insert_resource(snapshot);
    }
}

#[test]
fn producer_drop_revokes_current_and_delayed_world_permissions_without_host_maintenance() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let mut successor = PresentedWorld::new(&runtime, WindowId::new(1), 2);
    let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
    first.seed(&mut host, &a);
    second.seed(&mut host, &b);
    assert!(first.can_pick());
    successor.seed(&mut host, &a);
    assert!(!first.can_pick());
    assert!(successor.can_pick());
    let delayed = successor
        .world
        .remove_resource::<nara_render::__private::PresentedWorldFrame>()
        .unwrap();
    drop(a);
    // Returning the original result after close cannot reactivate its permission.
    successor.world.insert_resource(delayed);
    assert!(!first.can_pick());
    assert!(!successor.can_pick());
    assert!(second.can_pick());
    // No Host method has run since Drop. Native cleanup is a separate later responsibility.
    runtime.finish().unwrap();
}

#[test]
fn a_closed_producer_refuses_and_revokes_a_new_world_permission() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let mut frame = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let (snapshot, guard) =
        nara_render::__private::prepare_world_presentation(&frame.packet).unwrap();
    guard.mark_presented();
    frame.world.insert_resource(snapshot);
    assert!(frame.can_pick());
    let slot = std::sync::Arc::clone(&producer.world_presentation);
    let closed = std::sync::Arc::clone(&producer.closed);
    drop(producer);
    assert!(matches!(
        slot.replace(&closed, guard.revoker()),
        Err(WgpuRenderHostError::ProducerClosed)
    ));
    assert!(!frame.can_pick());
    runtime.finish().unwrap();
}

#[test]
fn changed_window_or_retired_authority_revokes_only_its_producer() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    for retire_authority in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        let a = host.create_producer().unwrap();
        let b = host.create_producer().unwrap();
        let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
        let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
        first.seed(&mut host, &a);
        second.seed(&mut host, &b);
        assert!(first.can_pick());
        assert!(second.can_pick());
        let mut current = nara_render::RenderWindowPacket {
            id: first.window.id,
            resolution: first.window.resolution,
            present_mode: first.window.present_mode,
        };
        if retire_authority {
            host.handles = Some(BackendWindowHandles::default());
        } else {
            current.resolution = WindowResolution::new(128, 64);
        }
        host.validate_runtime_presentation(&a, runtime.generation(), Some(current), None);
        assert!(!first.can_pick());
        assert!(second.can_pick());
        assert!(host.views[&a.id()].runtime_presentation.is_none());
        assert!(host.views[&b.id()].runtime_presentation.is_some());
    }
    runtime.finish().unwrap();
}

#[test]
#[ignore = "requires an explicitly selected native GPU; verifies owner registration, not native window presentation"]
fn native_runtime_world_guards_replace_and_retire_per_producer_and_revoke_on_device_loss() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let namespace = host.prepare_namespace(None).unwrap();
    let a = host.create_producer().unwrap();
    let b = host.create_producer().unwrap();
    let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
    first.world.insert_resource(
        host.commit_runtime_world_presentation(a.id(), &first.packet)
            .unwrap()
            .unwrap(),
    );
    second.world.insert_resource(
        host.commit_runtime_world_presentation(b.id(), &second.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(first.can_pick());
    assert!(second.can_pick());

    let mut successor = PresentedWorld::new(&runtime, WindowId::new(1), 2);
    successor.world.insert_resource(
        host.commit_runtime_world_presentation(a.id(), &successor.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(!first.can_pick());
    assert!(successor.can_pick());
    assert!(second.can_pick());
    let replacement = host.create_producer().unwrap();
    let mut same_window = PresentedWorld::new(&runtime, WindowId::new(1), 1);
    same_window.world.insert_resource(
        host.commit_runtime_world_presentation(replacement.id(), &same_window.packet)
            .unwrap()
            .unwrap(),
    );
    assert!(
        !successor.can_pick(),
        "the same surface no longer displays A's pixels"
    );
    assert!(same_window.can_pick());
    assert!(
        second.can_pick(),
        "another window still displays B's pixels"
    );
    host.retire_producer(&a);
    assert!(!successor.can_pick());
    assert!(
        same_window.can_pick(),
        "closing A cannot revoke its target's successor"
    );
    assert!(second.can_pick());
    namespace.device.destroy();
    namespace.device.poll(wgpu::PollType::Poll).unwrap();
    assert!(!second.can_pick());
    assert!(!same_window.can_pick());
    drop(host);
    runtime.finish().unwrap();
}

#[test]
fn another_runtime_cannot_revoke_the_original_producers_world_permission() {
    let original = RuntimeInstance::start(App::new()).unwrap();
    let foreign = RuntimeInstance::start(App::new()).unwrap();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let mut frame = PresentedWorld::new(&original, WindowId::new(1), 1);
    frame.seed(&mut host, &producer);
    assert!(frame.can_pick());
    // Foreign geometry is not a lifecycle observation of the original producer's Runtime.
    host.validate_runtime_presentation(&producer, foreign.generation(), None, None);
    assert!(frame.can_pick());
    // The actual owner losing its window still revokes the original permission.
    host.validate_runtime_presentation(&producer, original.generation(), None, None);
    assert!(!frame.can_pick());
    drop(host);
    original.finish().unwrap();
    foreign.finish().unwrap();
}

#[test]
#[ignore = "requires an explicitly selected native GPU; verifies owner registration, not native window presentation"]
fn native_unpickable_successor_revokes_displaced_world_without_issuing_permission() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    for same_producer in [false, true] {
        let mut host = WgpuRenderHost::offscreen();
        host.prepare_namespace(None).unwrap();
        let a = host.create_producer().unwrap();
        let b = host.create_producer().unwrap();
        let c = host.create_producer().unwrap();
        let mut first = PresentedWorld::new(&runtime, WindowId::new(1), 1);
        let mut second = PresentedWorld::new(&runtime, WindowId::new(2), 1);
        first.world.insert_resource(
            host.commit_runtime_world_presentation(a.id(), &first.packet)
                .unwrap()
                .unwrap(),
        );
        second.world.insert_resource(
            host.commit_runtime_world_presentation(b.id(), &second.packet)
                .unwrap()
                .unwrap(),
        );
        assert!(first.can_pick());
        assert!(second.can_pick());
        // Packet construction admits physical pixels even when no logical mapping can be made.
        let successor = PresentedWorld::with_scale(&runtime, WindowId::new(1), 2, 0.0);
        let owner = if same_producer { a.id() } else { c.id() };
        assert!(
            host.commit_runtime_world_presentation(owner, &successor.packet)
                .unwrap()
                .is_none()
        );
        assert!(!first.can_pick());
        assert!(second.can_pick());
        assert!(host.views[&owner].runtime_presentation.is_none());
        drop(host);
    }
    runtime.finish().unwrap();
}
