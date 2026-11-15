//! The physical source workflow through the original ordinary desktop product owner.

use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use nara::{
    app::{AppExit, FrameExecutionStart, RuntimeGeneration},
    core::{Color, Vec2},
    ecs::{Res, schedule::IntoScheduleConfigs},
    hierarchy::{HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter},
    identity::SceneInstanceId,
    project_host::{
        DesktopPresentationControl, DesktopRun, DesktopRunIntent, DesktopRunOptions,
        DesktopRunOutcome, DesktopRunProgress, ProjectSceneContentStamp,
    },
    render::{Camera2d, RenderFrameSourceGeneration, RenderTarget},
    render_wgpu::{WgpuBackendTelemetryConfig, WgpuPolicyCaptureSet, WgpuPolicyFrameReader},
    sprite::Sprite,
    transform::Transform2d,
    window::{PrimaryWindowId, Window, WindowId},
    winit::{WinitControlFlow, WinitEventLoop},
};

use super::{observations::MaterialFacts, *};

#[path = "desktop/observation.rs"]
mod observation;

use observation::PresentationState;

const POLL: Duration = Duration::from_millis(2);

#[derive(Clone, Debug)]
pub(super) struct FrameFacts {
    pub(super) app: u64,
    pub(super) runtime: RuntimeGeneration,
    pub(super) render: u64,
    pub(super) source: RenderFrameSourceGeneration,
    pub(super) window: WindowId,
    pub(super) step: Step,
    pub(super) commits: usize,
    pub(super) rejections: usize,
    pub(super) instance: SceneInstanceId,
    pub(super) content: ProjectSceneContentStamp,
    pub(super) material: MaterialFacts,
    pub(super) palette: [u8; 4],
}

#[derive(Clone)]
struct Configuration {
    files: GameFiles,
    presentation: PresentationState,
    fault: FaultMode,
}

impl ProductConfiguration for Configuration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"consumer.palette.source-watching.desktop.v1\0");
        self.files.write_canonical(output);
        output.push(self.fault as u8);
    }
}

struct DesktopGame(Configuration);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource)]
#[repr(u8)]
enum FaultMode {
    #[default]
    None,
    Update,
    Capture,
    Observer,
}

impl FaultMode {
    fn from_environment() -> Self {
        match std::env::var("NARA_SOURCE_WINDOW_FAULT") {
            Err(std::env::VarError::NotPresent) => Self::None,
            Ok(value) => match value.as_str() {
                "update" => Self::Update,
                "capture" => Self::Capture,
                "observer" => Self::Observer,
                _ => panic!("NARA_SOURCE_WINDOW_FAULT must be update, capture or observer"),
            },
            Err(_) => panic!("NARA_SOURCE_WINDOW_FAULT must be UTF-8"),
        }
    }
}

#[derive(Default, Resource)]
struct ViewReady(bool);

impl Plugin for DesktopGame {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-watching.desktop"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.register_component::<GameProgress>();
        app.insert_resource(self.0.files.clone())?
            .insert_resource(self.0.presentation.clone())?
            .insert_resource(self.0.fault)?
            .init_resource::<Workflow>()?
            .init_resource::<ViewReady>()?
            .add_systems(CoreStage::Update, update_desktop_game)?
            .add_systems(CoreStage::Update, configure_view.after(update_desktop_game))?
            .add_systems(
                CoreStage::Render,
                record
                    .after(WgpuPolicyCaptureSet::Seal)
                    .before(WgpuPolicyCaptureSet::Publish),
            )?;
        Ok(())
    }
}

fn update_desktop_game(world: &mut World) {
    let presentation = world.resource::<PresentationState>().clone();
    let _ = presentation.protect(|| update_desktop_game_inner(world));
}

fn update_desktop_game_inner(world: &mut World) {
    let workflow = world.resource::<Workflow>();
    if workflow.step == Step::Done {
        return;
    }
    if !world
        .resource::<PresentationState>()
        .update_is_allowed(workflow)
    {
        std::thread::sleep(Duration::from_millis(1));
        return;
    }
    assert!(
        *world.resource::<FaultMode>() != FaultMode::Update || workflow.commits < 2,
        "injected Update panic after original native presentation"
    );
    update_game(world);
}

fn configure_view(world: &mut World) {
    let presentation = world.resource::<PresentationState>().clone();
    let _ = presentation.protect(|| configure_view_inner(world));
}

