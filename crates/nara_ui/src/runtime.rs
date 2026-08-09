use std::sync::Arc;

use nara_ecs::{Mut, Resource, World};
use nara_hierarchy::HierarchyOrder;
use nara_input::{
    InputGeometryGeneration, InputIngressError, InputPresentationEpoch, InputPresentedRoute,
    InputRouteGeneration, InputSlotGeneration, InputSurfaceId, InputTimeline,
};
use nara_ui_kernel::{
    UiDispatchError, UiEnvironmentError, UiFrame, UiJournalError, UiPresentationReceipt,
    UiRouteStamp, UiSeatId, UiSemanticSnapshot, UiSurfaceId,
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
pub use surface::{UiSurfaceEndpoint, UiSurfaceRetirement, UiSurfaceSlot, UiSurfaceSlotError};

/// Move-only UI authority staged before one exact backend submission.
///
/// The ordinary runtime API does not expose this token. The first-party Host presentation bridge
/// retains it until the backend accepts, defers, aborts, or supersedes the matching packet.
#[must_use = "a prepared UI presentation must be committed, aborted, or superseded"]
pub struct UiPreparedPresentationAuthority {
    surface_id: UiSurfaceId,
    facts: candidate::UiPresentationFacts,
    prepared: state::UiPreparedPresentation,
    input_route: nara_input::__private::PreparedInputRoutePublication,
}

impl UiPreparedPresentationAuthority {
    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub fn frame(&self) -> &Arc<UiFrame> {
        self.facts.frame()
    }

    #[must_use]
    pub const fn receipt(&self) -> UiPresentationReceipt {
        self.prepared.receipt()
    }
}

/// Move-only exact candidate retained while a backend makes retryable residency progress.
#[must_use = "a deferred UI presentation must be retried, aborted, or superseded"]
pub struct UiDeferredPresentationAuthority {
    surface_id: UiSurfaceId,
    facts: candidate::UiPresentationFacts,
    presentation_epoch: u64,
}

impl UiDeferredPresentationAuthority {
    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub fn frame(&self) -> &Arc<UiFrame> {
        self.facts.frame()
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.presentation_epoch
    }
}

/// Result of atomically validating and restaging one deferred presentation.
pub enum UiDeferredPresentationRetry {
    Prepared(Box<UiPreparedPresentationAuthority>),
    Superseded,
}

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
    #[error("UI surface {surface} is already installed")]
    SurfaceAlreadyInstalled { surface: u64 },
    #[error("UI surface {surface} has no installed presentation authority")]
    PresentationAuthorityMissing { surface: u64 },
    #[error("UI surface limit was exceeded")]
    SurfaceLimitExceeded,
    #[error("UI surface identity lifetime space was exhausted")]
    SurfaceIdentitySpaceExhausted,
    #[error("UI presentation target window {window} is already owned by surface {surface}")]
    PresentationTargetConflict { window: u64, surface: u64 },
    #[error("UI surface seat {actual} does not match endpoint seat {expected}")]
    SeatOwnerMismatch { expected: u32, actual: u32 },
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
    SurfaceSlot(#[from] UiSurfaceSlotError),
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
            state.discard_all_candidates();
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
            state.discard_all_candidates();
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
            state.discard_all_candidates();
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
                surface.discard_candidate();
                continue;
            };
            let environment = match surface.slot().environment(theme.revision().get()) {
                Ok(environment) => environment,
                Err(error) => {
                    push_diagnostic(
                        &mut diagnostics,
                        "surface-environment-rejected",
                        &error.to_string(),
                    );
                    surface.discard_candidate();
                    continue;
                }
            };
            match surface.prepare_candidate(projected, environment, theme.kernel()) {
                Ok(None) => {}
                Ok(Some(staging)) => {
                    let Some(projected) = projection::project_authoring(
                        world,
                        &ordered_entities,
                        source_revision,
                        staging.reducers(),
                        &mut diagnostics,
                    ) else {
                        surface.discard_candidate();
                        continue;
                    };
                    if let Err(error) =
                        surface.finish_candidate(staging, projected, environment, theme.kernel())
                    {
                        surface.discard_candidate();
                        push_diagnostic(
                            &mut diagnostics,
                            "candidate-reprojection-rejected",
                            &error.to_string(),
                        );
                    }
                }
                Err(error) => {
                    surface.discard_candidate();
                    push_diagnostic(
                        &mut diagnostics,
                        "candidate-preparation-rejected",
                        &error.to_string(),
                    );
                }
            }
        }
        world
            .resource_mut::<UiAdapterDiagnostics>()
            .replace(diagnostics);
    });
}

