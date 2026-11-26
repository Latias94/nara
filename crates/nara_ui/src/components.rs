use crate::kernel::{
    UiActionId, UiAuthoringError, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii,
    UiImageId, UiKeyError, UiLayer, UiSemanticRole, UiStyle, UiStyleError, UiTextAlignment,
    UiTextOverflow, UiThemePalette,
};
use nara_asset::Handle;
use nara_asset::image::ImageAsset;
use nara_ecs::Component;
use nara_ecs::bevy_ecs::resource::Resource;

/// Starts a UI tree. Add children with Bevy's `ChildOf` relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
#[require(UiNode)]
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

/// Layout and visibility of a root or child in a UI tree.
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

/// Optional background, border and corner radii for a UI node.
#[derive(Debug, Clone, PartialEq, Component)]
pub struct UiVisual {
    visual: crate::kernel::UiVisual,
    image_asset: Option<Handle<ImageAsset>>,
}

impl UiVisual {
    #[must_use]
    pub fn solid(color: UiColor) -> Self {
        Self {
            visual: crate::kernel::UiVisual::solid(color),
            image_asset: None,
        }
    }

    #[must_use]
    pub fn role(role: UiColorRole) -> Self {
        Self {
            visual: crate::kernel::UiVisual::role(role),
            image_asset: None,
        }
    }

    #[must_use]
    pub fn image(image: UiImageId, tint: UiColor) -> Self {
        Self {
            visual: crate::kernel::UiVisual::image(image, tint),
            image_asset: None,
        }
    }

    /// Uses a typed image handle as the background.
    #[must_use]
    pub fn image_asset(handle: Handle<ImageAsset>, tint: UiColor) -> Self {
        Self {
            visual: crate::kernel::UiVisual::image(
                UiImageId::external::<ImageAsset>(handle.id().raw()),
                tint,
            ),
            image_asset: Some(handle),
        }
    }

    #[must_use]
    pub const fn image_handle(&self) -> Option<Handle<ImageAsset>> {
        self.image_asset
    }

    #[must_use]
    pub fn with_background(mut self, brush: UiBrush) -> Self {
        self.visual = self.visual.with_background(brush);
        self.image_asset = None;
        self
    }

    #[must_use]
    pub fn with_border(mut self, border: UiBorder) -> Self {
        self.visual = self.visual.with_border(border);
        self
    }

    #[must_use]
    pub fn with_radii(mut self, radii: UiCornerRadii) -> Self {
        self.visual = self.visual.with_radii(radii);
        self
    }

    pub(crate) fn kernel(&self) -> crate::kernel::UiVisual {
        self.visual.clone()
    }
}

/// Display text on a UI node.
#[derive(Debug, Clone, PartialEq, Component)]
#[require(UiNode)]
pub struct UiText(crate::kernel::UiText);

impl UiText {
    pub fn new(content: impl Into<String>, logical_size: f32) -> Result<Self, UiStyleError> {
        crate::kernel::UiText::new(content, logical_size).map(Self)
    }

    /// Replaces dynamic display content without changing authored presentation fields.
    #[must_use]
    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.0 = self.0.with_content(content);
        self
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

    pub(crate) fn kernel(&self) -> crate::kernel::UiText {
        self.0.clone()
    }
}

/// Presents a labelled control and emits a [`crate::UiActionEvent`] on activation.
#[derive(Debug, Clone, PartialEq, Component)]
#[require(UiNode)]
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
        crate::kernel::UiSemantics::new(UiSemanticRole::Button, label.clone())?;
        crate::kernel::UiText::new(label.clone(), logical_text_size)?;
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

/// Optional theme overrides for a button's interaction states.
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct UiButtonVisuals {
    normal_text: UiColorRole,
    hovered_background: UiColorRole,
    hovered_text: UiColorRole,
    pressed_background: UiColorRole,
    pressed_text: UiColorRole,
    focused_border: UiColorRole,
    focused_text: UiColorRole,
    focus_border_width: f32,
    disabled_background: UiColorRole,
    disabled_text: UiColorRole,
}

