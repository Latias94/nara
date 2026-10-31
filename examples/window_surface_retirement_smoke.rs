use std::error::Error;
use std::io;
use std::process::{Child, Command, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nara::project_host::{
    DesktopPresentationControl, DesktopPresentationObservation, DesktopRun, DesktopRunOptions,
    DesktopRunOutcome,
};
use nara::render_wgpu::{
    __private::{WgpuPresentedFrameReceipt, WgpuRuntimeConnection},
    WgpuRenderObservation,
};
use nara::window::backend::{BackendWindowHandles, WindowTargetPhase};
use nara::{advanced_prelude::*, backend_prelude::*};

#[path = "support/desktop_run.rs"]
mod desktop_run;

const INITIAL_EXTENT: Extent2d = Extent2d {
    width: 320,
    height: 180,
};
const RESIZED_EXTENT: Extent2d = Extent2d {
    width: 400,
    height: 240,
};

fn main() -> Result<(), Box<dyn Error>> {
    let invocation = SmokeInvocation::from_args(std::env::args().skip(1))?;
    if invocation.child {
        return run_smoke_child(invocation);
    }
    run_smoke_parent(invocation)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct SmokeInvocation {
    child: bool,
    mutation: RetirementMutation,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum RetirementMutation {
    #[default]
    None,
    RemoveRenderIngress,
    ReplaceRenderIngress,
    ReplaceObservation,
    ReplaceWindowRegistry,
}

impl RetirementMutation {
    const fn argument(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::RemoveRenderIngress => Some("--remove-render-ingress-before-exit"),
            Self::ReplaceRenderIngress => Some("--replace-render-ingress-before-exit"),
            Self::ReplaceObservation => Some("--replace-observation-before-exit"),
            Self::ReplaceWindowRegistry => Some("--replace-window-registry-before-exit"),
        }
    }

    const fn refuses_capture(self) -> bool {
        matches!(self, Self::RemoveRenderIngress | Self::ReplaceRenderIngress)
    }
}

impl SmokeInvocation {
    fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, io::Error> {
        let mut invocation = Self::default();
        for argument in args {
            let mutation = match argument.as_str() {
                "--smoke-child" => {
                    invocation.child = true;
                    continue;
                }
                "--remove-render-ingress-before-exit" => RetirementMutation::RemoveRenderIngress,
                "--replace-render-ingress-before-exit" => RetirementMutation::ReplaceRenderIngress,
                "--replace-observation-before-exit" => RetirementMutation::ReplaceObservation,
                "--replace-window-registry-before-exit" => {
                    RetirementMutation::ReplaceWindowRegistry
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "the surface retirement smoke received an unsupported argument",
                    ));
                }
            };
            if invocation.mutation != RetirementMutation::None {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the surface retirement smoke accepts one endpoint mutation per run",
                ));
            }
            invocation.mutation = mutation;
        }
        Ok(invocation)
    }
}

fn run_smoke_parent(invocation: SmokeInvocation) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--smoke-child");
    if let Some(argument) = invocation.mutation.argument() {
        command.arg(argument);
    }
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return finish_smoke_child(status),
            Ok(None) => {}
            Err(poll_error) => {
                let reap_error = terminate_and_reap(&mut child).err();
                return Err(io::Error::other(format!(
                    "surface retirement smoke polling failed: {poll_error}; reap={reap_error:?}"
                ))
                .into());
            }
        }
        if Instant::now() >= deadline {
            let status = terminate_and_reap(&mut child)?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("surface retirement smoke exceeded its hard deadline: {status}"),
            )
            .into());
        }
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn terminate_and_reap(child: &mut Child) -> Result<ExitStatus, io::Error> {
    let kill_error = child.kill().err();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::park_timeout(Duration::from_millis(10));
            }
            Ok(None) => {
                return Err(io::Error::other(match kill_error {
                    Some(kill_error) => {
                        format!("kill={kill_error}; child remained live past the reap deadline")
                    }
                    None => "child remained live past the reap deadline".to_owned(),
                }));
            }
            Err(poll_error) => {
                return Err(io::Error::other(match kill_error {
                    Some(kill_error) => format!("kill={kill_error}; poll={poll_error}"),
                    None => format!("poll={poll_error}"),
                }));
            }
        }
    }
}

