use std::{collections::VecDeque, sync::Arc, time::Instant};

use nara::window::WindowId;
use nara_render_wgpu::WgpuWindowFrameOutcome;
use winit::{
    keyboard::ModifiersState,
    window::{CursorIcon, Window},
};

use crate::{
    EditorApplication,
    panels::{EditorPanel, EditorPanelHost, EditorPanelInstanceToken},
};

use super::{
    Attachment, Capabilities, Delivery, EDITOR_PANEL_IMAGE_LIMIT, EditorClipboard,
    EditorPanelCancelReason, EditorPanelCapability, EditorPanelImage, EditorPanelImageError,
    EditorPanelImageFrame, EditorPanelImageHost, EditorPanelImageStatus, EditorPanelInputKind,
    EditorPanelWindowGeometry, ImageCandidate, NativeIme, geometry,
};

impl<S> EditorPanelImageHost<S> {
    /// Borrows the original native window through a shared owner. The caller remains responsible
    /// for forwarding its events and rendering through its existing graphics Host.
    #[must_use]
    pub fn new(window: Arc<Window>, window_id: WindowId, clipboard: EditorClipboard) -> Self {
        let geometry = EditorPanelWindowGeometry::read(&window);
        let focused = window.has_focus();
        Self {
            owner: Arc::new(()),
            semantic: None,
            window,
            window_id,
            clipboard,
            attachments: Vec::new(),
            pending: None,
            accepted: Vec::new(),
            geometry,
            last_display_frame: None,
            capture: None,
            keyboard: None,
            hovered: None,
            cancellations: VecDeque::with_capacity(3),
            pointer: None,
            pointer_inside: false,
            modifiers: ModifiersState::empty(),
            focused,
            ime: NativeIme::default(),
            cursor: CursorIcon::Default,
            redraw_requested: false,
            closed: false,
        }
    }

    #[must_use]
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    pub(super) fn check_semantic<P: EditorPanel>(
        &self,
        semantic: &EditorPanelHost<P>,
    ) -> Result<(), EditorPanelImageError> {
        if self
            .semantic
            .as_ref()
            .is_some_and(|owner| !Arc::ptr_eq(owner, semantic.identity()))
        {
            return Err(EditorPanelImageError::ForeignSemanticHost);
        }
        Ok(())
    }

    /// Attaches one original semantic incarnation and its complete required native services.
    /// Reattaching the same token/capabilities is idempotent; capability changes require detach.
    pub fn attach(
        &mut self,
        application: &EditorApplication,
        token: EditorPanelInstanceToken,
        capabilities: impl IntoIterator<Item = EditorPanelCapability>,
    ) -> Result<(), EditorPanelImageError> {
        if self.closed {
            return Err(EditorPanelImageError::Closed);
        }
        let capabilities = Capabilities::admit(capabilities)?;
        if self
            .semantic
            .as_ref()
            .is_some_and(|owner| !Arc::ptr_eq(owner, &token.host))
        {
            return Err(EditorPanelImageError::ForeignSemanticHost);
        }
        self.reconcile(application);
        if !token.is_current(application) {
            return Err(EditorPanelImageError::RetiredInstance);
        }
        if let Some(existing) = self.attachments.iter().find(|entry| entry.token == token) {
            return if existing.capabilities == capabilities {
                Ok(())
            } else {
                Err(EditorPanelImageError::AttachmentConflict)
            };
        }
        let previous = self
            .attachments
            .iter()
            .find(|entry| entry.token.instance() == token.instance())
            .map(|entry| entry.token.clone());
        if let Some(previous) = previous {
            self.detach(&previous);
        }
        if self.attachments.len() >= EDITOR_PANEL_IMAGE_LIMIT {
            return Err(EditorPanelImageError::LimitExceeded);
        }
        self.semantic.get_or_insert_with(|| Arc::clone(&token.host));
        self.attachments.push(Attachment {
            token,
            capabilities,
            deadline: None,
        });
        Ok(())
    }