/// Installs one new Host-owned surface lifetime without reusing retired identities.
///
/// The Host supplies the initial immutable slot while the runtime remains the only successor-
/// generation allocator.
pub fn install_ui_surface(world: &mut World, slot: UiSurfaceSlot) -> Result<(), UiAdapterError> {
    let theme_revision = active_theme_revision(world);
    slot.environment(theme_revision)?;
    world.resource_scope(|_world, mut state: Mut<UiRuntimeState>| {
        if let Some(surface) = state.conflicting_presentation_target(slot) {
            return Err(UiAdapterError::PresentationTargetConflict {
                window: slot.endpoint().window().get(),
                surface: surface.get(),
            });
        }
        let prepared = state.prepare_surface_install(slot)?;
        state.commit_surface_install(slot, prepared);
        Ok(())
    })
}

/// Atomically replaces one slot's endpoint and exact coordinate transform.
///
/// Endpoint and transform facts cannot be updated independently. Any change advances the slot,
/// route, and presentation generations. A transform change also advances geometry generation.
pub fn update_ui_surface_slot(
    world: &mut World,
    surface_id: UiSurfaceId,
    endpoint: UiSurfaceEndpoint,
    transform: nara_ui_kernel::UiSurfaceContentTransform,
) -> Result<UiSurfaceSlot, UiAdapterError> {
    let theme_revision = active_theme_revision(world);
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let previous = state
            .surface_slot(surface_id)
            .ok_or(UiAdapterError::UnknownSurface {
                surface: surface_id.get(),
            })?;
        let updated = previous.successor(endpoint, transform)?;
        updated.environment(theme_revision)?;
        if updated == previous {
            return Ok(updated);
        }
        if let Some(surface) = state.conflicting_presentation_target(updated) {
            return Err(UiAdapterError::PresentationTargetConflict {
                window: updated.endpoint().window().get(),
                surface: surface.get(),
            });
        }
        let prepared = state.prepare_surface_slot_update(updated)?;
        let previous_endpoint = previous.endpoint();
        world
            .resource_mut::<InputTimeline>()
            .unbind_presented_endpoint(
                previous_endpoint.window(),
                previous_endpoint.seat(),
                prepared.released_input_route(),
            )?;
        state.commit_surface_slot_update(prepared);
        Ok(updated)
    })
}

fn active_theme_revision(world: &World) -> u64 {
    world
        .get_resource::<UiDefaultTheme>()
        .map_or_else(UiTheme::default, |theme| theme.0)
        .revision()
        .get()
}

/// Retires one surface after balancing and clearing exactly the physical endpoint it owns.
pub fn retire_ui_surface(
    world: &mut World,
    surface_id: UiSurfaceId,
) -> Result<Option<UiSurfaceRetirement>, UiAdapterError> {
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let Some(retirement) = state.prepare_surface_retirement(surface_id)? else {
            return Ok(None);
        };
        let endpoint = retirement.endpoint();
        world
            .resource_mut::<InputTimeline>()
            .retire_presented_endpoint(
                endpoint.window(),
                endpoint.seat(),
                retirement.input_route(),
            )?;
        state.commit_surface_retirement(retirement);
        Ok(Some(retirement))
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

/// Returns the active Host-owned Surface Slot snapshots in stable surface order.
///
/// Presentation adapters use this bounded snapshot to mirror explicit window routes without
/// making the renderer-neutral UI authoring crate depend on a presentation backend.
pub fn active_ui_surface_slots(world: &World) -> Result<Vec<UiSurfaceSlot>, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    Ok(state
        .surfaces()
        .map(|(_, surface)| surface.slot())
        .collect())
}

/// Stages every fallible UI and input-route fact before the exact frame reaches a backend.
pub fn prepare_ui_presentation(
    world: &mut World,
    frame: Arc<UiFrame>,
    presentation_epoch: u64,
) -> Result<UiPreparedPresentationAuthority, UiAdapterError> {
    world.resource_scope(|world, state: Mut<UiRuntimeState>| {
        let surface_id = frame.environment().surface_id();
        let (slot, facts, prepared) = {
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
            let prepared = surface.prepare_presentation(presentation_epoch)?;
            (surface.slot(), candidate.presentation_facts(), prepared)
        };
        let input_route = prepare_input_route(world, slot, prepared.receipt())?;
        Ok(UiPreparedPresentationAuthority {
            surface_id,
            facts,
            prepared,
            input_route,
        })
    })
}

/// Releases live route capacity while retaining one exact retryable candidate.
pub fn defer_prepared_ui_presentation(
    world: &mut World,
    authority: UiPreparedPresentationAuthority,
) -> UiDeferredPresentationAuthority {
    let UiPreparedPresentationAuthority {
        surface_id,
        facts,
        prepared,
        input_route,
    } = authority;
    nara_input::__private::abort_presented_route(
        &mut world.resource_mut::<InputTimeline>(),
        input_route,
    );
    UiDeferredPresentationAuthority {
        surface_id,
        facts,
        presentation_epoch: prepared.receipt().presentation_epoch(),
    }
}

