use std::sync::Arc;

use nara_ecs::{Mut, Resource, World};
use nara_hierarchy::HierarchyOrder;
use nara_input::{InputIngressError, InputPresentationEpoch, InputSurfaceId, InputTimeline};
use nara_ui_kernel::{
    UiCompileError, UiDispatchError, UiEnvironmentError, UiFrame, UiJournalError,
    UiPresentationReceipt, UiRouteStamp, UiSeatId, UiSemanticSnapshot, UiSurfaceId,
};
use thiserror::Error;

use crate::UiTheme;

pub(crate) mod candidate;
mod identity;
pub(crate) mod presentation;
mod projection;
pub(crate) mod state;
mod surface;

pub(crate) use state::UiRuntimeState;
pub use surface::{UiSurfaceDescriptor, UiSurfaceMappingError};

const MAX_ADAPTER_DIAGNOSTICS: usize = 64;
const MAX_DIAGNOSTIC_BYTES: usize = 1_024;

/// One bounded adapter diagnostic without runtime entity identity leakage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiAdapterDiagnostic {
    code: &'static str,
    message: String,
}

impl UiAdapterDiagnostic {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Current bounded projection diagnostics from the most recent compile pass.
#[derive(Debug, Default, Resource)]
pub struct UiAdapterDiagnostics {
    entries: Vec<UiAdapterDiagnostic>,
}

impl UiAdapterDiagnostics {
    #[must_use]
    pub fn as_slice(&self) -> &[UiAdapterDiagnostic] {
        &self.entries
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn replace(&mut self, entries: Vec<UiAdapterDiagnostic>) {
        self.entries = entries;
    }
}

#[derive(Debug, Error)]
pub enum UiAdapterError {
    #[error("UI runtime adapter is not installed")]
    RuntimeUnavailable,
    #[error("UI runtime setup failed: {0}")]
    RuntimeSetup(String),
    #[error("UI source revision space was exhausted")]
    SourceRevisionExhausted,
    #[error("UI surface {surface} was retired and cannot be reused")]
    RetiredSurface { surface: u64 },
    #[error("UI surface {surface} is unknown")]
    UnknownSurface { surface: u64 },
    #[error("UI surface limit was exceeded")]
    SurfaceLimitExceeded,
    #[error("UI surface identity lifetime space was exhausted")]
    SurfaceIdentitySpaceExhausted,
    #[error("UI input endpoint window {window} seat {seat} is already owned by surface {surface}")]
    InputEndpointConflict {
        window: u64,
        seat: u32,
        surface: u64,
    },
    #[error("the submitted frame is not the adapter's latest frame for surface {surface}")]
    FrameNotLatest { surface: u64 },
    #[error(
        "surface {surface} presentation epoch {presentation_epoch} has no retained behavior snapshot"
    )]
    PresentedBehaviorMissing {
        surface: u64,
        presentation_epoch: u64,
    },
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
    #[error(transparent)]
    Journal(#[from] UiJournalError),
    #[error(transparent)]
    Input(#[from] InputIngressError),
    #[error(transparent)]
    Dispatch(#[from] UiDispatchError),
    #[error("UI widget interaction failed: {0}")]
    Widget(String),
}

#[derive(Resource)]
pub(crate) struct UiDefaultTheme(pub(crate) UiTheme);

pub(crate) fn compile_ui_frames(world: &mut World) {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let mut diagnostics = Vec::new();
        if nara_hierarchy::__private::completed_topology_generation(world).is_none() {
            push_diagnostic(
                &mut diagnostics,
                "hierarchy-incomplete",
                "UI compilation requires a completed structural hierarchy generation",
            );
            world
                .resource_mut::<UiAdapterDiagnostics>()
                .replace(diagnostics);
            return;
        }
        let Some(order) = world.get_resource::<HierarchyOrder>() else {
            push_diagnostic(
                &mut diagnostics,
                "hierarchy-order-missing",
                "UI compilation requires the hierarchy order projection",
            );
            world
                .resource_mut::<UiAdapterDiagnostics>()
                .replace(diagnostics);
            return;
        };
        let ordered_entities = order.as_slice().to_vec();
        let theme = projection::select_theme(world, &ordered_entities, &mut diagnostics);
        let source_revision = state.source_revision();
        if state.advance_source_revision().is_err() {
            push_diagnostic(
                &mut diagnostics,
                "source-revision-exhausted",
                "UI source revision space was exhausted",
            );
            world
                .resource_mut::<UiAdapterDiagnostics>()
                .replace(diagnostics);
            return;
        }

        for (_, surface) in state.surfaces_mut() {
            let Some(projected) = projection::project_authoring(
                world,
                &ordered_entities,
                source_revision,
                surface.reducer_states(),
                &mut diagnostics,
            ) else {
                continue;
            };
            let environment = match surface.descriptor().environment(theme.revision().get()) {
                Ok(environment) => environment,
                Err(error) => {
                    push_diagnostic(
                        &mut diagnostics,
                        "surface-environment-rejected",
                        &error.to_string(),
                    );
                    continue;
                }
            };
            if let Err(error) = surface.publish_candidate(projected, environment, theme.kernel()) {
                push_compile_diagnostic(&mut diagnostics, &error);
            }
        }
        world
            .resource_mut::<UiAdapterDiagnostics>()
            .replace(diagnostics);
    });
}

/// Installs or updates one active Host-owned surface without reusing retired identities.
pub fn install_ui_surface(
    world: &mut World,
    descriptor: UiSurfaceDescriptor,
) -> Result<(), UiAdapterError> {
    let theme_revision = world
        .get_resource::<UiDefaultTheme>()
        .map_or_else(UiTheme::default, |theme| theme.0)
        .revision()
        .get();
    descriptor.environment(theme_revision)?;
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        if let Some(surface) = state.conflicting_input_endpoint(descriptor) {
            return Err(UiAdapterError::InputEndpointConflict {
                window: descriptor.input_window().get(),
                seat: descriptor.input_seat().get(),
                surface: surface.get(),
            });
        }
        let previous = state.surface_descriptor(descriptor.surface_id());
        let prepared = state.prepare_new_surface(descriptor)?;
        if let Some(previous) = previous
            && (previous.input_window() != descriptor.input_window()
                || previous.input_seat() != descriptor.input_seat())
        {
            world
                .resource_mut::<InputTimeline>()
                .release_presented_route(
                    previous.input_window(),
                    previous.input_seat(),
                    InputSurfaceId::new(descriptor.surface_id().get()),
                )?;
        }
        state.commit_surface_install(descriptor, prepared);
        Ok(())
    })
}

