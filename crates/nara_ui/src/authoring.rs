use nara_ecs::Component;
use nara_ui_kernel::{
    UiActionId, UiAuthoringError, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii,
    UiImageId, UiKeyError, UiLayer, UiSemanticRole, UiStyle, UiStyleError, UiTextAlignment,
    UiTextOverflow, UiThemePalette,
};

/// Marks one ECS entity as the root of an authored UI tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct UiRoot {
    layer: UiLayer,
}

impl UiRoot {
    #[must_use]
    pub const fn new(layer: UiLayer) -> Self {
        Self { layer }
    }

    #[must_use]
    pub const fn layer(self) -> UiLayer {
        self.layer
    }
}

impl Default for UiRoot {
    fn default() -> Self {
        Self::new(UiLayer::Base)
    }
}

/// Authored layout/style data for one hierarchy participant.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UiNode {
    style: UiStyle,
}

impl UiNode {
    #[must_use]
    pub const fn new(style: UiStyle) -> Self {
        Self { style }
    }

    #[must_use]
    pub const fn style(self) -> UiStyle {
        self.style
    }
}

impl Default for UiNode {
    fn default() -> Self {
        Self::new(UiStyle::fill())
    }
}

/// Optional visual decoration for one authored node.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct UiVisual(nara_ui_kernel::UiVisual);

impl UiVisual {
    #[must_use]
    pub fn solid(color: UiColor) -> Self {
        Self(nara_ui_kernel::UiVisual::solid(color))
    }

    #[must_use]
    pub fn role(role: UiColorRole) -> Self {
        Self(nara_ui_kernel::UiVisual::role(role))
    }

    #[must_use]
    pub fn image(image: UiImageId, tint: UiColor) -> Self {
        Self(nara_ui_kernel::UiVisual::image(image, tint))
    }

    #[must_use]
    pub fn with_background(mut self, brush: UiBrush) -> Self {
        self.0 = self.0.with_background(brush);
        self
    }

    #[must_use]
    pub fn with_border(mut self, border: UiBorder) -> Self {
        self.0 = self.0.with_border(border);
        self
    }

    #[must_use]
    pub fn with_radii(mut self, radii: UiCornerRadii) -> Self {
        self.0 = self.0.with_radii(radii);
        self
    }

    pub(crate) fn kernel(&self) -> nara_ui_kernel::UiVisual {
        self.0.clone()
    }
}

/// Bounded display text authored on one ECS node.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct UiText(nara_ui_kernel::UiText);

impl UiText {
    pub fn new(content: impl Into<String>, logical_size: f32) -> Result<Self, UiStyleError> {
        nara_ui_kernel::UiText::new(content, logical_size).map(Self)
    }

    #[must_use]
    pub fn with_brush(mut self, brush: UiBrush) -> Self {
        self.0 = self.0.with_brush(brush);
        self
    }

    #[must_use]
    pub fn with_alignment(mut self, alignment: UiTextAlignment) -> Self {
        self.0 = self.0.with_alignment(alignment);
        self
    }

    #[must_use]
    pub fn with_overflow(mut self, overflow: UiTextOverflow) -> Self {
        self.0 = self.0.with_overflow(overflow);
        self
    }

    #[must_use]
    pub fn with_semantic_text(mut self, text: impl Into<String>) -> Self {
        self.0 = self.0.with_semantic_text(text);
        self
    }

    pub(crate) fn kernel(&self) -> nara_ui_kernel::UiText {
        self.0.clone()
    }
}

/// Controlled Button description. Behavior state remains private to the runtime adapter.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct UiButton {
    label: String,
    action: UiActionId,
    enabled: bool,
    logical_text_size: f32,
}

impl UiButton {
    pub fn new(
        label: impl Into<String>,
        action: UiActionId,
        logical_text_size: f32,
    ) -> Result<Self, UiAuthoringError> {
        let label = label.into();
        nara_ui_kernel::UiSemantics::new(UiSemanticRole::Button, label.clone())?;
        nara_ui_kernel::UiText::new(label.clone(), logical_text_size)?;
        Ok(Self {
            label,
            action,
            enabled: true,
            logical_text_size,
        })
    }

    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
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
    pub const fn logical_text_size(&self) -> f32 {
        self.logical_text_size
    }
}

/// Optional explicit semantics for a non-Button node.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct UiSemantics(nara_ui_kernel::UiSemantics);

impl UiSemantics {
    pub fn new(role: UiSemanticRole, name: impl Into<String>) -> Result<Self, UiAuthoringError> {
        nara_ui_kernel::UiSemantics::new(role, name).map(Self)
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Result<Self, UiAuthoringError> {
        self.0 = self.0.with_value(value)?;
        Ok(self)
    }

    #[must_use]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.0 = self.0.with_selected(selected);
        self
    }

    #[must_use]
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.0 = self.0.with_expanded(expanded);
        self
    }

    pub fn with_action(mut self, action: UiActionId) -> Result<Self, UiAuthoringError> {
        self.0 = self.0.with_action(action)?;
        Ok(self)
    }

    pub(crate) fn kernel(&self) -> nara_ui_kernel::UiSemantics {
        self.0.clone()
    }
}

/// Immutable theme value shared by every root compiled for one surface.
#[derive(Debug, Default, Clone, Copy, PartialEq, Component)]
pub struct UiTheme(nara_ui_kernel::UiTheme);

impl UiTheme {
    pub fn new(revision: u64, palette: UiThemePalette) -> Result<Self, UiStyleError> {
        nara_ui_kernel::UiTheme::new(revision, palette).map(Self)
    }

    #[must_use]
    pub const fn revision(self) -> nara_ui_kernel::UiRevision {
        self.0.revision()
    }

    #[must_use]
    pub const fn palette(self) -> UiThemePalette {
        self.0.palette()
    }

    pub(crate) const fn kernel(self) -> nara_ui_kernel::UiTheme {
        self.0
    }
}

/// Explicit caller-owned identity for runtime-created roots or nodes.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct UiRuntimeKey {
    domain: String,
    local: String,
}

impl UiRuntimeKey {
    pub fn new(domain: impl Into<String>, local: impl Into<String>) -> Result<Self, UiKeyError> {
        let domain = domain.into();
        let local = local.into();
        nara_ui_kernel::UiRootKey::runtime(domain.clone(), local.clone())?;
        nara_ui_kernel::UiLocalKey::runtime(domain.clone(), local.clone())?;
        Ok(Self { domain, local })
    }

    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }

    #[must_use]
    pub fn local(&self) -> &str {
        &self.local
    }
}