    /// Withdraws this attachment and queues cancellation for its original owners. This does not
    /// create or detach an OS window. Flush pending cancellation with `dispatch(None)`.
    pub fn detach(&mut self, token: &EditorPanelInstanceToken) -> bool {
        let existed = self.attachments.iter().any(|entry| &entry.token == token);
        if !existed {
            return false;
        }
        self.cancel_where(
            |image| &image.token == token,
            EditorPanelCancelReason::AttachmentRetired,
        );
        self.accepted.retain(|image| &image.token != token);
        self.attachments.retain(|entry| &entry.token != token);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.images.iter().any(|image| &image.token == token))
        {
            self.pending = None;
        }
        true
    }

    /// Stages a complete ordered window image list. Render each image using exactly its exposed
    /// mapping and clip, and record its actual sample through the original `WgpuWindowFrame`.
    /// An invalid proposed stage leaves the preceding pending candidate unconsumed.
    pub fn stage(
        &mut self,
        application: &EditorApplication,
        display_frame_index: u64,
        images: Vec<EditorPanelImage<S>>,
    ) -> Result<EditorPanelImageFrame<S>, EditorPanelImageError> {
        if self.closed {
            return Err(EditorPanelImageError::Closed);
        }
        self.reconcile(application);
        if images.len() > EDITOR_PANEL_IMAGE_LIMIT {
            return Err(EditorPanelImageError::LimitExceeded);
        }
        if self
            .last_display_frame
            .is_some_and(|last| display_frame_index <= last)
        {
            return Err(EditorPanelImageError::StaleDisplayFrame);
        }
        if self.geometry.logical_size().is_none() {
            return Err(EditorPanelImageError::InvalidGeometry);
        }
        for (index, image) in images.iter().enumerate() {
            self.validate_image(application, image)?;
            if images[..index]
                .iter()
                .any(|previous| previous.token == image.token)
            {
                return Err(EditorPanelImageError::DuplicatePanel);
            }
            if images[..index]
                .iter()
                .any(|previous| previous.display.occurrence() == image.display.occurrence())
            {
                return Err(EditorPanelImageError::DuplicateOccurrence);
            }
        }
        self.withdraw_incompatible(&images);
        let candidate = Arc::new(ImageCandidate {
            owner: Arc::clone(&self.owner),
            window: self.window_id,
            display_frame_index,
            geometry: self.geometry,
            images,
        });
        self.pending = Some(Arc::clone(&candidate));
        self.redraw_requested = false;
        Ok(EditorPanelImageFrame(candidate))
    }

    /// Consumes only this Host's current candidate. A successful exact complete original native
    /// outcome adopts every mapping and snapshot atomically. Malformed outcomes fail closed.
    pub fn presented(
        &mut self,
        application: &EditorApplication,
        frame: EditorPanelImageFrame<S>,
        outcome: &WgpuWindowFrameOutcome,
    ) -> Result<EditorPanelImageStatus, EditorPanelImageError> {
        if !Arc::ptr_eq(&self.owner, &frame.0.owner) {
            return Err(EditorPanelImageError::ForeignFrame);
        }
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| !Arc::ptr_eq(pending, &frame.0))
        {
            return Err(EditorPanelImageError::SupersededFrame);
        }
        self.pending = None;
        let candidate = frame.0;
        self.reconcile(application);
        let validation = (|| {
            if self.closed {
                return Err(EditorPanelImageError::Closed);
            }
            if candidate.geometry != self.geometry || self.geometry.logical_size().is_none() {
                return Err(EditorPanelImageError::InvalidGeometry);
            }
            for image in &candidate.images {
                self.validate_image(application, image)?;
            }
            let Some(window) = outcome.window() else {
                return if outcome.composites().is_empty() {
                    Ok(EditorPanelImageStatus::Deferred)
                } else {
                    Err(EditorPanelImageError::IncompletePresentation)
                };
            };
            if window.window_id() != candidate.window
                || window.display_frame_index() != candidate.display_frame_index
                || outcome.composites().len() != candidate.images.len()
            {
                return Err(EditorPanelImageError::IncompletePresentation);
            }
            for image in &candidate.images {
                let receipt = image.display.receipt();
                if receipt.backend_instance_id() != window.backend_instance_id()
                    || receipt.device_epoch() != window.device_epoch()
                    || outcome
                        .composites()
                        .iter()
                        .filter(|composite| {
                            composite.window() == window
                                && composite.image() == receipt
                                && composite.occurrence() == image.display.occurrence()
                        })
                        .count()
                        != 1
                {
                    return Err(EditorPanelImageError::IncompletePresentation);
                }
            }
            Ok(EditorPanelImageStatus::Presented)
        })();
        match validation {
            Ok(EditorPanelImageStatus::Presented) => {
                self.withdraw_incompatible(&candidate.images);
                let modal = candidate
                    .images
                    .iter()
                    .rev()
                    .find(|image| image.window_modal);
                if let Some(modal) = modal {
                    self.cancel_where(
                        |image| image.token != modal.token,
                        EditorPanelCancelReason::ModalAdmitted,
                    );
                }
                if let Some(capture) = &mut self.capture {
                    if let Some(next) = candidate
                        .images
                        .iter()
                        .find(|image| image.token == capture.image.token)
                    {
                        capture.image = next.clone();
                    }
                }
                for owner in [&mut self.keyboard, &mut self.hovered] {
                    if let Some(previous) = owner {
                        if let Some(next) = candidate
                            .images
                            .iter()
                            .find(|image| image.token == previous.token)
                        {
                            *previous = next.clone();
                        }
                    }
                }
                self.accepted = candidate.images.clone();
                self.last_display_frame = Some(candidate.display_frame_index);
                Ok(EditorPanelImageStatus::Presented)
            }
            Ok(EditorPanelImageStatus::Deferred) => Ok(EditorPanelImageStatus::Deferred),
            Err(error) => {
                self.withdraw_all(EditorPanelCancelReason::NativeInvalidated);
                Err(error)
            }
        }
    }

    /// Reports external native/backend invalidation when no window outcome is available.
    /// The caller retains responsibility for the original GPU Host's recovery and retirement.
    pub fn invalidate(&mut self) {
        self.withdraw_all(EditorPanelCancelReason::NativeInvalidated);
    }

    /// Closes native interaction immediately. `dispatch(None)` may still deliver the queued
    /// cancellations to original live semantic state; all future stages/attachments refuse.
    pub fn close(&mut self) {
        self.withdraw_all(EditorPanelCancelReason::WindowClosed);
        self.attachments.clear();
        self.closed = true;
    }

    /// Earliest explicit deadline among original live attachments with accepted output.
    /// Reconciliation may queue cancellation; flush it through `dispatch(None)`.
    pub fn next_deadline(&mut self, application: &EditorApplication) -> Option<Instant> {
        self.reconcile(application);
        self.attachments
            .iter()
            .filter_map(|entry| entry.deadline)
            .min()
    }

    /// Retires due deadlines and coalesces them into at most one actual native redraw request.
    /// Static panels have no deadline, and no timer or polling loop is created by this Host.
    pub fn poll_deadlines(&mut self, application: &EditorApplication, now: Instant) -> bool {
        self.reconcile(application);
        let mut due = false;
        for entry in &mut self.attachments {
            if entry.deadline.is_some_and(|deadline| deadline <= now) {
                entry.deadline = None;
                due = true;
            }
        }
        due && self.request_redraw()
    }

    fn validate_image(
        &self,
        application: &EditorApplication,
        image: &EditorPanelImage<S>,
    ) -> Result<(), EditorPanelImageError> {
        if !image.token.is_current(application) {
            return Err(EditorPanelImageError::RetiredInstance);
        }
        let caps = self
            .capabilities(&image.token)
            .ok_or(EditorPanelImageError::NotAttached)?;
        if image.window_modal && !caps.has(EditorPanelCapability::WindowModal) {
            return Err(EditorPanelImageError::MissingCapability(
                EditorPanelCapability::WindowModal,
            ));
        }
        if !image.display.output().is_current() {
            return Err(EditorPanelImageError::StaleImage);
        }
        geometry::validate_image(image, self.geometry)
    }

    pub(super) fn capabilities(&self, token: &EditorPanelInstanceToken) -> Option<Capabilities> {
        self.attachments
            .iter()
            .find(|entry| &entry.token == token)
            .map(|entry| entry.capabilities)
    }

    pub(super) fn reconcile(&mut self, application: &EditorApplication) {
        let geometry = EditorPanelWindowGeometry::read(&self.window);
        if geometry != self.geometry || geometry.logical_size().is_none() {
            self.withdraw_all(EditorPanelCancelReason::WindowGeometryChanged);
            self.geometry = geometry;
            self.pointer = None;
            self.pointer_inside = false;
        }
        let retired = self
            .attachments
            .iter()
            .filter(|entry| !entry.token.is_current(application))
            .map(|entry| entry.token.clone())
            .collect::<Vec<_>>();
        for token in retired {
            self.detach(&token);
        }
        if self
            .accepted
            .iter()
            .any(|image| !image.display.output().is_current())
        {
            self.withdraw_all(EditorPanelCancelReason::NativeInvalidated);
        }
        if self.pending.as_ref().is_some_and(|pending| {
            pending
                .images
                .iter()
                .any(|image| !image.display.output().is_current())
        }) {
            self.pending = None;
        }
    }

    fn withdraw_incompatible(&mut self, images: &[EditorPanelImage<S>]) {
        self.cancel_where(
            |old| !images.iter().any(|next| old.compatible(next)),
            EditorPanelCancelReason::OutputReplaced,
        );
        self.accepted
            .retain(|old| images.iter().any(|next| old.compatible(next)));
        for entry in &mut self.attachments {
            if !self.accepted.iter().any(|image| image.token == entry.token) {
                entry.deadline = None;
            }
        }
    }

    pub(super) fn withdraw_all(&mut self, reason: EditorPanelCancelReason) {
        self.cancel_where(|_| true, reason);
        self.accepted.clear();
        self.pending = None;
        for entry in &mut self.attachments {
            entry.deadline = None;
        }
        self.reset_ime();
        self.reset_cursor();
    }

    pub(super) fn cancel_where(
        &mut self,
        mut predicate: impl FnMut(&EditorPanelImage<S>) -> bool,
        reason: EditorPanelCancelReason,
    ) {
        let keyboard_token = self.keyboard.as_ref().map(|image| image.token.clone());
        let text_scope = self.ime.scope.clone();
        let mut cancelled = Vec::with_capacity(3);
        if self
            .capture
            .as_ref()
            .is_some_and(|capture| predicate(&capture.image))
        {
            if let Some(capture) = self.capture.take() {
                cancelled.push(capture.image);
            }
            self.reset_cursor();
        }
        if self.keyboard.as_ref().is_some_and(&mut predicate) {
            if let Some(keyboard) = self.keyboard.take() {
                cancelled.push(keyboard);
            }
            self.reset_ime();
        }
        if self.hovered.as_ref().is_some_and(predicate) {
            if let Some(hovered) = self.hovered.take() {
                cancelled.push(hovered);
            }
            self.reset_cursor();
        }
        let mut unique: Vec<EditorPanelImage<S>> = Vec::with_capacity(3);
        for image in cancelled {
            if unique.iter().any(|previous| previous.token == image.token) {
                continue;
            }
            self.cancellations.push_back(Delivery {
                position: self.position_for(&image, true),
                image: image.clone(),
                kind: EditorPanelInputKind::Cancelled(reason),
                modifiers: self.modifiers,
                text_scope: (keyboard_token.as_ref() == Some(&image.token))
                    .then(|| text_scope.clone())
                    .flatten(),
            });
            unique.push(image);
        }
    }

    pub(super) fn request_redraw(&mut self) -> bool {
        if self.closed || self.redraw_requested {
            return false;
        }
        self.window.request_redraw();
        self.redraw_requested = true;
        true
    }

    pub(super) fn reset_cursor(&mut self) {
        if self.cursor != CursorIcon::Default {
            self.window.set_cursor(CursorIcon::Default);
            self.cursor = CursorIcon::Default;
        }
    }
}

impl<S> Drop for EditorPanelImageHost<S> {
    fn drop(&mut self) {
        self.reset_ime();
        self.reset_cursor();
    }
}
