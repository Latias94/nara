use std::{collections::BTreeMap, sync::Arc};

use nara_ecs::Resource;
use nara_text::FontFace;
use nara_ui_kernel::{
    UiCompiler, UiEnvironment, UiFrame, UiInteractionSession, UiKey, UiLimits, UiRouteStamp,
    UiSeatId, UiSemanticSnapshot, UiTheme,
};
use nara_ui_widgets::UiButtonState;

use super::{
    UiAdapterError,
    candidate::UiCompiledCandidate,
    identity::UiSurfaceIdentityLedger,
    presentation::{UiPresentedHistory, UiPresentedSession},
    projection::UiProjectedAuthoring,
    surface::UiSurfaceDescriptor,
};

pub(super) const MAX_RUNTIME_SURFACES: usize = 16;

/// All candidate, presentation, interaction, and reducer state for one surface lineage.
pub(crate) struct UiSurfaceRuntime {
    descriptor: UiSurfaceDescriptor,
    compiler: UiCompiler,
    candidate: Option<UiCompiledCandidate>,
    presented: UiPresentedHistory,
    interaction: UiInteractionSession,
    reducers: BTreeMap<UiKey, UiButtonState>,
}

impl UiSurfaceRuntime {
    pub(super) fn new(
        descriptor: UiSurfaceDescriptor,
        font: FontFace,
        limits: UiLimits,
    ) -> Result<Self, String> {
        Ok(Self {
            descriptor,
            compiler: UiCompiler::new(font, limits).map_err(|error| error.to_string())?,
            candidate: None,
            presented: UiPresentedHistory::with_limits(limits)
                .map_err(|error| error.to_string())?,
            interaction: UiInteractionSession::with_limits(limits)
                .map_err(|error| error.to_string())?,
            reducers: BTreeMap::new(),
        })
    }

    pub(crate) const fn descriptor(&self) -> UiSurfaceDescriptor {
        self.descriptor
    }

    pub(super) fn replace_descriptor(&mut self, descriptor: UiSurfaceDescriptor) {
        if self.descriptor != descriptor {
            self.candidate = None;
            self.descriptor = descriptor;
        }
    }

    pub(crate) fn reducer_states(&self) -> &BTreeMap<UiKey, UiButtonState> {
        &self.reducers
    }

    pub(super) fn publish_candidate(
        &mut self,
        projected: UiProjectedAuthoring,
        environment: UiEnvironment,
        theme: UiTheme,
    ) -> Result<(), nara_ui_kernel::UiCompileError> {
        let (snapshot, buttons) = projected.into_parts();
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        self.candidate = Some(UiCompiledCandidate::new(frame, buttons));
        Ok(())
    }

    pub(super) fn candidate(&self) -> Option<&UiCompiledCandidate> {
        self.candidate.as_ref()
    }

    pub(super) fn prepare_presentation(
        &self,
        presentation_epoch: u64,
    ) -> Result<(UiPresentedHistory, nara_ui_kernel::UiPresentationReceipt), UiAdapterError> {
        let candidate = self
            .candidate
            .as_ref()
            .ok_or(UiAdapterError::FrameNotLatest {
                surface: self.descriptor.surface_id().get(),
            })?;
        let mut presented = self.presented.clone();
        let receipt = presented.submit(candidate, self.descriptor, presentation_epoch)?;
        Ok((presented, receipt))
    }

    pub(super) fn commit_presentation(&mut self, presented: UiPresentedHistory) {
        self.presented = presented;
        let retained = self.presented.retained_button_keys();
        self.reducers.retain(|key, _| retained.contains(key));
    }

    pub(crate) fn resolve_presented(
        &self,
        route: UiRouteStamp,
    ) -> Result<UiPresentedSession, UiAdapterError> {
        self.presented
            .resolve(route)?
            .ok_or(UiAdapterError::PresentedBehaviorMissing {
                surface: route.surface_id().get(),
                presentation_epoch: route.presentation_epoch(),
            })
    }

    pub(crate) const fn interaction(&self) -> &UiInteractionSession {
        &self.interaction
    }

    pub(crate) fn commit_interaction(
        &mut self,
        interaction: UiInteractionSession,
        reducers: BTreeMap<UiKey, UiButtonState>,
    ) {
        self.interaction = interaction;
        self.reducers = reducers;
    }

    pub(super) fn semantic_snapshot(
        &self,
        route: UiRouteStamp,
        seat: UiSeatId,
    ) -> Result<UiSemanticSnapshot, UiAdapterError> {
        self.interaction
            .semantic_snapshot(self.presented.journal(), route, seat)
            .map_err(UiAdapterError::from)
    }
}

