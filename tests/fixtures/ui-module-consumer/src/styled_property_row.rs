//! An independent styled composite over public text, layout and controlled-widget interfaces.

use nara_text::TextEngine;
use nara_ui_kernel::{
    UiActionId, UiBorder, UiBrush, UiColor, UiCornerRadii, UiCustomLayout, UiDimension,
    UiDispatchBatch, UiEdges, UiKey, UiKeyAtom, UiLayoutError, UiLocalKey, UiMesh, UiMeshVertex,
    UiNode, UiOverflow, UiPoint, UiRootKey, UiSemanticRole, UiSemantics, UiSize, UiStyle, UiVisual,
};
use nara_ui_widgets::{
    UiAppearance, UiButton, UiButtonState, UiButtonVariant, UiControlMetrics, UiControlSize,
    UiIconLabel, UiIntentBatch, UiPropertyRow, UiTextField, UiTextFieldAppearance, UiTextRole,
};

use crate::StatusLabelError;

/// Initial content and explicit item identity, independent of the accessible label.
pub struct StyledProperty<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub initial_value: &'a str,
    pub action_label: &'a str,
    pub action: UiActionId,
}

/// A label, retained editable value, action and inline status in one decorated measured row.
///
/// Retain this value by parent/item identity, for example in a caller's `UiStateStore`.
/// `scope` and the property's `id` name its visual children; use a distinct scope for each
/// collection in a root. Neither labels nor item positions are identities. The field owns
/// its original buffer, preedit, selection and local history; the caller owns domain changes,
/// membership, button reduction and presented input admission.
pub struct StyledPropertyRow {
    root: UiRootKey,
    key: UiLocalKey,
    label_key: UiLocalKey,
    label_text_key: UiLocalKey,
    icon_key: UiLocalKey,
    status_key: UiLocalKey,
    label: String,
    icon: UiMesh,
    field: UiTextField,
    button: UiButton,
    button_state: UiButtonState,
    appearance: UiAppearance,
    layout: Option<(UiPropertyRow, UiCustomLayout)>,
    label_layout: Option<(UiIconLabel, UiCustomLayout)>,
}

impl StyledPropertyRow {
    pub fn new(
        root: UiRootKey,
        scope: &str,
        property: StyledProperty<'_>,
        appearance: UiAppearance,
        engine: &mut TextEngine,
    ) -> Result<Self, StatusLabelError> {
        UiKeyAtom::new(scope)?;
        UiKeyAtom::new(property.id)?;
        let item = format!("{}:{scope}{}", scope.len(), property.id);
        let child_key = |part: &str| -> Result<UiLocalKey, StatusLabelError> {
            let local = UiLocalKey::runtime(format!("independent.property.{part}"), &item)?;
            UiKey::scoped(root.clone(), local.clone())?;
            Ok(local)
        };
        let key = child_key("row")?;
        let label_key = child_key("label")?;
        let label_text_key = child_key("label-text")?;
        let icon_key = child_key("icon")?;
        let status_key = child_key("status")?;
        let button_key = UiKey::scoped(root.clone(), child_key("action")?)?;
        let metrics = appearance.control_metrics(UiControlSize::Regular)?;
        let mut field = UiTextField::with_typography(
            root.clone(),
            property.label,
            property.initial_value,
            metrics.typography(),
            engine,
        )?;
        field.set_appearance(field_appearance(appearance, metrics, &field)?);
        Ok(Self {
            root,
            key,
            label_key,
            label_text_key,
            icon_key,
            status_key,
            label: property.label.to_owned(),
            icon: property_icon(appearance.palette().accent)?,
            field,
            button: UiButton::new(button_key, property.action_label, property.action)?,
            button_state: UiButtonState::default(),
            appearance,
            layout: None,
            label_layout: None,
        })
    }

    #[must_use]
    pub const fn field(&self) -> &UiTextField {
        &self.field
    }

    /// Input must already belong to this field's live focus, as with an ordinary text field.
    pub fn field_mut(&mut self) -> &mut UiTextField {
        &mut self.field
    }

    #[must_use]
    pub const fn button(&self) -> &UiButton {
        &self.button
    }

    /// Reduces an already admitted batch through the original controlled button.
    pub fn reduce_button(
        &mut self,
        dispatch: &UiDispatchBatch,
        intents: &mut UiIntentBatch,
    ) -> Result<(), StatusLabelError> {
        self.button_state
            .reduce_batch(&self.button, dispatch, intents)?;
        Ok(())
    }

    #[must_use]
    pub const fn appearance(&self) -> UiAppearance {
        self.appearance
    }

    /// Adopts an explicitly resolved nearest scope, including an ordinary local override.
    ///
    /// Rejection leaves the previous appearance and live editor intact. Color-only updates
    /// reuse compatible editing layouts. Typography adoption reconfigures the original field,
    /// including its bounded history, instead of replacing it or publishing a domain edit.
    pub fn set_appearance(
        &mut self,
        appearance: UiAppearance,
        engine: &mut TextEngine,
    ) -> Result<(), StatusLabelError> {
        let metrics = appearance.control_metrics(UiControlSize::Regular)?;
        let field_appearance = field_appearance(appearance, metrics, &self.field)?;
        let icon = if appearance.palette().accent == self.appearance.palette().accent {
            self.icon.clone()
        } else {
            property_icon(appearance.palette().accent)?
        };
        self.field.reconfigure(engine, metrics.typography())?;
        self.field.set_appearance(field_appearance);
        self.icon = icon;
        self.appearance = appearance;
        Ok(())
    }

