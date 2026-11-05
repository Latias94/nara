use std::sync::Arc;

use winit::{
    event::{ElementState, Ime},
    keyboard::ModifiersState,
};

use crate::{
    EditorApplication,
    panels::{EditorPanel, EditorPanelContext, EditorPanelHost, EditorPanelInstanceToken},
};

use super::{
    Delivery, EditorPanelCancelReason, EditorPanelCapability, EditorPanelImage,
    EditorPanelImageError, EditorPanelImageHost, EditorPanelInput, EditorPanelInputKind,
    EditorPanelInputOutcome, EditorPanelPlatformEvent, EditorPanelServices,
    EditorPanelWindowGeometry, geometry, services::ServiceRequests,
};

impl<S> EditorPanelImageHost<S> {
    /// Flushes original cancellations, then delivers ordinary input in the same chronological
    /// semantic candidate. `None` still flushes cancellation after presentation/retirement.
    ///
    /// The returned submission is the original `EditorPanelHost` submission: pass it back to that
    /// same Host's `submit_and_drive`. This method owns no dispatcher or source mutation path.
    pub fn dispatch<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        event: Option<EditorPanelPlatformEvent>,
        callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> Result<EditorPanelInputOutcome, EditorPanelImageError> {
        self.check_semantic(semantic)?;
        self.reconcile(application);
        let locked = application.ordinary_input_locked()
            && self.current_workspace_decision(application).is_none();
        if let Some(event) = &event {
            event.validate()?;
        }
        if let Some(EditorPanelPlatformEvent::PointerMoved { physical_position }) = &event {
            if self.geometry.logical_size().is_some()
                && self.geometry.logical_position(*physical_position).is_none()
            {
                return Err(EditorPanelImageError::InvalidEvent);
            }
        }
        let (ordinary, consumed) = match event.as_ref() {
            Some(event) => self.route_event(event, locked),
            None => (Vec::new(), false),
        };
        let outcome = self.deliver_input(application, semantic, ordinary, consumed, callback);
        self.finish_button_release(event.as_ref());
        Ok(outcome)
    }

    /// Transfers keyboard ownership to an original panel's currently accepted image.
    ///
    /// The native window must already be focused. Locked input admits only the first-party shell's
    /// currently accepted exact workspace decision. The target must declare `Keyboard`;
    /// `Pointer` is not required. While a window-modal image is accepted, only that provider can
    /// receive focus. This method does not focus the OS window
    /// or select a local widget.
    ///
    /// The old owner's cancellation precedes `KeyboardFocused` in one original semantic
    /// candidate. The callback chooses local focus and may explicitly request a text-input scope.
    /// Repeated focus on the same endpoint delivers `KeyboardFocused` without cancelling itself.
    /// Pass the returned submission to the same semantic Host's `submit_and_drive`.
    pub fn focus_panel<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        token: &EditorPanelInstanceToken,
        callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> Result<EditorPanelInputOutcome, EditorPanelImageError> {
        self.check_semantic(semantic)?;
        if self.closed {
            return Err(EditorPanelImageError::Closed);
        }
        self.reconcile(application);
        if application.ordinary_input_locked()
            && self
                .current_workspace_decision(application)
                .is_none_or(|image| &image.token != token)
        {
            return Err(EditorPanelImageError::InputLocked);
        }
        if !self.focused {
            return Err(EditorPanelImageError::WindowNotFocused);
        }
        if !token.is_current(application) {
            return Err(EditorPanelImageError::RetiredInstance);
        }
        let capabilities = self
            .capabilities(token)
            .ok_or(EditorPanelImageError::NotAttached)?;
        if !capabilities.has(EditorPanelCapability::Keyboard) {
            return Err(EditorPanelImageError::MissingCapability(
                EditorPanelCapability::Keyboard,
            ));
        }
        let image = self
            .accepted
            .iter()
            .find(|image| &image.token == token)
            .cloned()
            .ok_or(EditorPanelImageError::NotPresented)?;
        if self
            .modal()
            .is_some_and(|modal| !same_endpoint(modal, &image))
        {
            return Err(EditorPanelImageError::ModalBlocked);
        }
        self.claim_keyboard(Some(&image));
        let delivery = self.delivery(image, EditorPanelInputKind::KeyboardFocused, None);
        Ok(self.deliver_input(application, semantic, vec![delivery], true, callback))
    }

