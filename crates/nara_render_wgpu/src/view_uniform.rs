//! Immutable stock View projection and bounded GPU clock representation.

use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use nara_app::{RealTime, RenderTime, RuntimeTimeSettings, VirtualTime};
use nara_render::{CapturedRenderTime, ExtractedView};

pub(crate) const VIEW_UNIFORM_BYTES: u64 = nara_material::__STOCK_VIEW_UNIFORM_BYTES as u64;
const TIME_PERIOD_SECONDS: u64 = 3_600;

/// Exact WGSL `NaraView` storage, captured before native preparation.
///
/// Matrices use three padded columns. Elapsed seconds are reduced with integer arithmetic;
/// the complete clocks remain in the original CPU capture. The final representable float below
/// the period prevents rounding a late subsecond remainder into the following period.
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct WgpuViewUniform {
    pub(crate) world_to_clip: [[f32; 4]; 3],
    pub(crate) clip_to_world: [[f32; 4]; 3],
    pub(crate) viewport: [f32; 4],
    pub(crate) real_elapsed: f32,
    pub(crate) real_delta: f32,
    pub(crate) virtual_elapsed: f32,
    pub(crate) virtual_delta: f32,
    pub(crate) fixed_elapsed: f32,
    pub(crate) fixed_delta: f32,
    pub(crate) fixed_timestep: f32,
    pub(crate) interpolation_alpha: f32,
    pub(crate) time_scale: f32,
    pub(crate) clock_available: u32,
    pub(crate) paused: u32,
    pub(crate) spatial_interpolated: u32,
    pub(crate) frame: [u32; 2],
    pub(crate) fixed_tick: [u32; 2],
    pub(crate) real_period: [u32; 2],
    pub(crate) virtual_period: [u32; 2],
    pub(crate) fixed_period: [u32; 2],
    pub(crate) _reserved0: [u32; 2],
    pub(crate) virtual_frame: [u32; 2],
    pub(crate) fixed_steps: u32,
    pub(crate) _reserved1: u32,
}

const _: () = assert!(std::mem::size_of::<WgpuViewUniform>() as u64 == VIEW_UNIFORM_BYTES);

impl WgpuViewUniform {
    pub(crate) fn capture(view: &ExtractedView, time: Option<CapturedRenderTime>) -> Self {
        let uniform = Self {
            world_to_clip: padded_columns(view.mapping.world_to_clip_matrix().to_cols_array_2d()),
            clip_to_world: padded_columns(view.mapping.clip_to_world_matrix().to_cols_array_2d()),
            viewport: [
                view.viewport.physical_x as f32,
                view.viewport.physical_y as f32,
                view.viewport.physical_width as f32,
                view.viewport.physical_height as f32,
            ],
            ..Self::zeroed()
        };
        match time {
            Some(time) => uniform.with_clocks(
                time.real(),
                time.virtual_time(),
                time.render(),
                time.settings(),
                time.spatial_interpolated(),
            ),
            None => uniform,
        }
    }

    fn with_clocks(
        mut self,
        real: RealTime,
        virtual_time: VirtualTime,
        render: RenderTime,
        settings: RuntimeTimeSettings,
        spatial_interpolated: bool,
    ) -> Self {
        let fixed = render.fixed();
        (self.real_elapsed, self.real_period) = period_time(real.elapsed);
        (self.virtual_elapsed, self.virtual_period) = period_time(virtual_time.elapsed);
        (self.fixed_elapsed, self.fixed_period) = period_time(fixed.elapsed());
        self.real_delta = real.delta.as_secs_f32();
        self.virtual_delta = virtual_time.delta.as_secs_f32();
        self.fixed_delta = fixed.delta().as_secs_f32();
        self.fixed_timestep = fixed.timestep().as_secs_f32();
        self.interpolation_alpha = render.interpolation_alpha();
        self.time_scale = settings.time_scale();
        self.clock_available = 1;
        self.paused = u32::from(settings.paused());
        self.spatial_interpolated = u32::from(spatial_interpolated);
        self.frame = counter_words(real.frame);
        self.virtual_frame = counter_words(virtual_time.frame);
        self.fixed_tick = counter_words(fixed.tick());
        self.fixed_steps = fixed.steps_this_frame();
        self
    }

    #[cfg(test)]
    pub(crate) fn manual(viewport: nara_render::ViewportRect) -> Self {
        Self {
            world_to_clip: padded_columns([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
            clip_to_world: padded_columns([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
            viewport: [
                viewport.physical_x as f32,
                viewport.physical_y as f32,
                viewport.physical_width as f32,
                viewport.physical_height as f32,
            ],
            ..Self::zeroed()
        }
    }
}

fn padded_columns(columns: [[f32; 3]; 3]) -> [[f32; 4]; 3] {
    columns.map(|[x, y, z]| [x, y, z, 0.0])
}

fn counter_words(value: u64) -> [u32; 2] {
    [value as u32, (value >> 32) as u32]
}

fn period_time(elapsed: Duration) -> (f32, [u32; 2]) {
    let remainder = Duration::new(
        elapsed.as_secs() % TIME_PERIOD_SECONDS,
        elapsed.subsec_nanos(),
    );
    let maximum = f32::from_bits((TIME_PERIOD_SECONDS as f32).to_bits() - 1);
    (
        remainder.as_secs_f32().min(maximum),
        counter_words(elapsed.as_secs() / TIME_PERIOD_SECONDS),
    )
}

#[cfg(test)]
mod tests;
