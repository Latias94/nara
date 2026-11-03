use super::*;
use nara_ui_kernel::UiPositionMode;

impl OwnedInspector {
    pub(in crate::frontend::imgui) fn scroll(
        &mut self,
        frame: &Arc<UiFrame>,
        delta: f32,
    ) -> Result<bool, OwnedInspectorError> {
        self.retained(frame)?;
        if !delta.is_finite() {
            return Err(OwnedInspectorError::InvalidScroll);
        }
        let limit = self.scroll_limit(frame);
        let next = (self.scroll_pixels + delta.clamp(-limit, limit)).clamp(0.0, limit);
        let changed = next != self.scroll_pixels;
        self.scroll_pixels = next;
        Ok(changed)
    }

    pub(super) fn clamp_scroll(&mut self, frame: &UiFrame) -> bool {
        let next = self.scroll_pixels.min(self.scroll_limit(frame));
        let changed = next != self.scroll_pixels;
        self.scroll_pixels = next;
        changed
    }

    fn scroll_limit(&self, frame: &UiFrame) -> f32 {
        let height = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&self.content_key))
            .map_or(0.0, |layout| layout.rect().size().height());
        (height - frame.environment().logical_size().height()).max(0.0)
    }

    pub(super) fn viewport_snapshot(
        &self,
        nodes: Vec<UiNode>,
    ) -> Result<UiAuthoringSnapshot, OwnedInspectorError> {
        let content = UiNode::new(
            UiStyle::flex()
                .with_position_mode(UiPositionMode::Absolute)
                .with_left(UiDimension::Px(0.0))
                .with_top(UiDimension::Px(-self.scroll_pixels))
                .with_width(UiDimension::Percent(1.0))
                .with_flex_shrink(0.0)
                .with_padding(UiEdges::all(PANEL_INSET)?)
                .with_gap(8.0),
        )
        .with_key(
            self.content_key
                .local_key()
                .cloned()
                .ok_or(OwnedInspectorError::UnknownControl)?,
        )
        .with_children(nodes);
        let root = UiRoot::new(
            self.root.clone(),
            UiLayer::Base,
            UiStyle::fill().with_overflow(UiOverflow::Clip),
        )
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_child(content);
        Ok(UiAuthoringSnapshotBuilder::new(self.next_snapshot)
            .with_root(root)
            .build()?)
    }
}