fn finish_smoke_child(status: ExitStatus) -> Result<(), Box<dyn Error>> {
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("surface retirement smoke child failed: {status}")).into())
    }
}

fn run_smoke_child(invocation: SmokeInvocation) -> Result<(), Box<dyn Error>> {
    let evidence = Arc::new(Mutex::new(SmokeEvidence::default()));
    let replacement_registry = BackendWindowHandles::default();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut app = App::new();
    app.insert_resource(SmokeState {
        mutation: invocation.mutation,
        resize_requested: false,
        evidence: Arc::clone(&evidence),
        replacement_registry: replacement_registry.clone(),
    })?;
    app.add_plugins((
        MinimalPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "nara surface retirement smoke",
                WindowResolution::new(320, 180),
            )),
        },
        WgpuBackendPlugins,
    ))?;
    app.add_systems(StartupStage::Scene, setup_scene)?
        .add_systems(CoreStage::Last, advance_smoke)?;
    let runtime = nara::app::RuntimeInstance::start(app)?;

    assert!(runtime.world().contains_resource::<WgpuRuntimeConnection>());
    assert!(runtime.world().contains_resource::<WgpuRenderObservation>());
    assert!(
        runtime
            .world()
            .contains_resource::<nara::window::backend::BackendWindowHandles>()
    );
    assert!(runtime.world().contains_resource::<ExtractedViews>());
    assert!(runtime.world().contains_resource::<RenderFrame>());
    assert!(runtime.world().contains_resource::<FrameStats>());
    assert!(runtime.world().contains_resource::<RenderBackendStatus>());
    assert!(runtime.world().contains_resource::<AppExitRequests>());

    let target_authority = runtime
        .world()
        .resource::<nara::window::backend::BackendWindowHandles>()
        .clone();
    let observer_evidence = Arc::clone(&evidence);
    let observer_authority = target_authority.clone();
    let options = DesktopRunOptions::new()
        .with_native_retirement_budget(Duration::from_secs(5))
        .with_presentation_observer(move |observation| {
            observe_smoke(
                observation,
                deadline,
                &observer_authority,
                &observer_evidence,
                invocation.mutation,
            )
        });
    let mut run = DesktopRun::from_runtime(runtime, options);
    let report = run.execute();
    match report.outcome() {
        DesktopRunOutcome::Completed(_) if !invocation.mutation.refuses_capture() => {}
        DesktopRunOutcome::Failed if invocation.mutation.refuses_capture() => {
            assert!(
                report.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "project.desktop.runner-failed"
                }),
                "an ingress refusal must retain the native runner failure: {:?}",
                report.diagnostics(),
            );
        }
        // Preserve incomplete native custody in the typed example error. An expected ingress
        // refusal is successful only after physical retirement, never on CleanupIncomplete.
        _ => {
            let report = desktop_run::finish_desktop_run(run)?;
            return Err(io::Error::other(format!(
                "unexpected surface smoke outcome: {:?}; diagnostics={:?}",
                report.outcome(),
                report.diagnostics(),
            ))
            .into());
        }
    }
    let evidence = evidence.lock().unwrap();
    assert!(
        !evidence.timed_out,
        "surface retirement smoke exceeded its deadline"
    );
    let initial = evidence
        .initial
        .expect("the configured native window must present before mutation");
    let resized = evidence
        .resized
        .expect("the configured native surface must present its resized extent");
    assert!(resized.render_transaction_frame_index() > initial.render_transaction_frame_index());
    assert_eq!(initial.window_id(), WindowId::PRIMARY);
    assert_eq!(resized.window_id(), WindowId::PRIMARY);
    assert_eq!(resized.backend_instance_id(), initial.backend_instance_id());
    assert_eq!(resized.device_epoch(), initial.device_epoch());
    let target = target_authority
        .snapshot(WindowId::PRIMARY)
        .expect("the original target must remain observable after shutdown");
    assert_eq!(target.phase, WindowTargetPhase::NativeDestroyed);
    assert_eq!(target.fault, None);
    assert!(!target.surface_active);
    assert!(!target.provider_present);
    assert!(target.native_destroyed);
    if invocation.mutation == RetirementMutation::ReplaceWindowRegistry {
        assert!(!replacement_registry.is_registered(WindowId::PRIMARY));
    }
    assert_eq!(
        evidence.mutation_applied,
        invocation.mutation != RetirementMutation::None
    );
    if invocation.mutation != RetirementMutation::None && !invocation.mutation.refuses_capture() {
        assert!(
            evidence.post_mutation_presented,
            "replacing a CPU observation or registry must retain actual native presentation",
        );
    }
    Ok(())
}

