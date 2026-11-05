use std::sync::Arc;

use nara::render::DisplayRect;
use winit::event::Ime;

use crate::{EditorApplication, EditorPanel, EditorPanelContext, EditorPanelHost};

use super::{
    EditorPanelCapability, EditorPanelImage, EditorPanelImageEndpoint, EditorPanelImageError,
    EditorPanelImageHost, EditorPanelInput, EditorPanelInputKind, EditorPanelInputOutcome,
    EditorPanelPlatformEvent, EditorPanelServices, EditorPanelTextInputScope,
    EditorPanelWindowGeometry, input::same_endpoint,
};

/// One original accepted image and native mapping, independent of a provider's control vocabulary.
///
/// Only the original Host can issue this observation. Clones retain immutable output/snapshot
/// resources, not the panel state. A pending candidate grants no observation. Provider-specific
/// keys, roles, values, actions and interaction state remain in their original snapshot/owner.
pub struct EditorPanelImageObservation<S> {
    owner: Arc<()>,
    lifetime: Arc<()>,
    image: EditorPanelImage<S>,
    geometry: EditorPanelWindowGeometry,
    display_frame: u64,
    keyboard: bool,
    text_scope: Option<EditorPanelTextInputScope>,
}

impl<S> Clone for EditorPanelImageObservation<S> {
    fn clone(&self) -> Self {
        Self {
            owner: Arc::clone(&self.owner),
            lifetime: Arc::clone(&self.lifetime),
            image: self.image.clone(),
            geometry: self.geometry,
            display_frame: self.display_frame,
            keyboard: self.keyboard,
            text_scope: self.text_scope.clone(),
        }
    }
}

impl<S> EditorPanelImageObservation<S> {
    #[must_use]
    pub fn image(&self) -> &EditorPanelImage<S> {
        &self.image
    }

    #[must_use]
    pub fn endpoint(&self) -> EditorPanelImageEndpoint {
        self.image.endpoint()
    }

    #[must_use]
    pub fn snapshot(&self) -> &Arc<S> {
        self.image.snapshot()
    }

    #[must_use]
    pub const fn geometry(&self) -> EditorPanelWindowGeometry {
        self.geometry
    }

    /// Last complete window presentation when observed; unchanged panels may survive later ones.
    #[must_use]
    pub const fn display_frame_index(&self) -> u64 {
        self.display_frame
    }

    #[must_use]
    pub const fn owns_keyboard(&self) -> bool {
        self.keyboard
    }

    #[must_use]
    pub fn text_scope(&self) -> Option<&EditorPanelTextInputScope> {
        self.text_scope.as_ref()
    }

    /// Maps target-pixel bounds to visible logical-window bounds using the original image/clip.
    /// This is geometric visibility, not pointer occlusion or semantic activation permission.
    #[must_use]
    pub fn window_bounds(&self, target: DisplayRect) -> Option<DisplayRect> {
        if !target.is_valid() {
            return None;
        }
        let mapping = self.image.mapping();
        let extent = mapping.target();
        let min = mapping.uv_to_panel([
            target.min[0].max(0.0) / extent.width as f32,
            target.min[1].max(0.0) / extent.height as f32,
        ])?;
        let max = mapping.uv_to_panel([
            target.max[0].min(extent.width as f32) / extent.width as f32,
            target.max[1].min(extent.height as f32) / extent.height as f32,
        ])?;
        let clip = self.image.clip();
        let rect = DisplayRect {
            min: std::array::from_fn(|axis| min[axis].max(clip.min[axis])),
            max: std::array::from_fn(|axis| max[axis].min(clip.max[axis])),
        };
        (rect.is_valid() && (0..2).all(|axis| rect.min[axis] < rect.max[axis])).then_some(rect)
    }
}

impl<S> EditorPanelImageHost<S> {
    /// Captures accepted output in paint order without staging or superseding semantic work.
    /// Reconciliation may withdraw obsolete native routes; flush their cancellation normally.
    pub fn observe(
        &mut self,
        application: &EditorApplication,
    ) -> Result<Vec<EditorPanelImageObservation<S>>, EditorPanelImageError> {
        if self.closed {
            return Err(EditorPanelImageError::Closed);
        }
        self.reconcile(application);
        let Some(display_frame) = self.last_display_frame else {
            return Ok(Vec::new());
        };
        Ok(self
            .accepted
            .iter()
            .map(|image| {
                let keyboard = self
                    .keyboard
                    .as_ref()
                    .is_some_and(|owner| same_endpoint(owner, image));
                EditorPanelImageObservation {
                    owner: Arc::clone(&self.owner),
                    lifetime: Arc::clone(
                        self.attachments
                            .iter()
                            .find(|attachment| attachment.token == image.token)
                            .and_then(|attachment| attachment.observation_lifetime.as_ref())
                            .expect("an accepted image has its continuous observation lifetime"),
                    ),
                    image: image.clone(),
                    geometry: self.geometry,
                    display_frame,
                    keyboard,
                    text_scope: keyboard.then(|| self.ime.scope.clone()).flatten(),
                }
            })
            .collect())
    }

