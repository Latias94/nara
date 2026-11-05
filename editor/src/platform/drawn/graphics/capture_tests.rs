use std::sync::Arc;

use nara_render_wgpu::{WgpuCompositeColor, WgpuImageDisplay, WgpuWorldColor};
use nara_stock_composition_consumer::{oracle::Image, readback};
use nara_ui_kernel::UiFrame;

use super::Graphics;
use crate::workspace::EditorPanelInstanceId;

impl Graphics {
    pub(in crate::platform::drawn) fn capture_panel_pixels_for_test(
        &mut self,
        slot: EditorPanelInstanceId,
        frame: &Arc<UiFrame>,
    ) -> Result<(WgpuImageDisplay, Image), String> {
        self.check_open()?;
        let panel = self
            .panels
            .get(&slot)
            .ok_or("The captured panel has no original graphics lifetime")?;
        let (current, display) = panel
            .current
            .as_ref()
            .ok_or("The captured panel has no original GPU output")?;
        if !Arc::ptr_eq(current, frame)
            || panel.pending.is_some()
            || panel.retry.is_some()
            || !panel.returned.is_empty()
            || !display.output().is_current()
        {
            return Err("Capture requires the current completed original panel output".into());
        }
        let accepted = panel
            .presenter
            .journal()
            .latest(frame.environment().surface_id())
            .map_err(|error| error.to_string())?;
        if !Arc::ptr_eq(accepted.frame(), frame)
            || accepted.receipt().frame_revision() != frame.revision()
            || accepted.receipt().presentation_epoch() == 0
        {
            return Err("Capture requires the exact original presented UI frame".into());
        }
        let extent = display.output().extent();
        if extent.width == 0
            || extent.height == 0
            || extent.width > 2048
            || extent.height > 2048
            || !matches!(
                display.output().format(),
                wgpu::TextureFormat::Rgba8Unorm
                    | wgpu::TextureFormat::Rgba8UnormSrgb
                    | wgpu::TextureFormat::Bgra8Unorm
                    | wgpu::TextureFormat::Bgra8UnormSrgb
            )
        {
            return Err("Capture requires an RGBA8/BGRA8 panel of at most 2048 by 2048".into());
        }
        let display = display.clone();
        let image = match display.output().texture::<WgpuCompositeColor>() {
            Ok(output) => readback::read(&mut self.host, &output),
            Err(composite) => {
                let output = display
                    .output()
                    .texture::<WgpuWorldColor>()
                    .map_err(|world| {
                        format!("Capture requires a stock panel output: {composite}; {world}")
                    })?;
                readback::read(&mut self.host, &output)
            }
        };
        Ok((display, image))
    }
}