/// Restages current session/reducer facts for one exact equivalent deferred packet.
pub fn retry_deferred_ui_presentation(
    world: &mut World,
    authority: UiDeferredPresentationAuthority,
) -> Result<UiDeferredPresentationRetry, UiAdapterError> {
    world.resource_scope(|world, state: Mut<UiRuntimeState>| {
        let surface =
            state
                .surface(authority.surface_id)
                .ok_or(UiAdapterError::UnknownSurface {
                    surface: authority.surface_id.get(),
                })?;
        let slot = surface.slot();
        let Some(prepared) = surface
            .prepare_retained_presentation(&authority.facts, authority.presentation_epoch)?
        else {
            return Ok(UiDeferredPresentationRetry::Superseded);
        };
        let input_route = prepare_input_route(world, slot, prepared.receipt())?;
        Ok(UiDeferredPresentationRetry::Prepared(Box::new(
            UiPreparedPresentationAuthority {
                surface_id: authority.surface_id,
                facts: authority.facts,
                prepared,
                input_route,
            },
        )))
    })
}

/// Terminates deferred authority after a newer candidate is admitted.
pub fn supersede_deferred_ui_presentation(_authority: UiDeferredPresentationAuthority) {}

/// Commits one backend-accepted prepared presentation with no fallible post-submit work.
#[must_use]
pub fn commit_prepared_ui_presentation(
    world: &mut World,
    authority: UiPreparedPresentationAuthority,
) -> UiPresentationReceipt {
    let UiPreparedPresentationAuthority {
        surface_id,
        facts: _,
        mut prepared,
        input_route,
    } = authority;
    world.resource_scope(|world, mut state: Mut<UiRuntimeState>| {
        let input_route = nara_input::__private::commit_presented_route(
            &mut world.resource_mut::<InputTimeline>(),
            input_route,
        );
        prepared.bind_input_route(input_route);
        let receipt = prepared.receipt();
        state
            .surface_mut(surface_id)
            .expect("a prepared UI presentation keeps its Surface Slot installed")
            .commit_presentation(prepared);
        receipt
    })
}

fn prepare_input_route(
    world: &mut World,
    slot: UiSurfaceSlot,
    receipt: UiPresentationReceipt,
) -> Result<nara_input::__private::PreparedInputRoutePublication, UiAdapterError> {
    let epoch = InputPresentationEpoch::new(receipt.presentation_epoch())
        .ok_or(UiJournalError::ZeroPresentationEpoch)?;
    nara_input::__private::prepare_presented_route(
        &mut world.resource_mut::<InputTimeline>(),
        InputPresentedRoute::new(
            slot.endpoint().window(),
            InputSurfaceId::new(receipt.surface_id().get()),
            slot.endpoint().seat(),
            epoch,
            InputSlotGeneration::new(receipt.slot_generation().get())
                .expect("validated receipt slot generation is non-zero"),
            InputRouteGeneration::new(receipt.route_revision().get())
                .expect("validated receipt route generation is non-zero"),
            InputGeometryGeneration::new(receipt.geometry_generation().get())
                .expect("validated receipt geometry generation is non-zero"),
        ),
    )
    .map_err(UiAdapterError::from)
}

/// Aborts one prepared presentation before backend acceptance.
pub fn abort_prepared_ui_presentation(
    world: &mut World,
    authority: UiPreparedPresentationAuthority,
) {
    nara_input::__private::abort_presented_route(
        &mut world.resource_mut::<InputTimeline>(),
        authority.input_route,
    );
}

/// Supersedes one prepared presentation before a newer attempt is admitted.
pub fn supersede_prepared_ui_presentation(
    world: &mut World,
    authority: UiPreparedPresentationAuthority,
) {
    nara_input::__private::supersede_presented_route(
        &mut world.resource_mut::<InputTimeline>(),
        authority.input_route,
    );
}

/// Projects accessibility semantics from the latest submitted frame and interaction state.
pub fn ui_semantic_snapshot(
    world: &World,
    receipt: UiPresentationReceipt,
    seat: UiSeatId,
) -> Result<UiSemanticSnapshot, UiAdapterError> {
    let Some(state) = world.get_resource::<UiRuntimeState>() else {
        return Err(UiAdapterError::RuntimeUnavailable);
    };
    let surface = state
        .surface(receipt.surface_id())
        .ok_or(UiAdapterError::UnknownSurface {
            surface: receipt.surface_id().get(),
        })?;
    let expected_seat = surface.slot().endpoint().seat();
    if expected_seat.get() != seat.get() {
        return Err(UiAdapterError::SeatOwnerMismatch {
            expected: expected_seat.get(),
            actual: seat.get(),
        });
    }
    surface.semantic_snapshot(UiRouteStamp::from_receipt(receipt), seat)
}

#[cfg(test)]
pub(crate) fn presented_receipt(
    world: &World,
    surface_id: UiSurfaceId,
    presentation_epoch: u64,
) -> Option<UiPresentationReceipt> {
    world
        .get_resource::<UiRuntimeState>()?
        .surface(surface_id)?
        .presented_receipt(presentation_epoch)
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
