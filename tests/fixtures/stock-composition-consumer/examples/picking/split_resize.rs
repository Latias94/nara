use std::{
    cell::RefCell,
    io,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    rc::Rc,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use nara::{
    Presentation2dPlugins, Runtime2dPlugins,
    app::{App, AppExit, RuntimeGeneration, RuntimeInstance},
    core::{Color, Vec2},
    ecs::{Entity, Query, bevy_ecs::system::SystemState},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopRun, DesktopRunOptions,
        DesktopRunProgress,
    },
    render::{
        Camera2d, ExtractedView, ExtractedViews, FrameStats, RenderFrame, ViewportRect,
        WorldPointer,
    },
    sprite::Sprite,
    transform::Transform2d,
    window::{Window, WindowPlugin, WindowResolution},
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuStockCapturePlugin, WgpuStockPolicyPlugin, WgpuStockSpritePlugin,
    WgpuWindowPresentationPlugin,
};

use super::{
    application::retire_original,
    native::NativeProbe,
    scene::{OriginalWorld, Result},
};

const HALF_WIDTH: u32 = 320;
const HEIGHT: u32 = 360;
const LEFT: Vec2 = Vec2::new(-12.0, 24.0);
const RIGHT: Vec2 = Vec2::new(32.0, -16.0);

#[derive(Clone, Copy)]
struct Presented {
    generation: RuntimeGeneration,
    host: u64,
    frame: u64,
    resolution: WindowResolution,
    left: ExtractedView,
    right: Option<ExtractedView>,
    left_pick: Option<Vec2>,
    right_pick: Option<Vec2>,
    right_offset_pick: Option<Vec2>,
    right_outside: Option<Vec2>,
    sprites: u32,
}

type PointerState = SystemState<(
    Query<'static, 'static, &'static Window>,
    WorldPointer<'static, 'static>,
)>;
type Latest = Option<Result<Presented>>;

pub(super) fn run() -> Result<()> {
    let backend = std::env::var("WGPU_BACKEND")?;
    let fallback = std::env::var("NARA_WGPU_FORCE_FALLBACK")?;
    let title = format!("Nara split resize {}", std::process::id());
    let dropped = Arc::new(AtomicBool::new(false));
    let mut app = App::new();
    app.add_plugins((
        Runtime2dPlugins,
        Presentation2dPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                title.clone(),
                WindowResolution::new(HALF_WIDTH * 2, HEIGHT),
            )),
        },
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockSpritePlugin,
        WgpuStockPolicyPlugin,
        WgpuWindowPresentationPlugin,
    ))?;
    app.insert_resource(OriginalWorld(Arc::clone(&dropped)))?;
    let world = app.world_mut()?;
    let [left, right] = [(0, LEFT), (HALF_WIDTH, RIGHT)].map(|(x, translation)| {
        world
            .spawn((
                Camera2d {
                    viewport: ViewportRect::new(x, 0, HALF_WIDTH, HEIGHT),
                    viewport_height: HEIGHT as f32,
                    order: if x == 0 { 0 } else { 1 },
                    ..Default::default()
                },
                Transform2d::from_translation(translation),
            ))
            .id()
    });
    let sprite = world
        .spawn((
            Sprite::from_color(Vec2::splat(12.0), Color::WHITE),
            Transform2d::from_translation(LEFT),
        ))
        .id();
    HierarchyWriter::new(world).apply_batch(&[left, right, sprite].map(|entity| {
        HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }
    }))?;
    let mut pointer = PointerState::new(world);
    let runtime = RuntimeInstance::start(app)?;
    let observed = Rc::new(RefCell::new(None));
    let callback = Rc::clone(&observed);
    let options = DesktopRunOptions::new().with_presentation_observer(move |observation| {
        if callback.borrow().as_ref().is_some_and(Result::is_err) {
            return DesktopPresentationControl::Exit(AppExit::Requested);
        }
        if observation.presented_frame().is_some() {
            callback.replace(Some(observe(
                &observation,
                &mut pointer,
                left,
                right,
                &backend,
                &fallback,
            )));
        }
        DesktopPresentationControl::Continue
    });
    let mut run = DesktopRun::from_runtime(runtime, options);
    let mut probe = None;
    let trajectory = catch_unwind(AssertUnwindSafe(|| -> Result<()> {
        let initial = wait_for_frame(&mut run, &observed, None)?;
        probe = Some(NativeProbe::find_exact(&title).map_err(io::Error::other)?);
        let native = probe.as_ref().unwrap();
        let mut previous = initial;
        let mut original = None;
        for width in [640, 480, 320, 240, 640] {
            let size = [width, HEIGHT];
            let changed = native.resize_client(size).map_err(io::Error::other)?;
            let after = if changed { previous.frame } else { 0 };
            let presented = wait_for_frame(&mut run, &observed, Some((native, size, after)))?;
            assert_eq!(presented.generation, initial.generation);
            assert_eq!(presented.host, initial.host);
            let right_width = width.saturating_sub(HALF_WIDTH);
            assert_eq!(
                presented.left.viewport,
                ViewportRect::new(0, 0, width.min(HALF_WIDTH), HEIGHT).unwrap()
            );
            assert_eq!(
                presented.right.map(|view| view.viewport),
                ViewportRect::new(HALF_WIDTH, 0, right_width, HEIGHT)
            );
            assert!(
                presented
                    .left_pick
                    .is_some_and(|point| point.abs_diff_eq(LEFT, 1.0e-4))
            );
            if right_width > 0 {
                assert!(
                    presented
                        .right_pick
                        .is_some_and(|point| point.abs_diff_eq(RIGHT, 1.0e-4))
                );
                assert!(
                    presented.right_offset_pick.is_some_and(
                        |point| point.abs_diff_eq(RIGHT + Vec2::new(40.0, 20.0), 1.0e-4)
                    )
                );
            } else {
                assert!(presented.right_pick.is_none());
            }
            assert!(presented.right_outside.is_none());
            assert_eq!(presented.sprites, if right_width > 0 { 2 } else { 1 });
            let first = original.get_or_insert(presented);
            assert_eq!(presented.left.id, first.left.id);
            if let Some(right) = presented.right {
                assert_eq!(right.id, first.right.unwrap().id);
            }
            println!(
                "SPLIT_RESIZE_PRESENT width={width} right_width={right_width} sprites={} frame={}",
                presented.sprites, presented.frame
            );
            previous = presented;
        }
        let first = original.unwrap();
        assert_eq!(previous.left, first.left);
        assert_eq!(previous.right, first.right);
        Ok(())
    }));
    let retired = retire_original(&mut run, probe.as_ref(), &dropped);
    drop(run);
    match trajectory {
        Ok(Ok(())) => retired?,
        Ok(Err(failure)) => {
            if let Err(retirement) = retired {
                return Err(io::Error::other(format!("{failure}; {retirement}")).into());
            }
            return Err(failure);
        }
        Err(payload) => {
            if let Err(retirement) = retired {
                eprintln!("{retirement}");
            }
            resume_unwind(payload);
        }
    }
    println!(
        "SPLIT_RESIZE_OK original_runtime_retired=true original_window_destroyed=true original_pump_returned=true"
    );
    Ok(())
}