    /// Refreshes services for the already-owned accepted keyboard endpoint without claiming focus.
    /// Original cancellations and the refresh share the same semantic candidate.
    pub(crate) fn refresh_keyboard_services<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> Result<EditorPanelInputOutcome, EditorPanelImageError> {
        self.check_semantic(semantic)?;
        self.reconcile(application);
        let image = self
            .keyboard
            .as_ref()
            .filter(|image| {
                self.is_accepted(image) && self.has(image, EditorPanelCapability::Keyboard)
            })
            .cloned();
        let Some(image) = image else {
            return self.dispatch(application, semantic, None, callback);
        };
        let delivery = self.delivery(image, EditorPanelInputKind::KeyboardFocused, None);
        Ok(self.deliver_input(application, semantic, vec![delivery], true, callback))
    }

    fn deliver_input<P: EditorPanel>(
        &mut self,
        application: &EditorApplication,
        semantic: &mut EditorPanelHost<P>,
        mut ordinary: Vec<Delivery<S>>,
        consumed: bool,
        mut callback: impl FnMut(
            &mut P,
            &mut EditorPanelContext<'_, '_>,
            &EditorPanelInput<S>,
            &mut EditorPanelServices<'_>,
        ),
    ) -> EditorPanelInputOutcome {
        let mut deliveries = self.cancellations.drain(..).collect::<Vec<_>>();
        deliveries.append(&mut ordinary);
        deliveries.retain(|delivery| delivery.image.token.is_current(application));
        if deliveries.is_empty() {
            return EditorPanelInputOutcome {
                consumed,
                submission: None,
            };
        }
        let visits = deliveries
            .iter()
            .map(|delivery| delivery.image.token.instance())
            .collect::<Vec<_>>();
        let submission = semantic.prepare_ordered(application, &visits, |index, panel, context| {
            let delivery = &deliveries[index];
            if !context.matches_instance(&delivery.image.token) {
                return;
            }
            if !self.delivery_ready(delivery) {
                return;
            }
            let cancelled = matches!(&delivery.kind, EditorPanelInputKind::Cancelled(_));
            let active = !cancelled && self.is_accepted(&delivery.image);
            let capabilities = self.capabilities(&delivery.image.token).unwrap_or_default();
            let keyboard_owner = self
                .keyboard
                .as_ref()
                .is_some_and(|image| same_endpoint(image, &delivery.image));
            let capture_owner = self
                .capture
                .as_ref()
                .is_some_and(|capture| same_endpoint(&capture.image, &delivery.image));
            let pointer_owner = capture_owner
                || self
                    .hovered
                    .as_ref()
                    .is_some_and(|image| same_endpoint(image, &delivery.image));
            let pressed = match &delivery.kind {
                EditorPanelInputKind::PointerButton {
                    button,
                    state: ElementState::Pressed,
                } => Some(*button),
                _ => None,
            };
            let mut services = EditorPanelServices {
                clipboard: &self.clipboard,
                capabilities,
                active,
                keyboard_owner,
                pointer_owner,
                capture_owner,
                captured_button: self.capture.as_ref().map(|capture| capture.button),
                pressed,
                ime_ready: self.ime.ready,
                mapping: delivery.image.mapping,
                clip: delivery.image.clip,
                current_text_scope: self.ime.scope.clone(),
                delivery_text_scope: delivery.text_scope.clone(),
                requests: ServiceRequests::default(),
            };
            let input = EditorPanelInput {
                endpoint: delivery.image.endpoint(),
                snapshot: Arc::clone(&delivery.image.snapshot),
                kind: delivery.kind.clone(),
                modifiers: delivery.modifiers,
                target_position: delivery.position,
                text_scope: delivery.text_scope.clone(),
            };
            callback(panel, context, &input, &mut services);
            let requests = services.requests;
            if active {
                self.apply_services(&delivery.image, requests);
            }
        });
        EditorPanelInputOutcome {
            consumed,
            submission,
        }
    }

