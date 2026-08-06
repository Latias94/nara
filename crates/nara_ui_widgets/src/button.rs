use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiAuthoringError, UiBrush, UiDispatchBatch, UiInteraction,
    UiKey, UiNode, UiRootKey, UiRoutePhase, UiRoutedEvent, UiRoutedEventKind, UiSemanticRole,
    UiSemantics, UiStyle, UiStyleError, UiText, UiVisual,
};
use thiserror::Error;

use crate::{UiIntent, UiIntentBatch, UiIntentBatchError};

/// Controlled Button description. Product values and command authority remain with the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiButton {
    key: UiKey,
    label: String,
    action: UiActionId,
    enabled: bool,
}

impl UiButton {
    pub fn new(
        key: UiKey,
        label: impl Into<String>,
        action: UiActionId,
    ) -> Result<Self, UiWidgetError> {
        if key.local_key().is_none() {
            return Err(UiWidgetError::RootKeyUnsupported);
        }
        let label = label.into();
        UiSemantics::new(UiSemanticRole::Button, label.clone())?;
        Ok(Self {
            key,
            label,
            action,
            enabled: true,
        })
    }

    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn action(&self) -> &UiActionId {
        &self.action
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Builds the concrete authored node while keeping behavior state outside the document.
    pub fn authored_node(
        &self,
        root_key: &UiRootKey,
        style: UiStyle,
        visual: UiVisual,
        text_brush: UiBrush,
        logical_text_size: f32,
    ) -> Result<UiNode, UiWidgetError> {
        if self.key.root_key() != root_key {
            return Err(UiWidgetError::RootMismatch {
                expected: self.key.root_key().clone(),
                actual: root_key.clone(),
            });
        }
        let local_key = self
            .key
            .local_key()
            .cloned()
            .ok_or(UiWidgetError::RootKeyUnsupported)?;
        let semantics = UiSemantics::new(UiSemanticRole::Button, self.label.clone())?
            .with_action(self.action.clone())?;
        let text = UiText::new(self.label.clone(), logical_text_size)?.with_brush(text_brush);
        Ok(UiNode::new(style)
            .with_key(local_key)
            .with_visual(visual)
            .with_text(text)
            .with_interaction(UiInteraction::focus_target().with_enabled(self.enabled))
            .with_semantics(semantics))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiButtonState {
    hovered: bool,
    pressed: bool,
    focused: bool,
}

impl UiButtonState {
    #[must_use]
    pub const fn hovered(self) -> bool {
        self.hovered
    }

    #[must_use]
    pub const fn pressed(self) -> bool {
        self.pressed
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn visual_state(self, button: &UiButton) -> UiButtonVisualState {
        if !button.enabled {
            UiButtonVisualState::Disabled
        } else if self.pressed && self.hovered {
            UiButtonVisualState::Pressed
        } else if self.hovered {
            UiButtonVisualState::Hovered
        } else if self.focused {
            UiButtonVisualState::Focused
        } else {
            UiButtonVisualState::Idle
        }
    }

    /// Reduces one atomic kernel dispatch into local state and a bounded intent batch.
    pub fn reduce_batch(
        &mut self,
        button: &UiButton,
        dispatch: &UiDispatchBatch,
        intents: &mut UiIntentBatch,
    ) -> Result<(), UiIntentBatchError> {
        let mut candidate = *self;
        let mut emitted = Vec::new();
        for event in dispatch.events() {
            if let Some(intent) = candidate.reduce_event(button, event) {
                emitted.push(intent);
            }
        }
        intents.try_extend(emitted)?;
        *self = candidate;
        Ok(())
    }

    fn reduce_event(&mut self, button: &UiButton, event: &UiRoutedEvent) -> Option<UiIntent> {
        if event.phase() != UiRoutePhase::Target || event.target() != button.key() {
            return None;
        }
        if !button.enabled {
            self.hovered = false;
            self.pressed = false;
            self.focused = false;
            return None;
        }
        match event.kind() {
            UiRoutedEventKind::HoverChanged { hovered } => self.hovered = *hovered,
            UiRoutedEventKind::Pressed => self.pressed = true,
            UiRoutedEventKind::Released { inside } => {
                let activate = self.pressed && *inside;
                self.pressed = false;
                if activate {
                    return Some(UiIntent::Activate {
                        target: button.key.clone(),
                        action: button.action.clone(),
                        source: UiActivationSource::Pointer,
                    });
                }
            }
            UiRoutedEventKind::Cancelled => {
                self.hovered = false;
                self.pressed = false;
            }
            UiRoutedEventKind::FocusChanged { focused } => self.focused = *focused,
            UiRoutedEventKind::ActivateRequested { action, source }
                if action == button.action() =>
            {
                return Some(UiIntent::Activate {
                    target: button.key.clone(),
                    action: action.clone(),
                    source: *source,
                });
            }
            UiRoutedEventKind::ActivateRequested { .. } => {}
        }
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiButtonVisualState {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Focused,
    Disabled,
}

#[derive(Debug, Error, PartialEq)]
pub enum UiWidgetError {
    #[error(transparent)]
    Authoring(#[from] UiAuthoringError),
    #[error(transparent)]
    Style(#[from] UiStyleError),
    #[error("a Button requires a root-scoped local UI key")]
    RootKeyUnsupported,
    #[error("Button root {expected:?} does not match authored root {actual:?}")]
    RootMismatch {
        expected: UiRootKey,
        actual: UiRootKey,
    },
}
