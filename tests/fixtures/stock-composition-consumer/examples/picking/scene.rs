use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use nara::{
    Presentation2dPlugins, Runtime2dPlugins,
    app::{App, CoreStage, RuntimeGeneration},
    core::{Color, Vec2},
    ecs::{
        Entity, Query, Res, ResMut, Resource, SystemParam, World, schedule::IntoScheduleConfigs,
    },
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    render::{
        Camera2d, Extent2d, ExtractedViews, RenderFrame, RenderFrameTarget, RenderWindowPacket,
        ViewportRect, WorldCanvas, build_render_frame_packet_with_world_canvas,
    },
    sprite::Sprite,
    transform::Transform2d,
    window::{PrimaryWindowId, Window, WindowPlugin, WindowResolution},
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuPolicyDefinition, WgpuPolicyFrameReader,
    WgpuPolicyFrameWriter, WgpuPolicyInstallation, WgpuRenderInput, WgpuRenderProgramError,
    WgpuStockCapturePlugin, WgpuStockCaptureSet, WgpuStockFrameReader, WgpuStockSpritePlugin,
    WgpuWindowPresentationPlugin, install_wgpu_policy,
};

use super::effect::{Mode, PickingPolicy, PolicyCapture};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub const CANVAS_WIDTH: u32 = 64;
pub const CANVAS_HEIGHT: u32 = 48;
pub const RETAINED_LIMIT: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraMirror {
    None,
    #[default]
    X,
    Y,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Revision {
    #[default]
    A,
    B,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
pub struct SceneState {
    pub mirror: CameraMirror,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub mode: Mode,
    pub state: SceneState,
}

#[derive(Clone, Copy, Debug, Resource)]
pub struct SceneHandles {
    pub camera: Entity,
    pub parent: Entity,
}

#[derive(Clone)]
pub struct RetainedCapture {
    pub input: WgpuRenderInput,
    pub mode: Mode,
    pub state: SceneState,
    pub window: RenderWindowPacket,
}

#[derive(Default, Resource)]
pub struct RetainedCaptures(pub Vec<RetainedCapture>);

#[derive(Resource)]
struct Requests(Arc<Mutex<VecDeque<Request>>>);

#[derive(Resource)]
struct CurrentMode(Mode);

#[derive(Resource)]
struct OriginalWorld(Arc<AtomicBool>);

impl Drop for OriginalWorld {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub fn build(
    title: String,
    commands: Arc<Mutex<VecDeque<Request>>>,
    dropped: Arc<AtomicBool>,
) -> Result<App> {
    let mut app = App::new();
    app.add_plugins((
        Runtime2dPlugins,
        Presentation2dPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(title, WindowResolution::new(384, 256))),
        },
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockSpritePlugin,
        WgpuWindowPresentationPlugin,
    ))?;
    install_wgpu_policy(
        &mut app,
        WgpuPolicyDefinition::new(PickingPolicy::new()).install()?,
    )?;
    app.insert_resource(WorldCanvas::new(CANVAS_WIDTH, CANVAS_HEIGHT).unwrap())?;
    app.insert_resource(SceneState::default())?;
    app.insert_resource(CurrentMode(Mode::Identity))?;
    app.insert_resource(Requests(commands))?;
    app.insert_resource(OriginalWorld(dropped))?;
    app.init_resource::<RetainedCaptures>()?;
    let state = SceneState::default();
    let world = app.world_mut()?;
    let parent = world.spawn(parent_transform(state)).id();
    let camera = world
        .spawn((
            child_transform(state),
            Camera2d {
                viewport: ViewportRect::new(4, 4, 56, 40),
                viewport_height: 10.0,
                clear_color: Some(Color::rgb(1.0, 1.0, 0.0)),
                ..Camera2d::default()
            },
        ))
        .id();
    let mut placements = vec![
        HierarchyOperation::Place {
            entity: parent,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        },
        HierarchyOperation::Place {
            entity: camera,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Parent { parent }),
        },
    ];
    for (position, size, color) in [
        ((-11.13, -6.27), (5.82, 4.26), Color::rgb(1.0, 0.0, 0.0)),
        ((8.31, 1.43), (7.18, 3.62), Color::rgb(0.0, 0.0, 1.0)),
        ((-0.73, -11.61), (4.46, 5.18), Color::rgb(0.0, 1.0, 0.0)),
    ] {
        let entity = world
            .spawn((
                Transform2d::from_translation(Vec2::new(position.0, position.1)),
                Sprite::from_color(Vec2::new(size.0, size.1), color),
            ))
            .id();
        placements.push(HierarchyOperation::Place {
            entity,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        });
    }
    HierarchyWriter::new(world).apply_batch(&placements)?;
    world.insert_resource(SceneHandles { camera, parent });
    app.add_systems(CoreStage::First, apply_requests)?;
    app.add_systems(
        CoreStage::Render,
        compose.in_set(WgpuStockCaptureSet::Compose),
    )?;
    app.add_systems(
        CoreStage::Render,
        retain_original
            .after(WgpuPolicyCaptureSet::Seal)
            .before(WgpuPolicyCaptureSet::Publish),
    )?;
    Ok(app)
}

