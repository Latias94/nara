use nara_ui_kernel::{
    UiAvailableSpace, UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError,
    UiLayoutOutput, UiRect, UiSize,
};

/// A decorative icon followed by a text-bearing child, using its actual first baseline.
///
/// Supply two ordinary children: captured icon geometry and the label subtree. The icon's
/// logical envelope and top offset from the text baseline are explicit optical inputs;
/// geometry is not scaled or tessellated here. This adds no hit target, action or font policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiIconLabel {
    icon_size: UiSize,
    baseline_offset: f32,
    gap: f32,
}

impl UiIconLabel {
    pub fn new(icon_size: UiSize, baseline_offset: f32, gap: f32) -> Result<Self, UiLayoutError> {
        if icon_size.width() <= 0.0
            || icon_size.height() <= 0.0
            || !baseline_offset.is_finite()
            || !gap.is_finite()
            || gap < 0.0
        {
            return Err(UiLayoutError::InvalidGeometry);
        }
        Ok(Self {
            icon_size,
            baseline_offset,
            gap,
        })
    }
}

impl UiLayout for UiIconLabel {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 2 {
            return Err(UiLayoutError::Rejected {
                message: "an icon label requires an icon and a text-bearing label".into(),
            });
        }
        let inset = self.icon_size.width() + self.gap;
        let label = match constraints.width {
            UiAvailableSpace::Definite(width) => {
                children.measure_at_width(1, (width - inset).max(0.0))?
            }
            intrinsic => children.measure(
                1,
                UiLayoutConstraints {
                    width: intrinsic,
                    height: UiAvailableSpace::MaxContent,
                },
            )?,
        };
        let label_baseline = label
            .first_baseline()
            .ok_or_else(|| UiLayoutError::Rejected {
                message: "an icon label needs an actual text baseline".into(),
            })?;
        let label_width = match constraints.width {
            UiAvailableSpace::Definite(width) => (width - inset).max(0.0),
            _ => label.size().width(),
        };
        let label_y = (-(label_baseline + self.baseline_offset)).max(0.0);
        let baseline = label_y + label_baseline;
        let icon_y = baseline + self.baseline_offset;
        children.place(
            0,
            UiRect::from_origin_size(0.0, icon_y, self.icon_size.width(), self.icon_size.height())
                .ok_or(UiLayoutError::InvalidGeometry)?,
        )?;
        children.place(
            1,
            UiRect::from_origin_size(inset, label_y, label_width, label.size().height())
                .ok_or(UiLayoutError::InvalidGeometry)?,
        )?;
        UiLayoutOutput::new(
            UiSize::new(
                match constraints.width {
                    UiAvailableSpace::Definite(width) => width,
                    _ => inset + label_width,
                },
                (label_y + label.size().height()).max(icon_y + self.icon_size.height()),
            )
            .ok_or(UiLayoutError::InvalidGeometry)?,
        )
        .with_first_baseline(baseline)
    }
}
