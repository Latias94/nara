use nara_ui_kernel::{
    UiAvailableSpace, UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError,
    UiLayoutOutput, UiRect, UiSize,
};

/// Positions one measured child within the current root's logical content space.
/// A child taller than the available allocation is clipped by its authored overflow policy;
/// its owner supplies scrolling or paging so every offered action remains reachable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiAnchoredPopup {
    anchor: UiRect,
    preferred_width: f32,
    margin: f32,
    gap: f32,
}

impl UiAnchoredPopup {
    pub fn new(
        anchor: UiRect,
        preferred_width: f32,
        margin: f32,
        gap: f32,
    ) -> Result<Self, UiLayoutError> {
        if !preferred_width.is_finite()
            || preferred_width <= 0.0
            || !margin.is_finite()
            || margin < 0.0
            || !gap.is_finite()
            || gap < 0.0
        {
            return Err(UiLayoutError::InvalidGeometry);
        }
        Ok(Self {
            anchor,
            preferred_width,
            margin,
            gap,
        })
    }
}

impl UiLayout for UiAnchoredPopup {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if children.len() != 1 {
            return Err(UiLayoutError::InvalidGeometry);
        }
        let (UiAvailableSpace::Definite(width), UiAvailableSpace::Definite(height)) =
            (constraints.width, constraints.height)
        else {
            return Err(UiLayoutError::InvalidConstraints);
        };
        let available_width = (width - 2.0 * self.margin).max(0.0);
        let available_height = (height - 2.0 * self.margin).max(0.0);
        if available_width <= 0.0 || available_height <= 0.0 {
            children.hide(0)?;
            return Ok(UiLayoutOutput::new(
                UiSize::new(width, height).ok_or(UiLayoutError::InvalidGeometry)?,
            ));
        }
        let popup_width = self.preferred_width.min(available_width);
        let measured = children.measure(
            0,
            UiLayoutConstraints {
                width: UiAvailableSpace::Definite(popup_width),
                height: UiAvailableSpace::MaxContent,
            },
        )?;
        let popup_height = measured.size().height().min(available_height);
        let x = self.anchor.max().x().min(width - self.margin) - popup_width;
        let below = self.anchor.max().y() + self.gap;
        let above = self.anchor.origin().y() - self.gap - popup_height;
        let y = if below + popup_height <= height - self.margin {
            below
        } else if above >= self.margin {
            above
        } else {
            below
        }
        .clamp(self.margin, height - self.margin - popup_height);
        children.place(
            0,
            UiRect::from_origin_size(
                x.clamp(self.margin, width - self.margin - popup_width),
                y,
                popup_width,
                popup_height,
            )
            .ok_or(UiLayoutError::InvalidGeometry)?,
        )?;
        Ok(UiLayoutOutput::new(
            UiSize::new(width, height).ok_or(UiLayoutError::InvalidGeometry)?,
        ))
    }
}
