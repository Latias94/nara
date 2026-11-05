use nara_ui_kernel::{
    UiAvailableSpace, UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError,
    UiLayoutMeasurement, UiLayoutOutput, UiRect, UiSize,
};

/// A measured label, editor, action and status, supplied as four ordinary child subtrees.
///
/// Wide rows reserve natural label/action/status widths and allocate the remainder to the
/// editor. Narrow rows stack label and editor, then wrap the action/status footer. Actual
/// child baselines determine alignment. This description owns no field state or callbacks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPropertyRow {
    minimum_inline_editor_width: f32,
    gap: f32,
}

impl UiPropertyRow {
    pub fn new(minimum_inline_editor_width: f32, gap: f32) -> Result<Self, UiLayoutError> {
        if !minimum_inline_editor_width.is_finite()
            || minimum_inline_editor_width <= 0.0
            || !gap.is_finite()
            || gap < 0.0
        {
            return Err(UiLayoutError::InvalidGeometry);
        }
        Ok(Self {
            minimum_inline_editor_width,
            gap,
        })
    }
}

impl UiLayout for UiPropertyRow {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 4 {
            return Err(UiLayoutError::Rejected {
                message: "a property row requires label, editor, action and status".into(),
            });
        }
        let mut minimum = [0.0_f32; 4];
        let mut natural = [0.0_f32; 4];
        for index in 0..4 {
            minimum[index] = children.min_content_width(index)?;
            natural[index] = children
                .measure(index, UiLayoutConstraints::MAX_CONTENT)?
                .size()
                .width();
        }
        let inline_minimum = natural[0]
            + minimum[1].max(self.minimum_inline_editor_width)
            + natural[2]
            + natural[3]
            + 3.0 * self.gap;
        let width = match constraints.width {
            UiAvailableSpace::Definite(width) => width,
            UiAvailableSpace::MinContent => minimum.into_iter().fold(0.0_f32, f32::max),
            UiAvailableSpace::MaxContent => {
                natural[0]
                    + natural[1].max(self.minimum_inline_editor_width)
                    + natural[2]
                    + natural[3]
                    + 3.0 * self.gap
            }
        };
        let inline = width >= inline_minimum;
        let action_width = natural[2].min(width);
        let footer_inline = action_width + self.gap + minimum[3] <= width;
        let widths = if inline {
            [
                natural[0],
                width - natural[0] - natural[2] - natural[3] - 3.0 * self.gap,
                natural[2],
                natural[3],
            ]
        } else {
            [
                width,
                width,
                action_width,
                if footer_inline {
                    width - action_width - self.gap
                } else {
                    width
                },
            ]
        };
        let mut measured = Vec::with_capacity(4);
        for (index, &width) in widths.iter().enumerate() {
            measured.push(children.measure_at_width(index, width)?);
        }
        let (height, baseline) = if inline {
            place_line(children, &measured, &widths, &[0, 1, 2, 3], self.gap, 0.0)?
        } else {
            let (label_height, baseline) =
                place_line(children, &measured, &widths, &[0], self.gap, 0.0)?;
            let field_y = label_height + self.gap;
            let (field_height, _) =
                place_line(children, &measured, &widths, &[1], self.gap, field_y)?;
            let footer_y = field_y + field_height + self.gap;
            let height = if footer_inline {
                footer_y + place_line(children, &measured, &widths, &[2, 3], self.gap, footer_y)?.0
            } else {
                let (action_height, _) =
                    place_line(children, &measured, &widths, &[2], self.gap, footer_y)?;
                let status_y = footer_y + action_height + self.gap;
                status_y + place_line(children, &measured, &widths, &[3], self.gap, status_y)?.0
            };
            (height, baseline)
        };
        let output =
            UiLayoutOutput::new(UiSize::new(width, height).ok_or(UiLayoutError::InvalidGeometry)?);
        baseline.map_or(Ok(output), |baseline| output.with_first_baseline(baseline))
    }
}

fn place_line(
    children: &mut UiLayoutChildren<'_>,
    measured: &[UiLayoutMeasurement],
    widths: &[f32; 4],
    slots: &[usize],
    gap: f32,
    y: f32,
) -> Result<(f32, Option<f32>), UiLayoutError> {
    let baseline = slots
        .iter()
        .filter_map(|&index| measured[index].first_baseline())
        .reduce(f32::max);
    let mut x = 0.0;
    let mut height = 0.0_f32;
    for &index in slots {
        let child = measured[index];
        let offset = baseline
            .zip(child.first_baseline())
            .map_or(0.0, |(line, child)| line - child);
        children.place(
            index,
            UiRect::from_origin_size(x, y + offset, widths[index], child.size().height())
                .ok_or(UiLayoutError::InvalidGeometry)?,
        )?;
        height = height.max(offset + child.size().height());
        x += widths[index] + gap;
    }
    Ok((height, baseline))
}
