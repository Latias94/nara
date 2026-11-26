use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_ecs::{Entity, Mut, Resource, World};
use nara_input::{InputSeatId, InputWindowId};
use thiserror::Error;

use crate::interaction::SurfaceInteraction;
use crate::kernel::{
    UiAuthoringSnapshot, UiCompileError, UiCompiler, UiEnvironmentError, UiFrame, UiKey, UiLimits,
    UiPoint, UiPresentationReceipt, UiSurfaceContentTransform, UiSurfaceId,
};
use crate::{FontFace, UiActionId, UiRoot, UiTheme};

mod projection;
mod surface;
#[cfg(test)]
mod tests;

pub use surface::{
    UiSurfaceAvailability, UiSurfaceEndpoint, UiSurfaceRetirement, UiSurfaceSlot,
    UiSurfaceSlotError, UiSurfaceSlotSnapshot, UiSurfaceSuspension, UiSurfaceSuspensionReason,
};

#[derive(Debug, Default, Resource)]
pub struct UiAdapterDiagnostics(Vec<String>);

impl UiAdapterDiagnostics {
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

#[derive(Debug, Error)]
pub enum UiAdapterError {
    #[error("UI runtime adapter is not installed")]
    RuntimeUnavailable,
    #[error("UI surface {0:?} is unknown")]
    UnknownSurface(UiSurfaceId),
    #[error("UI surface {0:?} is already installed")]
    SurfaceAlreadyInstalled(UiSurfaceId),
    #[error("UI surface {0:?} was retired and cannot be reinstalled")]
    RetiredSurface(UiSurfaceId),
    #[error("UI surface {0:?} is suspended")]
    SurfaceSuspended(UiSurfaceId),
    #[error("UI surface {0:?} is already active")]
    SurfaceAlreadyActive(UiSurfaceId),
    #[error("UI surface capacity was exceeded")]
    SurfaceLimitExceeded,
    #[error("an installed UI surface already owns window {0:?}")]
    WindowAlreadyOwned(InputWindowId),
    #[error("compiled UI frame does not belong to the active surface generation")]
    FrameSuperseded,
    #[error("presentation epoch or frame revision predates the last accepted frame")]
    StalePresentation,
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
    #[error(transparent)]
    SurfaceSlot(#[from] UiSurfaceSlotError),
    #[error(transparent)]
    Compile(#[from] UiCompileError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiButtonBinding {
    pub(crate) entity: Entity,
    pub(crate) action: UiActionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiTarget {
    pub(crate) key: UiKey,
    pub(crate) binding: UiButtonBinding,
}

#[derive(Debug, PartialEq)]
pub(crate) struct UiFrameBindings {
    producer: Arc<()>,
    pub(crate) buttons: BTreeMap<UiKey, UiButtonBinding>,
}

struct Candidate {
    source: UiAuthoringSnapshot,
    theme: UiTheme,
    frame: Arc<UiFrame>,
}

pub(crate) struct Surface {
    pub(crate) slot: UiSurfaceSlot,
    availability: UiSurfaceAvailability,
    producer: Arc<()>,
    compiler: UiCompiler,
    candidate: Option<Candidate>,
    pub(crate) presented: Option<(UiPresentationReceipt, Arc<UiFrame>)>,
    pub(crate) interaction: SurfaceInteraction,
}

impl Surface {
    fn set_slot(&mut self, slot: UiSurfaceSlot, availability: UiSurfaceAvailability) {
        self.slot = slot;
        self.availability = availability;
        self.candidate = None;
        self.presented = None;
        self.interaction = SurfaceInteraction::default();
    }

    fn compile(
        &mut self,
        world: &World,
        roots: &[(Entity, UiRoot)],
        revision: u64,
        theme: UiTheme,
    ) -> Result<(), String> {
        let (source, buttons) = projection::project(world, roots, revision, &self.interaction)
            .map_err(|error| error.to_string())?;
        let environment = self
            .slot
            .environment(theme.revision().get())
            .map_err(|error| error.to_string())?;
        if self.candidate.as_ref().is_some_and(|current| {
            current.theme == theme
                && current.source.same_tree(&source)
                && current
                    .frame
                    .bindings
                    .as_ref()
                    .is_some_and(|bindings| bindings.buttons == buttons)
        }) {
            return Ok(());
        }
        let mut frame = self
            .compiler
            .compile_frame(&source, environment, theme.kernel())
            .map_err(|error| error.to_string())?;
        frame.bindings = Some(UiFrameBindings {
            producer: Arc::clone(&self.producer),
            buttons,
        });
        self.candidate = Some(Candidate {
            source,
            theme,
            frame: Arc::new(frame),
        });
        Ok(())
    }

    pub(crate) fn authored_pointer(&self, logical: [f32; 2]) -> Option<UiPoint> {
        self.slot
            .transform()
            .platform_logical_to_authored(UiPoint::new(logical[0], logical[1]))
    }
}

#[derive(Resource)]
pub(crate) struct UiRuntimeState {
    font: FontFace,
    surfaces: BTreeMap<UiSurfaceId, Surface>,
    retired: BTreeSet<UiSurfaceId>,
    source_revision: u64,
    pub(crate) action_sequence: u64,
}

impl UiRuntimeState {
    pub(crate) fn new(font: FontFace) -> Self {
        Self {
            font,
            surfaces: BTreeMap::new(),
            retired: BTreeSet::new(),
            source_revision: 0,
            action_sequence: 0,
        }
    }

    fn ensure_window_available(
        &self,
        id: UiSurfaceId,
        endpoint: UiSurfaceEndpoint,
    ) -> Result<(), UiAdapterError> {
        if self.surfaces.values().any(|surface| {
            surface.slot.surface_id() != id && surface.slot.endpoint().window() == endpoint.window()
        }) {
            return Err(UiAdapterError::WindowAlreadyOwned(endpoint.window()));
        }
        Ok(())
    }

    fn surface_mut(&mut self, id: UiSurfaceId) -> Result<&mut Surface, UiAdapterError> {
        if self.retired.contains(&id) {
            return Err(UiAdapterError::RetiredSurface(id));
        }
        self.surfaces
            .get_mut(&id)
            .ok_or(UiAdapterError::UnknownSurface(id))
    }

    pub(crate) fn input_surface(
        &mut self,
        window: InputWindowId,
        seat: Option<InputSeatId>,
    ) -> Option<&mut Surface> {
        self.surfaces.values_mut().find(|surface| {
            let endpoint = surface.slot.endpoint();
            surface.availability.is_active()
                && endpoint.window() == window
                && seat.is_none_or(|seat| endpoint.seat() == seat)
        })
    }
}

pub(crate) fn compile_ui_frames(world: &mut World) {
    world.resource_scope(|world, mut runtime: Mut<UiRuntimeState>| {
        if !runtime
            .surfaces
            .values()
            .any(|surface| surface.availability.is_active())
        {
            world.resource_mut::<UiAdapterDiagnostics>().0.clear();
            return;
        }
        runtime.source_revision = runtime.source_revision.saturating_add(1);
        let revision = runtime.source_revision;
        let mut roots: Vec<_> = world
            .query::<(Entity, &UiRoot)>()
            .iter(world)
            .take(UiLimits::default().max_roots() + 1)
            .map(|(entity, root)| (entity, *root))
            .collect();
        roots.sort_unstable_by_key(|(entity, _)| entity.index());
        let theme = world
            .get_resource::<UiTheme>()
            .copied()
            .or_else(|| {
                roots
                    .iter()
                    .find_map(|(entity, _)| world.get::<UiTheme>(*entity).copied())
            })
            .unwrap_or_default();
        let mut diagnostics = Vec::new();
        for (id, surface) in &mut runtime.surfaces {
            if surface.availability.is_active() {
                if roots.len() > UiLimits::default().max_roots() {
                    surface.candidate = None;
                    diagnostics.push(format!("UI surface {}: root capacity exceeded", id.get()));
                } else if let Err(error) = surface.compile(world, &roots, revision, theme) {
                    surface.candidate = None;
                    diagnostics.push(format!("UI surface {}: {error}", id.get()));
                }
            }
        }
        world.resource_mut::<UiAdapterDiagnostics>().0 = diagnostics;
    });
}

pub fn install_ui_surface(world: &mut World, slot: UiSurfaceSlot) -> Result<(), UiAdapterError> {
    let mut runtime = state_mut(world)?;
    let id = slot.surface_id();
    if runtime.retired.contains(&id) {
        return Err(UiAdapterError::RetiredSurface(id));
    }
    if runtime.surfaces.contains_key(&id) {
        return Err(UiAdapterError::SurfaceAlreadyInstalled(id));
    }
    let limits = UiLimits::default();
    if runtime.surfaces.len() >= limits.max_surfaces
        || runtime.surfaces.len() + runtime.retired.len() >= limits.max_surface_lifetimes
    {
        return Err(UiAdapterError::SurfaceLimitExceeded);
    }
    runtime.ensure_window_available(id, slot.endpoint())?;
    let compiler = UiCompiler::new(runtime.font.clone(), limits)?;
    runtime.surfaces.insert(
        id,
        Surface {
            slot,
            availability: UiSurfaceAvailability::Active,
            producer: Arc::new(()),
            compiler,
            candidate: None,
            presented: None,
            interaction: SurfaceInteraction::default(),
        },
    );
    Ok(())
}

pub fn update_ui_surface_slot(
    world: &mut World,
    id: UiSurfaceId,
    endpoint: UiSurfaceEndpoint,
    transform: UiSurfaceContentTransform,
) -> Result<UiSurfaceSlot, UiAdapterError> {
    let mut runtime = state_mut(world)?;
    runtime.ensure_window_available(id, endpoint)?;
    let surface = runtime.surface_mut(id)?;
    if !surface.availability.is_active() {
        return Err(UiAdapterError::SurfaceSuspended(id));
    }
    if surface.slot.endpoint() != endpoint || surface.slot.transform() != transform {
        surface.set_slot(
            surface.slot.advance(endpoint, transform)?,
            UiSurfaceAvailability::Active,
        );
    }
    Ok(surface.slot)
}

pub fn suspend_ui_surface(
    world: &mut World,
    id: UiSurfaceId,
    reason: UiSurfaceSuspensionReason,
) -> Result<Option<UiSurfaceSuspension>, UiAdapterError> {
    let mut runtime = state_mut(world)?;
    let surface = runtime.surface_mut(id)?;
    if !surface.availability.is_active() {
        return Ok(None);
    }
    let previous = surface.slot;
    let slot = previous.advance(previous.endpoint(), previous.transform())?;
    let availability = UiSurfaceAvailability::Suspended(reason);
    surface.set_slot(slot, availability);
    Ok(Some(UiSurfaceSuspension {
        previous_slot_generation: previous.slot_generation(),
        snapshot: UiSurfaceSlotSnapshot { slot, availability },
    }))
}

pub fn restore_ui_surface(
    world: &mut World,
    id: UiSurfaceId,
    endpoint: UiSurfaceEndpoint,
    transform: UiSurfaceContentTransform,
) -> Result<UiSurfaceSlot, UiAdapterError> {
    let mut runtime = state_mut(world)?;
    runtime.ensure_window_available(id, endpoint)?;
    let surface = runtime.surface_mut(id)?;
    if surface.availability.is_active() {
        return Err(UiAdapterError::SurfaceAlreadyActive(id));
    }
    let slot = surface.slot.advance(endpoint, transform)?;
    surface.set_slot(slot, UiSurfaceAvailability::Active);
    Ok(slot)
}

pub fn retire_ui_surface(
    world: &mut World,
    id: UiSurfaceId,
) -> Result<Option<UiSurfaceRetirement>, UiAdapterError> {
    let mut runtime = state_mut(world)?;
    let Some(surface) = runtime.surfaces.get(&id) else {
        return Ok(None);
    };
    let slot = surface.slot;
    let retirement_generation = slot.next_generation()?;
    runtime.surfaces.remove(&id);
    runtime.retired.insert(id);
    Ok(Some(UiSurfaceRetirement {
        slot,
        retirement_generation,
    }))
}

pub fn ui_surface_slot_snapshots(
    world: &World,
) -> Result<Vec<UiSurfaceSlotSnapshot>, UiAdapterError> {
    Ok(state(world)?
        .surfaces
        .values()
        .map(|surface| UiSurfaceSlotSnapshot {
            slot: surface.slot,
            availability: surface.availability,
        })
        .collect())
}

pub fn active_ui_surface_slots(world: &World) -> Result<Vec<UiSurfaceSlot>, UiAdapterError> {
    Ok(ui_surface_slot_snapshots(world)?
        .into_iter()
        .filter_map(UiSurfaceSlotSnapshot::active_slot)
        .collect())
}

pub fn ui_candidate_frame(
    world: &World,
    id: UiSurfaceId,
) -> Result<Option<Arc<UiFrame>>, UiAdapterError> {
    Ok(state(world)?
        .surfaces
        .get(&id)
        .and_then(|surface| surface.candidate.as_ref())
        .map(|candidate| Arc::clone(&candidate.frame)))
}

pub fn ui_candidate_frames(world: &World) -> Result<Vec<Arc<UiFrame>>, UiAdapterError> {
    Ok(state(world)?
        .surfaces
        .values()
        .filter_map(|surface| surface.candidate.as_ref())
        .map(|candidate| Arc::clone(&candidate.frame))
        .collect())
}

/// Returns the font used by the runtime's surface compilers.
#[must_use]
pub fn ui_font_face(world: &World) -> Option<&FontFace> {
    Some(&state(world).ok()?.font)
}

/// Returns the single accepted layout used to route this surface's input.
pub fn latest_presented_ui_frame(
    world: &World,
    id: UiSurfaceId,
) -> Option<(UiPresentationReceipt, Arc<UiFrame>)> {
    state(world).ok()?.surfaces.get(&id)?.presented.clone()
}

/// Publishes the exact frame already drawn by the backend. A compiled candidate alone never
/// becomes interactive. Renderer-owned in-flight frames may complete after a newer candidate.
pub fn publish_presented_frame(
    world: &mut World,
    frame: &Arc<UiFrame>,
    epoch: u64,
) -> Result<UiPresentationReceipt, UiAdapterError> {
    let receipt = frame
        .presentation_receipt(epoch)
        .ok_or(UiAdapterError::StalePresentation)?;
    let mut runtime = state_mut(world)?;
    let surface = runtime.surface_mut(receipt.surface_id())?;
    if !surface.availability.is_active()
        || frame.environment().slot_generation().get() != surface.slot.slot_generation()
        || !frame
            .bindings
            .as_ref()
            .is_some_and(|bindings| Arc::ptr_eq(&surface.producer, &bindings.producer))
    {
        return Err(UiAdapterError::FrameSuperseded);
    }
    if surface.presented.as_ref().is_some_and(|(previous, _)| {
        previous.presentation_epoch() >= epoch
            || previous.frame_revision() > receipt.frame_revision()
    }) {
        return Err(UiAdapterError::StalePresentation);
    }
    surface.interaction.reconcile(frame);
    surface.presented = Some((receipt, Arc::clone(frame)));
    Ok(receipt)
}

/// Invalidates hit geometry when the presenter loses or replaces a display target.
/// The compiled candidate remains available for the next presentation.
pub fn clear_presented_ui_frame(world: &mut World, id: UiSurfaceId) {
    if let Ok(mut runtime) = state_mut(world)
        && let Ok(surface) = runtime.surface_mut(id)
    {
        surface.presented = None;
        surface.interaction = SurfaceInteraction::default();
    }
}

fn state(world: &World) -> Result<&UiRuntimeState, UiAdapterError> {
    world
        .get_resource()
        .ok_or(UiAdapterError::RuntimeUnavailable)
}

fn state_mut(world: &mut World) -> Result<Mut<'_, UiRuntimeState>, UiAdapterError> {
    world
        .get_resource_mut()
        .ok_or(UiAdapterError::RuntimeUnavailable)
}