/// Retires one surface after releasing only the input endpoint it owns.
pub fn retire_ui_surface(
    world: &mut World,
    surface_id: UiSurfaceId,
) -> Result<bool, UiAdapterError> {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let Some(descriptor) = state.surface_descriptor(surface_id) else {
            return Ok(false);
        };
        world
            .resource_mut::<InputTimeline>()
            .release_presented_route(
                descriptor.input_window(),
                descriptor.input_seat(),
                InputSurfaceId::new(surface_id.get()),
            )?;
        Ok(state.retire_surface(surface_id))
    })
}

/// Returns the latest compiled immutable frame for one surface.
pub fn latest_ui_frame(
    world: &World,
    surface_id: UiSurfaceId,
) -> Result<Option<Arc<UiFrame>>, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    Ok(state
        .surface(surface_id)
        .and_then(|surface| surface.candidate())
        .map(state::candidate_frame))
}

/// Returns the latest immutable frame for every installed surface in stable surface order.
pub fn latest_ui_frames(world: &World) -> Result<Vec<Arc<UiFrame>>, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    Ok(state
        .surfaces()
        .filter_map(|(_, surface)| surface.candidate().map(state::candidate_frame))
        .collect())
}

/// Returns the active Host-owned surface descriptors in stable surface order.
///
/// Presentation adapters use this bounded snapshot to mirror explicit window routes without
/// making the renderer-neutral UI authoring crate depend on a presentation backend.
pub fn active_ui_surface_descriptors(
    world: &World,
) -> Result<Vec<UiSurfaceDescriptor>, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    Ok(state
        .surfaces()
        .map(|(_, surface)| surface.descriptor())
        .collect())
}

/// Commits backend acceptance of the exact current frame and publishes its input route.
pub fn commit_ui_presentation(
    world: &mut World,
    frame: Arc<UiFrame>,
    presentation_epoch: u64,
) -> Result<UiPresentationReceipt, UiAdapterError> {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let surface_id = frame.environment().surface_id();
        let (descriptor, presented, receipt) = {
            let surface = state
                .surface(surface_id)
                .ok_or(UiAdapterError::UnknownSurface {
                    surface: surface_id.get(),
                })?;
            let Some(candidate) = surface.candidate() else {
                return Err(UiAdapterError::FrameNotLatest {
                    surface: surface_id.get(),
                });
            };
            if !Arc::ptr_eq(candidate.frame(), &frame) {
                return Err(UiAdapterError::FrameNotLatest {
                    surface: surface_id.get(),
                });
            }
            let (presented, receipt) = surface.prepare_presentation(presentation_epoch)?;
            (surface.descriptor(), presented, receipt)
        };
        let epoch = InputPresentationEpoch::new(presentation_epoch)
            .ok_or(UiJournalError::ZeroPresentationEpoch)?;
        world
            .resource_mut::<InputTimeline>()
            .publish_presented_route(
                descriptor.input_window(),
                InputSurfaceId::new(surface_id.get()),
                descriptor.input_seat(),
                epoch,
            )?;
        state
            .surface_mut(surface_id)
            .expect("validated UI surface remains installed")
            .commit_presentation(presented);
        Ok(receipt)
    })
}

/// Projects accessibility semantics from the latest submitted frame and interaction state.
pub fn ui_semantic_snapshot(
    world: &World,
    surface_id: UiSurfaceId,
    presentation_epoch: u64,
    seat: UiSeatId,
) -> Result<UiSemanticSnapshot, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    let surface = state
        .surface(surface_id)
        .ok_or(UiAdapterError::UnknownSurface {
            surface: surface_id.get(),
        })?;
    surface.semantic_snapshot(UiRouteStamp::new(surface_id, presentation_epoch), seat)
}

fn push_compile_diagnostic(diagnostics: &mut Vec<UiAdapterDiagnostic>, error: &UiCompileError) {
    push_diagnostic(diagnostics, "compile-rejected", &error.to_string());
}

pub(super) fn push_diagnostic(
    diagnostics: &mut Vec<UiAdapterDiagnostic>,
    code: &'static str,
    message: &str,
) {
    if diagnostics.len() == MAX_ADAPTER_DIAGNOSTICS {
        return;
    }
    diagnostics.push(UiAdapterDiagnostic {
        code,
        message: truncate_message(message),
    });
}

fn truncate_message(message: &str) -> String {
    if message.len() <= MAX_DIAGNOSTIC_BYTES {
        return message.to_string();
    }
    let mut end = MAX_DIAGNOSTIC_BYTES;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].to_string()
}
