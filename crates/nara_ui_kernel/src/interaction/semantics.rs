use crate::{
    UiActionId, UiFrameRevision, UiKey, UiRect, UiSeatId, UiSemanticNode, UiSemanticRole,
    UiSurfaceId,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiSemanticState {
    enabled: bool,
    inert: bool,
    selected: Option<bool>,
    expanded: Option<bool>,
    focusable: bool,
    focused: bool,
    pressed: bool,
}

impl UiSemanticState {
    pub(crate) const fn new(
        enabled: bool,
        inert: bool,
        selected: Option<bool>,
        expanded: Option<bool>,
        focusable: bool,
        focused: bool,
        pressed: bool,
    ) -> Self {
        Self {
            enabled,
            inert,
            selected,
            expanded,
            focusable,
            focused,
            pressed,
        }
    }

    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn inert(self) -> bool {
        self.inert
    }

    #[must_use]
    pub const fn selected(self) -> Option<bool> {
        self.selected
    }

    #[must_use]
    pub const fn expanded(self) -> Option<bool> {
        self.expanded
    }

    #[must_use]
    pub const fn focusable(self) -> bool {
        self.focusable
    }

    #[must_use]
    pub const fn focused(self) -> bool {
        self.focused
    }

    #[must_use]
    pub const fn pressed(self) -> bool {
        self.pressed
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticFact {
    key: UiKey,
    parent: Option<UiKey>,
    role: UiSemanticRole,
    name: String,
    value: Option<String>,
    actions: Vec<UiActionId>,
    rect: UiRect,
    state: UiSemanticState,
}

impl UiSemanticFact {
    pub(crate) fn from_frame_node(node: &UiSemanticNode, state: UiSemanticState) -> Self {
        Self {
            key: node.key().clone(),
            parent: node.parent().cloned(),
            role: node.role(),
            name: node.name().to_owned(),
            value: node.value().map(str::to_owned),
            actions: node.actions().to_vec(),
            rect: node.rect(),
            state,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &UiKey {
        &self.key
    }

    #[must_use]
    pub const fn parent(&self) -> Option<&UiKey> {
        self.parent.as_ref()
    }

    #[must_use]
    pub const fn role(&self) -> UiSemanticRole {
        self.role
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    #[must_use]
    pub fn actions(&self) -> &[UiActionId] {
        &self.actions
    }

    #[must_use]
    pub const fn rect(&self) -> UiRect {
        self.rect
    }

    #[must_use]
    pub const fn state(&self) -> UiSemanticState {
        self.state
    }
}

/// Semantic projection generated from the same exact presented frame and interaction state.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSemanticSnapshot {
    surface_id: UiSurfaceId,
    frame_revision: UiFrameRevision,
    presentation_epoch: u64,
    seat: UiSeatId,
    nodes: Vec<UiSemanticFact>,
}

impl UiSemanticSnapshot {
    pub(crate) const fn new(
        surface_id: UiSurfaceId,
        frame_revision: UiFrameRevision,
        presentation_epoch: u64,
        seat: UiSeatId,
        nodes: Vec<UiSemanticFact>,
    ) -> Self {
        Self {
            surface_id,
            frame_revision,
            presentation_epoch,
            seat,
            nodes,
        }
    }

    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn frame_revision(&self) -> UiFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.presentation_epoch
    }

    #[must_use]
    pub const fn seat(&self) -> UiSeatId {
        self.seat
    }

    #[must_use]
    pub fn nodes(&self) -> &[UiSemanticFact] {
        &self.nodes
    }
}