#[derive(Debug, Resource)]
struct SmokeState {
    mutation: RetirementMutation,
    resize_requested: bool,
    evidence: Arc<Mutex<SmokeEvidence>>,
    replacement_registry: BackendWindowHandles,
}

#[derive(Debug, Default)]
struct SmokeEvidence {
    initial: Option<WgpuPresentedFrameReceipt>,
    resized: Option<WgpuPresentedFrameReceipt>,
    mutation_applied: bool,
    post_mutation_presented: bool,
    timed_out: bool,
}

fn setup_scene(mut commands: Commands) {
    commands.spawn((
        Name::new("camera"),
        Transform2d::default(),
        Camera2d {
            clear_color: Some(Color::rgb(0.08, 0.1, 0.13)),
            ..Camera2d::default()
        },
    ));
}

fn advance_smoke(
    mut commands: Commands,
    mut windows: Query<&mut Window>,
    mut state: ResMut<SmokeState>,
    mut exit: ResMut<AppExitRequests>,
) {
    let evidence = Arc::clone(&state.evidence);
    let evidence = evidence.lock().unwrap();
    if evidence.initial.is_none() {
        return;
    }
    if !state.resize_requested {
        let mut window = windows
            .iter_mut()
            .find(|window| window.id == WindowId::PRIMARY)
            .expect("the smoke window must have its configured entity");
        window.resolution.physical_width = RESIZED_EXTENT.width;
        window.resolution.physical_height = RESIZED_EXTENT.height;
        state.resize_requested = true;
        return;
    }
    if evidence.resized.is_none() {
        return;
    }
    if state.mutation != RetirementMutation::None && !evidence.mutation_applied {
        let mutation = state.mutation;
        let replacement_registry = state.replacement_registry.clone();
        let evidence = Arc::clone(&state.evidence);
        commands.queue(move |world: &mut World| {
            match mutation {
                RetirementMutation::None => unreachable!("only a selected mutation is queued"),
                RetirementMutation::RemoveRenderIngress => {
                    assert!(world.remove_resource::<WgpuRuntimeConnection>().is_some());
                }
                RetirementMutation::ReplaceRenderIngress => {
                    assert!(world.remove_resource::<WgpuRuntimeConnection>().is_some());
                    world.insert_resource(WgpuRuntimeConnection::default());
                }
                RetirementMutation::ReplaceObservation => {
                    assert!(world.remove_resource::<WgpuRenderObservation>().is_some());
                    world.insert_resource(WgpuRenderObservation::default());
                }
                RetirementMutation::ReplaceWindowRegistry => {
                    assert!(world.remove_resource::<BackendWindowHandles>().is_some());
                    world.insert_resource(replacement_registry);
                }
            }
            evidence.lock().unwrap().mutation_applied = true;
        });
    }
    exit.request_exit();
}