    /// Admits platform input only to this exact observed image and original input arbitration.
    ///
    /// Pointer events retain occlusion/capture checks; keyboard and text require the observed
    /// keyboard endpoint and text lifetime. Refusal does not drain cancellation or supersede a
    /// semantic candidate. Window lifecycle events must use ordinary [`Self::dispatch`].
    pub fn dispatch_observed<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        observation: &EditorPanelImageObservation<S>,
        event: EditorPanelPlatformEvent,
        callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> Result<EditorPanelInputOutcome, EditorPanelImageError> {
        self.check_semantic(semantic)?;
        self.check_observation(application, observation)?;
        event.validate()?;
        use EditorPanelPlatformEvent as E;
        match &event {
            E::PointerMoved { physical_position } => {
                let logical = self
                    .geometry
                    .logical_position(*physical_position)
                    .ok_or(EditorPanelImageError::InvalidEvent)?;
                let target = self.pointer_target_at(logical);
                self.check_pointer_target(
                    observation,
                    target.as_ref(),
                    EditorPanelCapability::Pointer,
                )?;
            }
            E::PointerButton { .. } | E::Wheel { .. } | E::PointerLeft => {
                let target = match &event {
                    E::PointerButton { .. } => {
                        self.pointer_target().or_else(|| self.modal().cloned())
                    }
                    _ => self.pointer_target(),
                };
                let capability = if matches!(event, E::Wheel { .. }) {
                    EditorPanelCapability::Wheel
                } else {
                    EditorPanelCapability::Pointer
                };
                self.check_pointer_target(observation, target.as_ref(), capability)?;
            }
            E::Keyboard { .. } | E::ModifiersChanged(_) | E::Text(_) | E::Ime(_) => {
                self.check_keyboard(application, observation)?;
                if !observation.keyboard
                    || self
                        .keyboard
                        .as_ref()
                        .is_none_or(|image| !same_endpoint(image, &observation.image))
                    || observation.text_scope != self.ime.scope
                {
                    return Err(EditorPanelImageError::ChangedInputScope);
                }
                if matches!(event, E::Text(_) | E::Ime(_)) {
                    self.require(&observation.image, EditorPanelCapability::TextInput)?;
                    if observation.text_scope.is_none() && !matches!(event, E::Ime(Ime::Disabled)) {
                        return Err(EditorPanelImageError::ChangedInputScope);
                    }
                    if matches!(event, E::Ime(Ime::Preedit(..) | Ime::Commit(_))) && !self.ime.ready
                    {
                        return Err(EditorPanelImageError::ChangedInputScope);
                    }
                }
            }
            _ => return Err(EditorPanelImageError::UnsupportedObservedEvent),
        }
        self.dispatch(application, semantic, Some(event), callback)
    }

    /// Admits one typed provider request captured by the callback against the original snapshot.
    ///
    /// Only the final `ObservedOperation` delivery executes that request; earlier cancellations
    /// use the same normal reducer. Providers must resolve their original control/action identity,
    /// enabled/modal state and current draft before lowering domain work. A successful Host
    /// admission is not provider acceptance, a source result or a later presentation.
    pub fn invoke_observed<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        observation: &EditorPanelImageObservation<S>,
        callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> Result<EditorPanelInputOutcome, EditorPanelImageError> {
        self.check_semantic(semantic)?;
        self.check_observation(application, observation)?;
        self.check_keyboard(application, observation)?;
        self.require(
            &observation.image,
            EditorPanelCapability::ObservedOperations,
        )?;
        self.claim_keyboard(Some(&observation.image));
        let delivery = self.delivery(
            observation.image.clone(),
            EditorPanelInputKind::ObservedOperation,
            None,
        );
        Ok(self.deliver_input(application, semantic, vec![delivery], true, callback))
    }

    fn check_observation(
        &mut self,
        application: &EditorApplication,
        observation: &EditorPanelImageObservation<S>,
    ) -> Result<(), EditorPanelImageError> {
        if !Arc::ptr_eq(&self.owner, &observation.owner) {
            return Err(EditorPanelImageError::ForeignObservation);
        }
        if self.closed {
            return Err(EditorPanelImageError::Closed);
        }
        self.reconcile(application);
        if self.geometry != observation.geometry
            || !self.attachments.iter().any(|attachment| {
                attachment.token == observation.image.token
                    && attachment
                        .observation_lifetime
                        .as_ref()
                        .is_some_and(|lifetime| Arc::ptr_eq(lifetime, &observation.lifetime))
            })
            || !self.is_accepted(&observation.image)
            || !self
                .accepted
                .iter()
                .any(|image| image.compatible(&observation.image))
        {
            return Err(EditorPanelImageError::StaleObservation);
        }
        Ok(())
    }

    fn check_keyboard(
        &self,
        application: &EditorApplication,
        observation: &EditorPanelImageObservation<S>,
    ) -> Result<(), EditorPanelImageError> {
        if application.ordinary_input_locked()
            && self
                .current_workspace_decision(application)
                .is_none_or(|image| !same_endpoint(image, &observation.image))
        {
            return Err(EditorPanelImageError::InputLocked);
        }
        if !self.focused {
            return Err(EditorPanelImageError::WindowNotFocused);
        }
        if self
            .modal()
            .is_some_and(|image| !same_endpoint(image, &observation.image))
        {
            return Err(EditorPanelImageError::ModalBlocked);
        }
        self.require(&observation.image, EditorPanelCapability::Keyboard)
    }

    fn check_pointer_target(
        &self,
        observation: &EditorPanelImageObservation<S>,
        target: Option<&EditorPanelImage<S>>,
        capability: EditorPanelCapability,
    ) -> Result<(), EditorPanelImageError> {
        if target.is_none_or(|image| !same_endpoint(image, &observation.image)) {
            return Err(EditorPanelImageError::PointerBlocked);
        }
        self.require(&observation.image, capability)
    }

    fn require(
        &self,
        image: &EditorPanelImage<S>,
        capability: EditorPanelCapability,
    ) -> Result<(), EditorPanelImageError> {
        if self.has(image, capability) {
            Ok(())
        } else {
            Err(EditorPanelImageError::MissingCapability(capability))
        }
    }
}
