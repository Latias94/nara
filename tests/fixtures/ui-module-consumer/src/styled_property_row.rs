//! An independent styled composite over public text, layout and controlled-widget interfaces.

use nara_text::{TextEngine, TextLayoutRequest, TextTypography};
use nara_ui_kernel::{
    UiActionId, UiAvailableSpace, UiBorder, UiBrush, UiCornerRadii, UiCustomLayout, UiDimension,
    UiDispatchBatch, UiEdges, UiKey, UiKeyAtom, UiLayout, UiLayoutChildren, UiLayoutConstraints,
    UiLayoutError, UiLayoutMeasurement, UiLayoutOutput, UiLocalKey, UiNode, UiOverflow, UiRect,
    UiRootKey, UiSemanticRole, UiSemantics, UiSize, UiStyle, UiVisual,
};
use nara_ui_widgets::{
    UiAppearance, UiButton, UiButtonState, UiButtonVariant, UiControlMetrics, UiControlSize,
    UiIntentBatch, UiTextField, UiTextFieldAppearance, UiTextRole,
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
    status_key: UiLocalKey,
    label: String,
    field: UiTextField,
    button: UiButton,
    button_state: UiButtonState,
    appearance: UiAppearance,
    layout: Option<(PropertyRowLayout, UiCustomLayout)>,
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
        // Length framing keeps ("a", "bc") distinct from ("ab", "c"). Kernel key admission
        // validates the complete scoped identities, including the caller's root, below.
        let item = format!("{}:{scope}{}", scope.len(), property.id);
        let child_key = |part: &str| -> Result<UiLocalKey, StatusLabelError> {
            let local = UiLocalKey::runtime(format!("independent.property.{part}"), &item)?;
            UiKey::scoped(root.clone(), local.clone())?;
            Ok(local)
        };
        let key = child_key("row")?;
        let label_key = child_key("label")?;
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
            status_key,
            label: property.label.to_owned(),
            field,
            button: UiButton::new(button_key, property.action_label, property.action)?,
            button_state: UiButtonState::default(),
            appearance,
            layout: None,
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
        self.field.reconfigure(engine, metrics.typography())?;
        self.field.set_appearance(field_appearance);
        self.appearance = appearance;
        Ok(())
    }

    /// Composes at an explicit logical outer width with the same fonts as `engine`.
    ///
    /// Public text measurement determines the field viewport before authoring it. The custom
    /// layout then measures all four actual child subtrees and aligns their real baselines.
    /// Wide rows keep all slots inline; narrow rows stack label/value and wrap the footer.
    /// The caller must allocate this width, then present the new geometry before point/IME
    /// input. Extremely narrow widths that cannot contain control padding are rejected.
    pub fn authored_node(
        &mut self,
        width: f32,
        status: &str,
        engine: &mut TextEngine,
    ) -> Result<UiNode, StatusLabelError> {
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
        let measure = |engine: &mut TextEngine, content: &str, typography: TextTypography| {
            engine
                .layout(&TextLayoutRequest::from_typography(content, typography))
                .map(|layout| layout.width().ceil())
        };
        let label_width = measure(engine, label.content(), label.typography())?;
        let status_width = measure(engine, status.content(), status.typography())?;
        let action_width = measure(engine, self.button.label(), metrics.typography())?
            + metrics.padding().left()
            + metrics.padding().right();
        let gap = appearance.gap();
        let minimum_field = 8.0 * metrics.typography().logical_size() + minimum;
        let inline =
            label_width + minimum_field + action_width + status_width + 3.0 * gap <= content_width;
        let footer_inline = action_width + gap + 4.0 * status.logical_size() <= content_width;
        let widths = if inline {
            [
                label_width,
                content_width - label_width - action_width - status_width - 3.0 * gap,
                action_width,
                status_width,
            ]
        } else {
            [
                content_width,
                content_width,
                action_width.min(content_width),
                if footer_inline {
                    content_width - action_width - gap
                } else {
                    content_width
                },
            ]
        };
        self.field
            .set_appearance(field_appearance(appearance, metrics, &self.field)?);
        let field = self
            .field
            .authored_node(&self.root, slot_style(widths[1]), widths[1])?;
        let button = appearance
            .button(
                &self.button,
                self.button_state,
                UiButtonVariant::Secondary,
                UiControlSize::Regular,
                false,
            )?
            .authored_node(&self.button, &self.root, slot_style(widths[2]))?;
        let description = PropertyRowLayout {
            content_width,
            widths,
            gap,
            inline,
            footer_inline,
        };
        let layout = match &self.layout {
            Some((previous, layout)) if previous == &description => layout.clone(),
            _ => {
                let layout = UiCustomLayout::new(description);
                self.layout = Some((description, layout.clone()));
                layout
            }
        };
        let palette = appearance.palette();
        Ok(UiNode::new(
            slot_style(width)
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
            UiNode::new(slot_style(widths[0]))
                .with_key(self.label_key.clone())
                .with_text(label),
            field,
            button,
            UiNode::new(slot_style(widths[3]))
                .with_key(self.status_key.clone())
                .with_text(status),
        ]))
    }
}