fn observe_smoke(
    observation: DesktopPresentationObservation<'_>,
    deadline: Instant,
    target_authority: &BackendWindowHandles,
    evidence: &Mutex<SmokeEvidence>,
    mutation: RetirementMutation,
) -> DesktopPresentationControl {
    let mut evidence = evidence.lock().unwrap();
    if observation.now() >= deadline {
        evidence.timed_out = true;
        return DesktopPresentationControl::Exit(AppExit::Requested);
    }
    let Some(presented) = observation.presented_frame() else {
        return DesktopPresentationControl::Continue;
    };
    assert_eq!(presented.window_id(), WindowId::PRIMARY);
    assert_eq!(
        presented.runtime_generation(),
        observation.runtime_generation()
    );
    let backend = observation.wgpu();
    assert_eq!(presented.backend_instance_id(), backend.instance_id());
    assert_eq!(presented.device_epoch(), backend.device_epoch());
    let transaction = backend.frame_transaction_stats();
    assert_eq!(
        transaction.app_frame_index(),
        Some(presented.app_frame_index())
    );
    assert_eq!(
        transaction.render_transaction_frame_index(),
        Some(presented.render_transaction_frame_index())
    );
    assert_eq!(transaction.packet_admissions(), 1);
    assert_eq!(transaction.packet_rejections(), 0);
    assert_eq!(transaction.surface_acquire_attempts(), 1);
    assert_eq!(transaction.surface_acquires(), 1);
    assert_eq!(transaction.queue_submissions(), 1);
    assert_eq!(transaction.presents(), 1);
    let world = observation
        .world()
        .expect("a presentation requires a live Runtime");
    let frame = world.resource::<RenderFrame>();
    assert_eq!(frame.index, presented.render_transaction_frame_index());
    assert_eq!(frame.state, RenderFrameState::Submitted);
    if evidence.mutation_applied {
        assert!(
            !mutation.refuses_capture(),
            "a foreign or missing ingress must refuse capture"
        );
        evidence.post_mutation_presented = true;
        let published = world.resource::<WgpuRenderObservation>();
        assert_eq!(published.instance_id(), backend.instance_id());
        assert_eq!(published.device_epoch(), backend.device_epoch());
        assert_eq!(published.frame_transaction_stats(), transaction);
        if mutation == RetirementMutation::ReplaceWindowRegistry {
            assert!(
                !world
                    .resource::<BackendWindowHandles>()
                    .shares_authority_with(target_authority)
            );
        }
    }
    let target = target_authority.snapshot(presented.window_id()).unwrap();
    assert_eq!(target.phase, WindowTargetPhase::Active);
    assert_eq!(
        target.surface_generation,
        Some(presented.surface_generation())
    );
    assert!(target.surface_active);
    assert!(target.provider_present);
    assert!(!target.native_destroyed);
    assert_eq!(target.fault, None);
    // This is a fact copied from the actual surface owner, not the next requested ECS extent.
    match backend.configured_surface_extent(presented.window_id()) {
        Some(INITIAL_EXTENT) if evidence.initial.is_none() => {
            evidence.initial = Some(presented);
        }
        Some(RESIZED_EXTENT) => {
            assert!(
                evidence.initial.is_some(),
                "resize cannot replace the first native presentation"
            );
            if evidence.resized.is_none() {
                evidence.resized = Some(presented);
            }
        }
        _ => {}
    }
    DesktopPresentationControl::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_routes_parent_and_child_modes_without_recursion() {
        assert_eq!(
            SmokeInvocation::from_args(Vec::<String>::new()).unwrap(),
            SmokeInvocation::default()
        );
        assert_eq!(
            SmokeInvocation::from_args([
                "--smoke-child".to_owned(),
                "--remove-render-ingress-before-exit".to_owned(),
            ])
            .unwrap(),
            SmokeInvocation {
                child: true,
                mutation: RetirementMutation::RemoveRenderIngress,
            }
        );
        assert!(SmokeInvocation::from_args(["--unknown".to_owned()]).is_err());
        assert!(
            SmokeInvocation::from_args([
                "--remove-render-ingress-before-exit".to_owned(),
                "--replace-render-ingress-before-exit".to_owned(),
            ])
            .is_err()
        );
        assert!(SmokeInvocation::from_args(["--drop-backend-before-exit".to_owned()]).is_err());
    }
}
