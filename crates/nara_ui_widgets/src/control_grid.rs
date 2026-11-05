use nara_ui_kernel::{
    UiAvailableSpace, UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError,
    UiLayoutMeasurement, UiLayoutOutput, UiRect, UiSize,
};

/// A measured equal-width control group that wraps as its available content width narrows.
/// The declared minimum is enlarged by actual child min-content widths under current typography.
/// If even one cell cannot fit, it receives the available width and retains its overflow policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiControlGrid {
    minimum_cell_width: f32,
    gap: f32,
}

impl UiControlGrid {
    pub fn new(minimum_cell_width: f32, gap: f32) -> Result<Self, UiLayoutError> {
        if !minimum_cell_width.is_finite()
            || minimum_cell_width <= 0.0
            || !gap.is_finite()
            || gap < 0.0
        {
            return Err(UiLayoutError::InvalidGeometry);
        }
        Ok(Self {
            minimum_cell_width,
            gap,
        })
    }
}

impl UiLayout for UiControlGrid {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.is_empty() {
            return output(0.0, 0.0, None);
        }
        let count = children.len();
        let mut minimum_cell_width = self.minimum_cell_width;
        for index in 0..count {
            minimum_cell_width = minimum_cell_width.max(children.min_content_width(index)?);
        }
        let width = match constraints.width {
            UiAvailableSpace::Definite(width) => width,
            UiAvailableSpace::MinContent => minimum_cell_width,
            intrinsic => {
                let mut natural: f32 = self.minimum_cell_width;
                for index in 0..count {
                    let measured = children.measure(
                        index,
                        UiLayoutConstraints {
                            width: intrinsic,
                            height: UiAvailableSpace::MaxContent,
                        },
                    )?;
                    natural = natural.max(measured.size().width());
                }
                natural * count as f32 + self.gap * (count - 1) as f32
            }
        };
        if width <= 0.0 {
            for index in 0..count {
                children.hide(index)?;
            }
            return output(0.0, 0.0, None);
        }
        let columns = (((width + self.gap) / (minimum_cell_width + self.gap)).floor() as usize)
            .clamp(1, count);
        let cell_width = ((width - self.gap * (columns - 1) as f32) / columns as f32).max(0.0);
        let mut y = 0.0;
        let mut first_baseline = None;
        for start in (0..count).step_by(columns) {
            let end = (start + columns).min(count);
            let mut row = Vec::<UiLayoutMeasurement>::with_capacity(end - start);
            let mut baseline: Option<f32> = None;
            for index in start..end {
                let measured = children.measure_at_width(index, cell_width)?;
                if let Some(child_baseline) = measured.first_baseline() {
                    baseline = Some(baseline.map_or(child_baseline, |old| old.max(child_baseline)));
                }
                row.push(measured);
            }
            let mut height: f32 = 0.0;
            for (slot, measured) in row.iter().enumerate() {
                let offset = baseline
                    .zip(measured.first_baseline())
                    .map_or(0.0, |(row, child)| (row - child).max(0.0));
                let rect = UiRect::from_origin_size(
                    slot as f32 * (cell_width + self.gap),
                    y + offset,
                    cell_width,
                    measured.size().height(),
                )
                .ok_or(UiLayoutError::InvalidGeometry)?;
                children.place(start + slot, rect)?;
                height = height.max(offset + measured.size().height());
            }
            if start == 0 {
                first_baseline = baseline;
            }
            y += height;
            if end < count {
                y += self.gap;
            }
        }
        output(width, y, first_baseline)
    }
}

fn output(width: f32, height: f32, baseline: Option<f32>) -> Result<UiLayoutOutput, UiLayoutError> {
    let size = UiSize::new(width, height).ok_or(UiLayoutError::InvalidGeometry)?;
    let output = UiLayoutOutput::new(size);
    baseline.map_or(Ok(output), |baseline| output.with_first_baseline(baseline))
}