    /// Composes at an explicit logical outer width, without premeasuring child text.
    ///
    /// The shared property layout measures all four actual child subtrees and aligns their
    /// real baselines. The field's preferred intrinsic width is independent of its allocation.
    /// The shared icon/label layout places an ordinary decorative mesh against the text's
    /// actual baseline. Wide rows stay inline; narrow rows stack label/value and wrap the footer.
    /// The caller must allocate this width, then present the new geometry before point/IME
    /// input. Extremely narrow widths that cannot contain control padding are rejected.
    pub fn authored_node(&mut self, width: f32, status: &str) -> Result<UiNode, StatusLabelError> {
        let appearance = self.appearance;
        let metrics = appearance.control_metrics(UiControlSize::Regular)?;
        let inset = appearance.panel_inset();
        let content_width = width - 2.0 * inset;
        let minimum = metrics.padding().left() + metrics.padding().right() + 1.0;
        if !width.is_finite() || content_width < minimum {
            return Err(UiLayoutError::Rejected {
                message: format!(
                    "property row width {width} must provide at least {minimum} content units"
                ),
            }
            .into());
        }
        let label = appearance.text(&self.label, UiTextRole::Body, false)?;
        let status = appearance.text(status, UiTextRole::Caption, true)?;
        let gap = appearance.gap();
        let minimum_field = 8.0 * metrics.typography().logical_size() + minimum;
        self.field
            .set_appearance(field_appearance(appearance, metrics, &self.field)?);
        let field = self
            .field
            .authored_node(&self.root, UiStyle::flex(), minimum_field)?;
        let button = appearance
            .button(
                &self.button,
                self.button_state,
                UiButtonVariant::Secondary,
                UiControlSize::Regular,
                false,
            )?
            .authored_node(&self.button, &self.root, UiStyle::flex())?;
        let description = UiPropertyRow::new(minimum_field, gap)?;
        let layout = match &self.layout {
            Some((previous, layout)) if previous == &description => layout.clone(),
            _ => {
                let layout = UiCustomLayout::new(description);
                self.layout = Some((description, layout.clone()));
                layout
            }
        };
        let description = UiIconLabel::new(
            UiSize::new(12.0, 12.0).ok_or(UiLayoutError::InvalidGeometry)?,
            -10.0,
            gap,
        )?;
        let label_layout = match &self.label_layout {
            Some((previous, layout)) if previous == &description => layout.clone(),
            _ => {
                let layout = UiCustomLayout::new(description);
                self.label_layout = Some((description, layout.clone()));
                layout
            }
        };
        let palette = appearance.palette();
        Ok(UiNode::new(
            UiStyle::flex()
                .with_width(UiDimension::Px(width))
                .with_flex_shrink(0.0)
                .with_padding(UiEdges::all(inset)?)
                .with_overflow(UiOverflow::Clip),
        )
        .with_key(self.key.clone())
        .with_visual(
            UiVisual::solid(palette.panel)
                .with_border(UiBorder::new(
                    metrics.stroke(),
                    UiBrush::Solid(palette.border),
                )?)
                .with_radii(UiCornerRadii::all(metrics.radius())?),
        )
        .with_semantics(UiSemantics::new(UiSemanticRole::Group, &self.label)?)
        .with_custom_layout(layout)
        .with_children([
            UiNode::new(UiStyle::flex())
                .with_key(self.label_key.clone())
                .with_semantics(UiSemantics::new(UiSemanticRole::Group, &self.label)?)
                .with_custom_layout(label_layout)
                .with_children([
                    UiNode::new(UiStyle::flex())
                        .with_key(self.icon_key.clone())
                        .with_mesh(self.icon.clone()),
                    UiNode::new(UiStyle::flex())
                        .with_key(self.label_text_key.clone())
                        .with_text(label),
                ]),
            field,
            button,
            UiNode::new(UiStyle::flex())
                .with_key(self.status_key.clone())
                .with_text(status),
        ]))
    }
}

/// A caller-designed notched bookmark, captured once as ordinary local triangle geometry.
fn property_icon(color: UiColor) -> Result<UiMesh, StatusLabelError> {
    let vertices = [
        (0.0, 0.0),
        (12.0, 0.0),
        (12.0, 12.0),
        (6.0, 8.0),
        (0.0, 12.0),
    ]
    .into_iter()
    .map(|(x, y)| UiMeshVertex::new(UiPoint::new(x, y), color))
    .collect();
    Ok(UiMesh::new(vertices, vec![0, 1, 3, 1, 2, 3, 0, 3, 4])?)
}

fn field_appearance(
    appearance: UiAppearance,
    metrics: UiControlMetrics,
    field: &UiTextField,
) -> Result<UiTextFieldAppearance, StatusLabelError> {
    let palette = appearance.palette();
    let border = if field.enabled() && field.input_token().is_some() {
        palette.focus
    } else {
        palette.border
    };
    Ok(UiTextFieldAppearance {
        padding: metrics.padding(),
        foreground: UiBrush::Solid(if field.enabled() {
            palette.text
        } else {
            palette.disabled_text
        }),
        visual: UiVisual::solid(palette.input)
            .with_border(UiBorder::new(metrics.stroke(), UiBrush::Solid(border))?)
            .with_radii(UiCornerRadii::all(metrics.radius())?),
        selection: UiBrush::Solid(palette.selection),
        caret: UiBrush::Solid(palette.focus),
    })
}