fn observe(
    observation: &DesktopPresentationObservation<'_>,
    pointer: &mut PointerState,
    left: Entity,
    right: Entity,
    backend: &str,
    fallback: &str,
) -> Result<Presented> {
    let receipt = observation
        .presented_frame()
        .expect("called after a present call");
    let adapter = observation
        .adapter_summary()
        .ok_or_else(|| io::Error::other("missing native adapter"))?;
    if adapter.backend() != backend || (fallback == "0" && adapter.device_type() == "cpu") {
        return Err(
            io::Error::other(format!("unexpected split resize adapter {adapter:?}")).into(),
        );
    }
    let world = observation
        .world()
        .ok_or_else(|| io::Error::other("missing live Runtime"))?;
    let frame = receipt.render_transaction_frame_index();
    if frame != world.resource::<RenderFrame>().index {
        return Err(
            io::Error::other("present receipt did not join the original extracted Views").into(),
        );
    }
    let views = world.resource::<ExtractedViews>().as_slice();
    let left_view = views
        .first()
        .copied()
        .filter(|view| view.camera_entity == left)
        .ok_or_else(|| {
            io::Error::other("the first presented View must belong to the left camera")
        })?;
    let right_view = match views {
        [_] => None,
        [_, view] if view.camera_entity == right => Some(*view),
        _ => return Err(io::Error::other("unexpected presented View order or count").into()),
    };
    let (windows, pointer) = pointer.get(world)?;
    let resolution = windows
        .iter()
        .find(|window| window.id == receipt.window_id())
        .ok_or_else(|| io::Error::other("presented window is not in the original World"))?
        .resolution;
    let scale = resolution.scale_factor as f32;
    let width = resolution.physical_width;
    let y = resolution.physical_height.min(HEIGHT) as f32 * 0.5;
    let pick = |camera, x, y| pointer.window_to_world(camera, Vec2::new(x, y) / scale);
    let right_x = if width > HALF_WIDTH {
        (width.min(HALF_WIDTH * 2) + HALF_WIDTH) as f32 * 0.5
    } else {
        width as f32 * 0.5
    };
    Ok(Presented {
        generation: observation.runtime_generation(),
        host: receipt.backend_instance_id(),
        frame,
        resolution,
        left: left_view,
        right: right_view,
        left_pick: pick(left, width.min(HALF_WIDTH) as f32 * 0.5, y),
        right_pick: pick(right, right_x, y),
        right_offset_pick: pick(right, right_x + 40.0, y - 20.0),
        right_outside: pick(right, width.min(HALF_WIDTH) as f32 - 1.0, y),
        sprites: world.resource::<FrameStats>().sprites,
    })
}

fn wait_for_frame(
    run: &mut DesktopRun,
    latest: &RefCell<Latest>,
    resize: Option<(&NativeProbe, [u32; 2], u64)>,
) -> Result<Presented> {
    let deadline = Instant::now() + Duration::from_secs(if resize.is_some() { 8 } else { 60 });
    while Instant::now() < deadline {
        let progress = run.poll(Duration::from_millis(10));
        if latest.borrow().as_ref().is_some_and(Result::is_err) {
            return latest.borrow_mut().take().unwrap();
        }
        if progress != DesktopRunProgress::Running {
            return Err(io::Error::other(format!(
                "original split Runtime left Running: {progress:?}"
            ))
            .into());
        }
        let Some(presented) = latest
            .borrow()
            .as_ref()
            .and_then(|frame| frame.as_ref().ok())
            .copied()
        else {
            continue;
        };
        let Some((native, size, after)) = resize else {
            return Ok(presented);
        };
        let geometry = native.geometry().map_err(io::Error::other)?;
        let resolution = presented.resolution;
        if presented.frame > after
            && geometry.size == size
            && [resolution.physical_width, resolution.physical_height] == size
            && resolution.scale_factor == geometry.scale
        {
            return Ok(presented);
        }
    }
    Err(io::Error::other(format!(
        "timed out waiting for native split resize to {:?}",
        resize.map(|(_, size, _)| size)
    ))
    .into())
}