fn configure_view_inner(world: &mut World) {
    if !world.resource::<ViewReady>().0 {
        assert_eq!(world.query::<&Window>().iter(world).count(), 1);
        let window = world.resource::<PrimaryWindowId>().0;
        let camera = world
            .spawn((
                Camera2d {
                    target: RenderTarget::Window(window),
                    viewport_height: 24.0,
                    clear_color: Some(Color::rgba(0.0, 0.0, 0.0, 0.0)),
                    ..Default::default()
                },
                Transform2d::default(),
            ))
            .id();
        HierarchyWriter::new(world)
            .apply_batch(&[HierarchyOperation::Place {
                entity: camera,
                placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
            }])
            .unwrap();
        world.resource_mut::<ViewReady>().0 = true;
    }
    let entity = world
        .resource::<Workflow>()
        .current
        .as_ref()
        .unwrap()
        .entity;
    if world.get::<Transform2d>(entity).is_none() {
        world.entity_mut(entity).insert(Transform2d::default());
    }
    let mut sprite = world.get_mut::<Sprite>(entity).unwrap();
    sprite.size = Vec2::splat(8.0);
    sprite.material.tint = Color::WHITE;
}

fn record(
    reader: WgpuPolicyFrameReader,
    workflow: Res<Workflow>,
    start: Res<FrameExecutionStart>,
    runtime: Res<RuntimeGeneration>,
    primary: Res<PrimaryWindowId>,
    presentation: Res<PresentationState>,
    fault: Res<FaultMode>,
) {
    let _ = presentation.protect(|| {
        assert!(
            *fault != FaultMode::Capture || workflow.commits < 2,
            "injected capture panic after original native predecessor presentation"
        );
        let Some(input) = reader
            .read()
            .expect("the original native capture must be valid")
        else {
            return;
        };
        assert_eq!(input.packet().views().len(), 1);
        assert_eq!(
            input.packet().views()[0].target,
            RenderTarget::Window(primary.0)
        );
        assert_eq!(
            input.packet().source_generation().runtime_generation(),
            Some(*runtime)
        );
        let current = workflow.current.as_ref().unwrap();
        if workflow.step == Step::Done {
            assert_eq!(workflow.commits, 8);
            assert_eq!(workflow.rejections, 3);
            assert!(workflow.request.is_none());
            assert!(workflow.candidate.is_none());
            assert_eq!(
                current.gameplay,
                GameProgress {
                    health: 7,
                    score: 128,
                    inventory: vec!["brass-key".to_owned(), "map".to_owned()],
                }
            );
        }
        let pixels = captured::material(input, current);
        presentation.record(FrameFacts {
            app: start.frame(),
            runtime: *runtime,
            render: input.packet().frame_index(),
            source: input.packet().source_generation(),
            window: primary.0,
            step: workflow.step,
            commits: workflow.commits,
            rejections: workflow.rejections,
            instance: current.instance.instance_id(),
            content: current.stamp,
            material: current.material.clone(),
            palette: pixels.palette,
        });
    });
}

fn drive(run: &mut DesktopRun, presentation: &PresentationState) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(150);
    loop {
        if Instant::now() >= deadline {
            return Err(
                "ordinary native Source workflow exceeded its cooperative deadline".to_owned(),
            );
        }
        match run.poll(POLL) {
            DesktopRunProgress::Running | DesktopRunProgress::Retiring => {}
            DesktopRunProgress::Completed(AppExit::Success) => break,
            DesktopRunProgress::Failed => {
                return Err(format!(
                    "ordinary native Source workflow failed: {:?}",
                    run.execute()
                ));
            }
            progress => {
                return Err(format!(
                    "ordinary native Source workflow stopped: {progress:?}"
                ));
            }
        }
        if let Some(error) = presentation.failure() {
            return Err(error);
        }
    }
    if let Some(error) = presentation.failure() {
        return Err(error);
    }
    let report = run.execute();
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Success)
        || report.diagnostics().has_errors()
    {
        return Err(format!(
            "ordinary native Source completion failed: {report:?}"
        ));
    }
    Ok(())
}