    fn route_event(
        &mut self,
        event: &EditorPanelPlatformEvent,
        locked: bool,
    ) -> (Vec<Delivery<S>>, bool) {
        use EditorPanelPlatformEvent as E;
        match event {
            E::Focused(focused) => {
                self.focused = *focused;
                if !focused {
                    self.withdraw_all(EditorPanelCancelReason::FocusLost);
                    self.modifiers = ModifiersState::empty();
                    self.pointer = None;
                    self.pointer_inside = false;
                }
                return (Vec::new(), false);
            }
            E::Resized(_) | E::ScaleFactorChanged(_) => {
                self.withdraw_all(EditorPanelCancelReason::WindowGeometryChanged);
                self.geometry = EditorPanelWindowGeometry::read(&self.window);
                self.pointer = None;
                self.pointer_inside = false;
                return (Vec::new(), false);
            }
            E::CloseRequested => {
                self.withdraw_all(EditorPanelCancelReason::WindowClosed);
                return (Vec::new(), false);
            }
            E::Destroyed => {
                self.close();
                return (Vec::new(), false);
            }
            E::Ime(Ime::Enabled) => {
                if self.ime.allowed && self.ime.scope.is_some() && !self.ime.waiting_disabled {
                    self.ime.ready = true;
                    self.publish_ime_area();
                    return self.ime_delivery(EditorPanelInputKind::ImeEnabled, locked, None);
                }
                return (Vec::new(), locked || self.modal().is_some());
            }
            E::Ime(Ime::Disabled) => {
                let retiring = self.ime.retiring_scope.take();
                self.ime.allowed = false;
                self.ime.ready = false;
                self.ime.waiting_disabled = false;
                self.enable_requested_ime();
                return self.ime_delivery(EditorPanelInputKind::ImeDisabled, locked, retiring);
            }
            _ => {}
        }
        if locked {
            return (Vec::new(), true);
        }
        if self.closed || !self.focused {
            return (Vec::new(), false);
        }
        let modal = self.modal().is_some();
        match event {
            E::PointerMoved { physical_position } => {
                self.pointer = Some(*physical_position);
                self.pointer_inside = true;
                let image = self.pointer_target();
                let mut deliveries = self.update_hover(image.as_ref());
                if let Some(image) = image {
                    if self.has(&image, EditorPanelCapability::Pointer) {
                        let extrapolate = self
                            .capture
                            .as_ref()
                            .is_some_and(|capture| same_endpoint(&capture.image, &image));
                        if let Some(position) = self.position_for(&image, extrapolate) {
                            deliveries.push(self.delivery(
                                image,
                                EditorPanelInputKind::PointerMoved,
                                Some(position),
                            ));
                        }
                    }
                    (deliveries, true)
                } else {
                    (deliveries, modal)
                }
            }
            E::PointerEntered => {
                self.pointer_inside = true;
                (Vec::new(), modal || self.capture.is_some())
            }
            E::PointerLeft => {
                self.pointer_inside = false;
                let deliveries = self.update_hover(None);
                let consumed = modal || self.capture.is_some() || !deliveries.is_empty();
                (deliveries, consumed)
            }
            E::PointerButton { button, state } => {
                let image = self.pointer_target();
                if let Some(image) = image {
                    if !self.has(&image, EditorPanelCapability::Pointer) {
                        return (Vec::new(), true);
                    }
                    if *state == ElementState::Pressed {
                        self.claim_keyboard(Some(&image));
                    }
                    let mut deliveries = self.update_hover(Some(&image));
                    let extrapolate = self
                        .capture
                        .as_ref()
                        .is_some_and(|capture| same_endpoint(&capture.image, &image));
                    if let Some(position) = self.position_for(&image, extrapolate) {
                        deliveries.push(self.delivery(
                            image,
                            EditorPanelInputKind::PointerButton {
                                button: *button,
                                state: *state,
                            },
                            Some(position),
                        ));
                    }
                    (deliveries, true)
                } else if let Some(image) = self.modal().cloned() {
                    if *state == ElementState::Pressed {
                        self.claim_keyboard(Some(&image));
                    }
                    let position = self.position_for(&image, true);
                    let delivery = self.delivery(
                        image,
                        EditorPanelInputKind::ModalOutsidePointer {
                            button: *button,
                            state: *state,
                        },
                        position,
                    );
                    (vec![delivery], true)
                } else {
                    if *state == ElementState::Pressed {
                        self.claim_keyboard(None);
                    }
                    (Vec::new(), false)
                }
            }
            E::Wheel { delta, phase } => {
                if let Some(image) = self.pointer_target() {
                    if self.has(&image, EditorPanelCapability::Wheel) {
                        let extrapolate = self
                            .capture
                            .as_ref()
                            .is_some_and(|capture| same_endpoint(&capture.image, &image));
                        if let Some(position) = self.position_for(&image, extrapolate) {
                            return (
                                vec![self.delivery(
                                    image,
                                    EditorPanelInputKind::Wheel {
                                        delta: *delta,
                                        phase: *phase,
                                    },
                                    Some(position),
                                )],
                                true,
                            );
                        }
                    }
                    (Vec::new(), true)
                } else {
                    (Vec::new(), modal)
                }
            }
            E::Keyboard { key, text } => {
                let Some(image) = self.keyboard_target() else {
                    return (Vec::new(), modal);
                };
                let mut deliveries = vec![self.delivery(
                    image.clone(),
                    EditorPanelInputKind::Key(key.clone()),
                    None,
                )];
                if key.state == ElementState::Pressed
                    && !key.synthetic
                    && self.has(&image, EditorPanelCapability::TextInput)
                {
                    if let Some(text) = text.as_ref().filter(|text| !text.is_empty()) {
                        deliveries.push(self.delivery(
                            image,
                            EditorPanelInputKind::Text(text.clone()),
                            None,
                        ));
                    }
                }
                (deliveries, true)
            }
            E::Text(text) => self.text_delivery(EditorPanelInputKind::Text(text.clone())),
            E::ModifiersChanged(modifiers) => {
                self.modifiers = *modifiers;
                let mut images = Vec::with_capacity(3);
                for image in [
                    self.keyboard.as_ref(),
                    self.capture.as_ref().map(|capture| &capture.image),
                    self.hovered.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    if !images
                        .iter()
                        .any(|previous: &EditorPanelImage<S>| previous.token == image.token)
                    {
                        images.push(image.clone());
                    }
                }
                let deliveries = images
                    .into_iter()
                    .map(|image| {
                        let position = self.position_for(&image, true);
                        self.delivery(image, EditorPanelInputKind::ModifiersChanged, position)
                    })
                    .collect::<Vec<_>>();
                let consumed = modal || !deliveries.is_empty();
                (deliveries, consumed)
            }
            E::Ime(Ime::Preedit(text, cursor)) => {
                self.text_delivery(EditorPanelInputKind::ImePreedit {
                    text: text.clone(),
                    cursor: *cursor,
                })
            }
            E::Ime(Ime::Commit(text)) => {
                self.text_delivery(EditorPanelInputKind::ImeCommit(text.clone()))
            }
            E::Ime(Ime::Enabled | Ime::Disabled)
            | E::Focused(_)
            | E::Resized(_)
            | E::ScaleFactorChanged(_)
            | E::CloseRequested
            | E::Destroyed => (Vec::new(), false),
        }
    }