impl UiButtonVisuals {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            normal_text: UiColorRole::PrimaryText,
            hovered_background: UiColorRole::Accent,
            hovered_text: UiColorRole::Surface,
            pressed_background: UiColorRole::Positive,
            pressed_text: UiColorRole::Surface,
            focused_border: UiColorRole::Focus,
            focused_text: UiColorRole::Focus,
            focus_border_width: 2.0,
            disabled_background: UiColorRole::Disabled,
            disabled_text: UiColorRole::SecondaryText,
        }
    }

    #[must_use]
    pub const fn with_normal_text(mut self, role: UiColorRole) -> Self {
        self.normal_text = role;
        self
    }

    #[must_use]
    pub const fn with_hovered(mut self, background: UiColorRole, text: UiColorRole) -> Self {
        self.hovered_background = background;
        self.hovered_text = text;
        self
    }

    #[must_use]
    pub const fn with_pressed(mut self, background: UiColorRole, text: UiColorRole) -> Self {
        self.pressed_background = background;
        self.pressed_text = text;
        self
    }

    pub fn with_focused(
        mut self,
        border: UiColorRole,
        text: UiColorRole,
        border_width: f32,
    ) -> Result<Self, UiStyleError> {
        UiBorder::new(border_width, UiBrush::Role(border))?;
        if border_width <= 0.0 {
            return Err(UiStyleError::InvalidFiniteValue {
                field: "button.focus_border_width",
                value: border_width,
            });
        }
        self.focused_border = border;
        self.focused_text = text;
        self.focus_border_width = border_width;
        Ok(self)
    }

    #[must_use]
    pub const fn with_disabled(mut self, background: UiColorRole, text: UiColorRole) -> Self {
        self.disabled_background = background;
        self.disabled_text = text;
        self
    }

    #[must_use]
    pub const fn normal_text(self) -> UiColorRole {
        self.normal_text
    }

    #[must_use]
    pub const fn hovered_background(self) -> UiColorRole {
        self.hovered_background
    }

    #[must_use]
    pub const fn hovered_text(self) -> UiColorRole {
        self.hovered_text
    }

    #[must_use]
    pub const fn pressed_background(self) -> UiColorRole {
        self.pressed_background
    }

    #[must_use]
    pub const fn pressed_text(self) -> UiColorRole {
        self.pressed_text
    }

    #[must_use]
    pub const fn focused_border(self) -> UiColorRole {
        self.focused_border
    }

    #[must_use]
    pub const fn focused_text(self) -> UiColorRole {
        self.focused_text
    }

    #[must_use]
    pub const fn focus_border_width(self) -> f32 {
        self.focus_border_width
    }

    #[must_use]
    pub const fn disabled_background(self) -> UiColorRole {
        self.disabled_background
    }

    #[must_use]
    pub const fn disabled_text(self) -> UiColorRole {
        self.disabled_text
    }
}

impl Default for UiButtonVisuals {
    fn default() -> Self {
        Self::new()
    }
}

/// Optional explicit semantics for a non-Button node.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct UiSemantics(crate::kernel::UiSemantics);

impl UiSemantics {
    pub fn new(role: UiSemanticRole, name: impl Into<String>) -> Result<Self, UiAuthoringError> {
        crate::kernel::UiSemantics::new(role, name).map(Self)
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

    pub(crate) fn kernel(&self) -> crate::kernel::UiSemantics {
        self.0.clone()
    }
}

/// Theme shared by every root. Insert it as a resource to configure the HUD.
#[derive(Debug, Default, Clone, Copy, PartialEq, Component)]
pub struct UiTheme(crate::kernel::UiTheme);

impl Resource for UiTheme {}

impl UiTheme {
    pub fn new(revision: u64, palette: UiThemePalette) -> Result<Self, UiStyleError> {
        crate::kernel::UiTheme::new(revision, palette).map(Self)
    }

    #[must_use]
    pub const fn revision(self) -> crate::kernel::UiRevision {
        self.0.revision()
    }

    #[must_use]
    pub const fn palette(self) -> UiThemePalette {
        self.0.palette()
    }

    pub(crate) const fn kernel(self) -> crate::kernel::UiTheme {
        self.0
    }
}

/// Optional name for a code-created root or node in frame observations.
#[derive(Debug, Clone, PartialEq, Eq, Component)]
pub struct UiRuntimeKey {
    domain: String,
    local: String,
}

impl UiRuntimeKey {
    pub fn new(domain: impl Into<String>, local: impl Into<String>) -> Result<Self, UiKeyError> {
        let domain = domain.into();
        let local = local.into();
        crate::kernel::UiRootKey::runtime(domain.clone(), local.clone())?;
        crate::kernel::UiLocalKey::runtime(domain.clone(), local.clone())?;
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