fn retire(run: &mut DesktopRun) -> Result<Option<WinitEventLoop>, String> {
    if !matches!(
        run.progress(),
        DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed
    ) {
        run.request_close();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut retries = 0;
    loop {
        match run.poll(POLL) {
            DesktopRunProgress::Completed(_) | DesktopRunProgress::Failed => break,
            DesktopRunProgress::Running | DesktopRunProgress::Retiring
                if Instant::now() < deadline => {}
            DesktopRunProgress::CleanupIncomplete if retries < 2 && Instant::now() < deadline => {
                retries += 1;
                run.execute();
            }
            progress => {
                return Err(format!(
                    "original native Source cleanup is unfinished: {progress:?}"
                ));
            }
        }
    }
    Ok(run.take_event_loop())
}

pub(crate) fn run() {
    let backend = std::env::var("WGPU_BACKEND").expect("select the actual Vulkan or DX12 adapter");
    assert!(matches!(backend.as_str(), "vulkan" | "dx12"));
    assert!(matches!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0" | "1")
    ));
    let fault = FaultMode::from_environment();
    let project = Project::new();
    let manifest_path = project.root().join("nara.toml");
    let original = fs::read_to_string(&manifest_path).unwrap();
    let mut manifest = original.replace(
        "requested = ['runtime-2d']",
        "requested = ['runtime-2d', 'asset-watch', 'presentation-2d', 'render-wgpu', 'desktop-winit']",
    );
    assert_ne!(manifest, original);
    manifest.push_str("\n[window]\nenabled = true\nwidth = 320\nheight = 240\n");
    fs::write(manifest_path, manifest).unwrap();
    let scene = fs::read(project.root().join(STARTUP)).unwrap();
    let proof = CloseProof::default();
    let presentation = PresentationState::default();
    let configuration = Configuration {
        files: GameFiles(project.root().to_owned()),
        presentation: presentation.clone(),
        fault,
    };
    let watcher = ProjectSceneWatchPlugin::new(project.root().to_owned()).unwrap();
    let product = recipe()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(watcher, Clone::clone)
        .unwrap()
        .add_configured_plugin::<DesktopGame, _, _>(configuration, |configuration| {
            DesktopGame(configuration.clone())
        })
        .unwrap()
        .add_configured_plugin::<retirement::ObserveClose, _, _>(proof.clone(), |proof| {
            retirement::ObserveClose(proof.clone())
        })
        .unwrap();
    let observed = presentation.clone();
    let options = DesktopRunOptions::new()
        .with_input_isolation(true)
        .with_control_flow(WinitControlFlow::Poll)
        .with_telemetry(WgpuBackendTelemetryConfig::default())
        .with_presentation_observer(move |facts| {
            observed
                .protect(|| {
                    assert!(
                        fault != FaultMode::Observer
                            || facts
                                .world()
                                .is_none_or(|world| world.resource::<Workflow>().commits < 2)
                            || facts.presented_frame().is_none(),
                        "injected observer panic after original native presentation"
                    );
                    observed.observe(facts)
                })
                .unwrap_or(DesktopPresentationControl::Exit(AppExit::Requested))
        });
    let mut run = DesktopRun::new(
        project.capability(),
        DesktopRunIntent::new()
            .with_recipe(product)
            .with_execution(options),
    );
    let result = catch_unwind(AssertUnwindSafe(|| drive(&mut run, &presentation)));
    let cleanup = catch_unwind(AssertUnwindSafe(|| retire(&mut run)));
    let event_loop = match cleanup {
        Ok(Ok(event_loop)) => event_loop,
        failure => {
            std::mem::forget(run);
            std::mem::forget(project);
            if let Some(panic) = presentation.take_panic() {
                eprintln!("original native cleanup also failed; owner retained until process exit");
                resume_unwind(panic);
            }
            if let Err(panic) = result {
                eprintln!("original native cleanup also failed; owner retained until process exit");
                resume_unwind(panic);
            }
            match failure {
                Ok(Err(error)) => panic!("{error}; original owner retained until process exit"),
                Err(panic) => resume_unwind(panic),
                Ok(Ok(_)) => unreachable!(),
            }
        }
    };
    if let Some(panic) = presentation.take_panic() {
        assert!(
            event_loop.is_some(),
            "failed fixture must recover its original physical pump"
        );
        presentation.verify_retired().unwrap();
        proof.assert_joined_before_world_drop();
        eprintln!(
            "native source fault cleanup: mode={fault:?} physical_loop_recovered=true native_destroyed=true watchers_joined_before_world_drop=true"
        );
        resume_unwind(panic);
    }
    match result {
        Ok(result) => result.unwrap(),
        Err(panic) => resume_unwind(panic),
    }
    assert!(
        event_loop.is_some(),
        "recover the original fully retired physical pump"
    );
    assert!(
        run.take_event_loop().is_none(),
        "the original physical pump transfers once"
    );
    presentation.verify_retired().unwrap();
    proof.assert_joined_before_world_drop();
    presentation.assert_success();
    assert_eq!(fs::read(project.root().join(STARTUP)).unwrap(), scene);
}