fn parent_transform(state: SceneState) -> Transform2d {
    let signs = match state.mirror {
        CameraMirror::None => Vec2::new(1.0, 1.0),
        CameraMirror::X => Vec2::new(-1.0, 1.0),
        CameraMirror::Y => Vec2::new(1.0, -1.0),
        CameraMirror::Both => Vec2::new(-1.0, -1.0),
    };
    Transform2d {
        translation: match state.revision {
            Revision::A => Vec2::new(10.0, -4.0),
            Revision::B => Vec2::new(8.0, -1.0),
        },
        rotation: std::f32::consts::FRAC_PI_2,
        scale: signs * Vec2::new(2.0, 3.0),
    }
}

fn child_transform(state: SceneState) -> Transform2d {
    match state.revision {
        Revision::A => Transform2d {
            translation: Vec2::new(1.0, 2.0),
            rotation: 0.6_f32.atan2(0.8),
            ..Transform2d::IDENTITY
        },
        Revision::B => Transform2d {
            translation: Vec2::new(-0.5, 1.5),
            rotation: (-0.8_f32).atan2(0.6),
            ..Transform2d::IDENTITY
        },
    }
}

fn apply_requests(world: &mut World) -> nara::ecs::bevy_ecs::error::Result {
    let request = world
        .resource::<Requests>()
        .0
        .lock()
        .map_err(|_| WgpuRenderProgramError::feature("picking request mutex poisoned"))?
        .pop_front();
    let Some(request) = request else {
        return Ok(());
    };
    let handles = *world.resource::<SceneHandles>();
    *world
        .get_mut::<Transform2d>(handles.parent)
        .ok_or_else(|| {
            WgpuRenderProgramError::feature("the original camera parent is missing")
        })? = parent_transform(request.state);
    *world
        .get_mut::<Transform2d>(handles.camera)
        .ok_or_else(|| WgpuRenderProgramError::feature("the original camera is missing"))? =
        child_transform(request.state);
    *world.resource_mut::<SceneState>() = request.state;
    world.resource_mut::<CurrentMode>().0 = request.mode;
    Ok(())
}

#[derive(SystemParam)]
struct CaptureSource<'w, 's> {
    generation: Res<'w, RuntimeGeneration>,
    frame: Res<'w, RenderFrame>,
    primary: Res<'w, PrimaryWindowId>,
    windows: Query<'w, 's, &'static Window>,
    views: Res<'w, ExtractedViews>,
    state: Res<'w, SceneState>,
    mode: Res<'w, CurrentMode>,
}

fn compose(
    source: CaptureSource<'_, '_>,
    installation: Res<WgpuPolicyInstallation<PickingPolicy>>,
    mut reader: WgpuStockFrameReader,
    mut writer: WgpuPolicyFrameWriter,
) -> nara::ecs::bevy_ecs::error::Result {
    if !writer.can_capture(&installation) {
        return Ok(());
    }
    let Some(packet) = build_render_frame_packet_with_world_canvas(
        *source.generation,
        source.frame.index,
        Some(source.primary.0),
        source.windows.iter(),
        &source.views,
        source.views.world_canvas(),
    )?
    else {
        return Ok(());
    };
    let Some(frame) = reader.take(&installation, &writer) else {
        return Ok(());
    };
    let (stock, ui) = frame.into_parts();
    let capture = Arc::new(PolicyCapture {
        stock,
        mode: source.mode.0,
        state: *source.state,
        extent: packet.target().extent(),
    });
    if let Some(ui) = ui {
        if let Err(failure) = writer.capture_with_ui(&installation, capture, ui) {
            let (error, ui) = failure.into_parts();
            drop(ui);
            return Err(error.into());
        }
    } else if !writer.capture(&installation, capture)? {
        return Err(
            WgpuRenderProgramError::feature("picking policy capture admission changed").into(),
        );
    }
    Ok(())
}

fn retain_original(
    reader: WgpuPolicyFrameReader,
    mut retained: ResMut<RetainedCaptures>,
) -> nara::ecs::bevy_ecs::error::Result {
    let Some(input) = reader
        .read()
        .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
    else {
        return Ok(());
    };
    let capture = input.captured::<PickingPolicy>().ok_or_else(|| {
        WgpuRenderProgramError::feature("the sealed input has a different original policy")
    })?;
    let RenderFrameTarget::Window(window) = input.packet().target() else {
        return Err(
            WgpuRenderProgramError::feature("the original input must target its window").into(),
        );
    };
    let extent = Extent2d::new(
        window.resolution.physical_width,
        window.resolution.physical_height,
    );
    if extent != Some(capture.extent) {
        return Err(WgpuRenderProgramError::feature(
            "the original target extent changed during capture",
        )
        .into());
    }
    if retained
        .0
        .last()
        .is_some_and(|last| last.input.packet().frame_index() == input.packet().frame_index())
    {
        return Ok(());
    }
    if retained.0.len() == RETAINED_LIMIT {
        retained.0.remove(0);
    }
    retained.0.push(RetainedCapture {
        input: input.clone(),
        mode: capture.mode,
        state: capture.state,
        window,
    });
    Ok(())
}