    pub(super) fn position_for(
        &self,
        image: &EditorPanelImage<S>,
        extrapolate: bool,
    ) -> Option<[f32; 2]> {
        let logical = self.geometry.logical_position(self.pointer?)?;
        if !extrapolate && !image.clip.contains(logical) {
            return None;
        }
        geometry::target_position(image.mapping, logical, extrapolate)
    }

    fn modal(&self) -> Option<&EditorPanelImage<S>> {
        self.accepted.iter().rev().find(|image| image.window_modal)
    }

    fn pointer_target(&self) -> Option<EditorPanelImage<S>> {
        if let Some(capture) = &self.capture {
            return Some(capture.image.clone());
        }
        if !self.pointer_inside {
            return None;
        }
        let logical = self.geometry.logical_position(self.pointer?)?;
        if let Some(modal) = self.modal() {
            return geometry::hit(modal, logical).then(|| modal.clone());
        }
        self.accepted
            .iter()
            .rev()
            .find(|image| geometry::hit(image, logical))
            .cloned()
    }

    fn keyboard_target(&mut self) -> Option<EditorPanelImage<S>> {
        if let Some(modal) = self.modal().cloned() {
            self.claim_keyboard(Some(&modal));
        }
        self.keyboard
            .as_ref()
            .filter(|image| self.has(image, EditorPanelCapability::Keyboard))
            .cloned()
    }

    fn claim_keyboard(&mut self, image: Option<&EditorPanelImage<S>>) {
        let next = image.filter(|image| self.has(image, EditorPanelCapability::Keyboard));
        if self
            .keyboard
            .as_ref()
            .is_some_and(|previous| next.is_none_or(|next| !same_endpoint(previous, next)))
        {
            let previous = self.keyboard.as_ref().map(|image| image.token.clone());
            self.cancel_where(
                |image| previous.as_ref() == Some(&image.token),
                EditorPanelCancelReason::KeyboardTransferred,
            );
        }
        if let Some(next) = next {
            self.keyboard = Some(next.clone());
        }
    }