fn slot_style(width: f32) -> UiStyle {
    UiStyle::flex()
        .with_width(UiDimension::Px(width))
        .with_flex_shrink(0.0)
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

#[derive(Clone, Copy, PartialEq)]
struct PropertyRowLayout {
    content_width: f32,
    widths: [f32; 4],
    gap: f32,
    inline: bool,
    footer_inline: bool,
}

impl UiLayout for PropertyRowLayout {
    fn layout(
        &self,
        _constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 4 {
            return Err(UiLayoutError::Rejected {
                message: "a property row requires label, field, action and status".to_owned(),
            });
        }
        let mut measured = Vec::with_capacity(4);
        for (index, width) in self.widths.iter().enumerate() {
            measured.push(children.measure(
                index,
                UiLayoutConstraints {
                    width: UiAvailableSpace::Definite(*width),
                    height: UiAvailableSpace::MaxContent,
                },
            )?);
        }
        let (height, baseline) = if self.inline {
            self.place_line(children, &measured, &[0, 1, 2, 3], 0.0)?
        } else {
            let (label_height, baseline) = self.place_line(children, &measured, &[0], 0.0)?;
            let field_y = label_height + self.gap;
            let (field_height, _) = self.place_line(children, &measured, &[1], field_y)?;
            let footer_y = field_y + field_height + self.gap;
            let height = if self.footer_inline {
                footer_y + self.place_line(children, &measured, &[2, 3], footer_y)?.0
            } else {
                let (action_height, _) = self.place_line(children, &measured, &[2], footer_y)?;
                let status_y = footer_y + action_height + self.gap;
                status_y + self.place_line(children, &measured, &[3], status_y)?.0
            };
            (height, baseline)
        };
        let output = UiLayoutOutput::new(
            UiSize::new(self.content_width, height).ok_or(UiLayoutError::InvalidGeometry)?,
        );
        match baseline {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}

impl PropertyRowLayout {
    fn place_line(
        &self,
        children: &mut UiLayoutChildren<'_>,
        measured: &[UiLayoutMeasurement],
        slots: &[usize],
        y: f32,
    ) -> Result<(f32, Option<f32>), UiLayoutError> {
        let baseline = slots
            .iter()
            .filter_map(|index| measured[*index].first_baseline())
            .reduce(f32::max);
        let mut x = 0.0;
        let mut height = 0.0_f32;
        for &index in slots {
            let child = measured[index];
            let offset = match (baseline, child.first_baseline()) {
                (Some(line), Some(child)) => line - child,
                _ => 0.0,
            };
            children.place(
                index,
                UiRect::from_origin_size(x, y + offset, self.widths[index], child.size().height())
                    .ok_or(UiLayoutError::InvalidGeometry)?,
            )?;
            height = height.max(offset + child.size().height());
            x += self.widths[index] + self.gap;
        }
        Ok((height, baseline.map(|baseline| y + baseline)))
    }
}