/// Runtime UI adapter state; all mutable surface-local values live inside `UiSurfaceRuntime`.
#[derive(Resource)]
pub(crate) struct UiRuntimeState {
    font: FontFace,
    limits: UiLimits,
    surfaces: BTreeMap<nara_ui_kernel::UiSurfaceId, UiSurfaceRuntime>,
    identities: UiSurfaceIdentityLedger,
    next_source_revision: u64,
}

impl UiRuntimeState {
    pub(crate) fn new() -> Result<Self, String> {
        Ok(Self {
            font: nara_text::bundled_font_face().map_err(|error| error.to_string())?,
            limits: UiLimits::default(),
            surfaces: BTreeMap::new(),
            identities: UiSurfaceIdentityLedger::default(),
            next_source_revision: 1,
        })
    }

    pub(crate) fn surface_descriptor(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<UiSurfaceDescriptor> {
        self.surfaces
            .get(&surface_id)
            .map(UiSurfaceRuntime::descriptor)
    }

    pub(crate) fn surface(
        &self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<&UiSurfaceRuntime> {
        self.surfaces.get(&surface_id)
    }

    pub(crate) fn surface_mut(
        &mut self,
        surface_id: nara_ui_kernel::UiSurfaceId,
    ) -> Option<&mut UiSurfaceRuntime> {
        self.surfaces.get_mut(&surface_id)
    }

    pub(super) fn surfaces(
        &self,
    ) -> impl Iterator<Item = (&nara_ui_kernel::UiSurfaceId, &UiSurfaceRuntime)> {
        self.surfaces.iter()
    }

    pub(super) fn surfaces_mut(
        &mut self,
    ) -> impl Iterator<Item = (&nara_ui_kernel::UiSurfaceId, &mut UiSurfaceRuntime)> {
        self.surfaces.iter_mut()
    }

    pub(super) const fn source_revision(&self) -> u64 {
        self.next_source_revision
    }

    pub(super) fn advance_source_revision(&mut self) -> Result<(), UiAdapterError> {
        self.next_source_revision = self
            .next_source_revision
            .checked_add(1)
            .ok_or(UiAdapterError::SourceRevisionExhausted)?;
        Ok(())
    }

    pub(super) fn prepare_new_surface(
        &self,
        descriptor: UiSurfaceDescriptor,
    ) -> Result<Option<UiSurfaceRuntime>, UiAdapterError> {
        let surface_id = descriptor.surface_id();
        if self.surfaces.contains_key(&surface_id) {
            return Ok(None);
        }
        self.identities.validate_new(surface_id)?;
        if self.surfaces.len() == MAX_RUNTIME_SURFACES {
            return Err(UiAdapterError::SurfaceLimitExceeded);
        }
        UiSurfaceRuntime::new(descriptor, self.font.clone(), self.limits)
            .map(Some)
            .map_err(UiAdapterError::RuntimeSetup)
    }

    pub(super) fn conflicting_input_endpoint(
        &self,
        descriptor: UiSurfaceDescriptor,
    ) -> Option<nara_ui_kernel::UiSurfaceId> {
        self.surfaces.iter().find_map(|(surface_id, surface)| {
            (*surface_id != descriptor.surface_id()
                && surface.descriptor().input_window() == descriptor.input_window()
                && surface.descriptor().input_seat() == descriptor.input_seat())
            .then_some(*surface_id)
        })
    }

    pub(super) fn commit_surface_install(
        &mut self,
        descriptor: UiSurfaceDescriptor,
        prepared: Option<UiSurfaceRuntime>,
    ) {
        match prepared {
            Some(surface) => {
                self.surfaces.insert(descriptor.surface_id(), surface);
            }
            None => self
                .surfaces
                .get_mut(&descriptor.surface_id())
                .expect("validated UI surface remains installed")
                .replace_descriptor(descriptor),
        }
    }

    pub(super) fn retire_surface(&mut self, surface_id: nara_ui_kernel::UiSurfaceId) -> bool {
        let removed = self.surfaces.remove(&surface_id).is_some();
        if removed {
            self.identities.retire(surface_id);
        }
        removed
    }

    pub(crate) fn is_retired(&self, surface_id: nara_ui_kernel::UiSurfaceId) -> bool {
        self.identities.is_retired(surface_id)
    }
}

pub(super) fn candidate_frame(candidate: &UiCompiledCandidate) -> Arc<UiFrame> {
    Arc::clone(candidate.frame())
}
