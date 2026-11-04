//! A user-authored container with child measurement and placement, without layout-tree access.

use nara_ui_kernel::{
    UiAvailableSpace, UiLayout, UiLayoutChildren, UiLayoutConstraints, UiLayoutError,
    UiLayoutOutput, UiRect, UiSize,
};

pub struct MeasuredColumn {
    pub gap: f32,
}

impl UiLayout for MeasuredColumn {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        if !self.gap.is_finite() || self.gap < 0.0 {
            return Err(UiLayoutError::InvalidGeometry);
        }
        let mut width = 0.0_f32;
        let mut height = 0.0_f32;
        let mut baseline = None;
        for child in 0..children.len() {
            let measured = children.measure(
                child,
                UiLayoutConstraints {
                    width: constraints.width,
                    height: UiAvailableSpace::MaxContent,
                },
            )?;
            if child == 0 {
                baseline = measured.first_baseline();
            } else {
                height += self.gap;
            }
            children.place(
                child,
                UiRect::from_origin_size(
                    0.0,
                    height,
                    measured.size().width(),
                    measured.size().height(),
                )
                .ok_or(UiLayoutError::InvalidGeometry)?,
            )?;
            width = width.max(measured.size().width());
            height += measured.size().height();
        }
        let output =
            UiLayoutOutput::new(UiSize::new(width, height).ok_or(UiLayoutError::InvalidGeometry)?);
        match baseline {
            Some(baseline) => output.with_first_baseline(baseline),
            None => Ok(output),
        }
    }
}