    fn update_hover(&mut self, image: Option<&EditorPanelImage<S>>) -> Vec<Delivery<S>> {
        let next = image.filter(|image| {
            self.has(image, EditorPanelCapability::Pointer)
                && self.pointer_inside
                && self.position_for(image, false).is_some()
        });
        let unchanged = match (&self.hovered, next) {
            (None, None) => true,
            (Some(previous), Some(next)) => same_endpoint(previous, next),
            _ => false,
        };
        if unchanged {
            return Vec::new();
        }
        let mut deliveries = Vec::with_capacity(1);
        if let Some(previous) = self.hovered.take() {
            let position = self.position_for(&previous, true);
            deliveries.push(self.delivery(previous, EditorPanelInputKind::PointerLeft, position));
        }
        self.hovered = next.cloned();
        if self.capture.is_none() {
            self.reset_cursor();
        }
        deliveries
    }

    fn ime_delivery(
        &self,
        kind: EditorPanelInputKind,
        locked: bool,
        retiring: Option<(
            crate::panels::EditorPanelInstanceToken,
            super::EditorPanelTextInputScope,
        )>,
    ) -> (Vec<Delivery<S>>, bool) {
        if locked || self.closed || !self.focused {
            return (Vec::new(), locked);
        }
        if let Some(image) = self
            .keyboard
            .as_ref()
            .filter(|image| self.has(image, EditorPanelCapability::TextInput))
        {
            let mut delivery = self.delivery(image.clone(), kind, None);
            if let Some((token, scope)) = retiring {
                if token != image.token {
                    return (Vec::new(), true);
                }
                delivery.text_scope = Some(scope);
            }
            return (vec![delivery], true);
        }
        (Vec::new(), self.modal().is_some())
    }

    fn text_delivery(&mut self, kind: EditorPanelInputKind) -> (Vec<Delivery<S>>, bool) {
        let modal = self.modal().is_some();
        let Some(image) = self.keyboard_target() else {
            return (Vec::new(), modal);
        };
        if !self.has(&image, EditorPanelCapability::TextInput) {
            return (Vec::new(), true);
        }
        (vec![self.delivery(image, kind, None)], true)
    }

    fn delivery(
        &self,
        image: EditorPanelImage<S>,
        kind: EditorPanelInputKind,
        position: Option<[f32; 2]>,
    ) -> Delivery<S> {
        let text_scope = match &kind {
            EditorPanelInputKind::Key(_)
            | EditorPanelInputKind::Text(_)
            | EditorPanelInputKind::ImeEnabled
            | EditorPanelInputKind::ImeDisabled
            | EditorPanelInputKind::ImePreedit { .. }
            | EditorPanelInputKind::ImeCommit(_) => self
                .keyboard
                .as_ref()
                .filter(|owner| same_endpoint(owner, &image))
                .and(self.ime.scope.as_ref())
                .cloned(),
            _ => None,
        };
        Delivery {
            image,
            kind,
            position,
            modifiers: self.modifiers,
            text_scope,
        }
    }

    fn delivery_ready(&self, delivery: &Delivery<S>) -> bool {
        if matches!(&delivery.kind, EditorPanelInputKind::Cancelled(_)) {
            return true;
        }
        if !self.is_accepted(&delivery.image) {
            return false;
        }
        match &delivery.kind {
            EditorPanelInputKind::Text(_)
            | EditorPanelInputKind::ImePreedit { .. }
            | EditorPanelInputKind::ImeCommit(_) => {
                self.keyboard
                    .as_ref()
                    .is_some_and(|image| same_endpoint(image, &delivery.image))
                    && delivery.text_scope.is_some()
                    && delivery.text_scope == self.ime.scope
                    && (matches!(&delivery.kind, EditorPanelInputKind::Text(_)) || self.ime.ready)
            }
            EditorPanelInputKind::ImeEnabled => self.ime.ready,
            _ => true,
        }
    }

    fn is_accepted(&self, image: &EditorPanelImage<S>) -> bool {
        self.accepted.iter().any(|accepted| {
            same_endpoint(accepted, image) && Arc::ptr_eq(&accepted.snapshot, &image.snapshot)
        })
    }

    fn has(&self, image: &EditorPanelImage<S>, capability: EditorPanelCapability) -> bool {
        self.capabilities(&image.token)
            .is_some_and(|caps| caps.has(capability))
    }

    fn finish_button_release(&mut self, event: Option<&EditorPanelPlatformEvent>) {
        if let Some(EditorPanelPlatformEvent::PointerButton {
            button,
            state: ElementState::Released,
        }) = event
        {
            if self
                .capture
                .as_ref()
                .is_some_and(|capture| capture.button == *button)
            {
                self.capture = None;
                if self.hovered.is_none() {
                    self.reset_cursor();
                }
            }
        }
    }
}

fn same_endpoint<S>(left: &EditorPanelImage<S>, right: &EditorPanelImage<S>) -> bool {
    left.token == right.token && left.display == right.display
}
