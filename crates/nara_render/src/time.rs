use nara_app::{FixedTime, RealTime, RenderTime, RuntimeTimeSettings, VirtualTime};
use nara_transform::SpatialSnapshotStamp;

/// Immutable App time and spatial presentation facts captured with the extracted views.
///
/// This sample retains the original clock values, including exact durations and fixed-step
/// policy. The App's real frame counter is independent of the render packet's frame index:
/// an exact fixed step advances App time without extracting another set of views.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturedRenderTime {
    real: RealTime,
    virtual_time: VirtualTime,
    fixed: FixedTime,
    render: RenderTime,
    settings: RuntimeTimeSettings,
    spatial_stamp: SpatialSnapshotStamp,
    spatial_interpolated: bool,
}

impl CapturedRenderTime {
    pub(crate) const fn new(
        real: RealTime,
        virtual_time: VirtualTime,
        fixed: FixedTime,
        render: RenderTime,
        settings: RuntimeTimeSettings,
        spatial_stamp: SpatialSnapshotStamp,
        spatial_interpolated: bool,
    ) -> Self {
        Self {
            real,
            virtual_time,
            fixed,
            render,
            settings,
            spatial_stamp,
            spatial_interpolated,
        }
    }

    #[must_use]
    pub const fn real(&self) -> RealTime {
        self.real
    }

    #[must_use]
    pub const fn virtual_time(&self) -> VirtualTime {
        self.virtual_time
    }

    #[must_use]
    pub const fn fixed(&self) -> FixedTime {
        self.fixed
    }

    #[must_use]
    pub const fn render(&self) -> RenderTime {
        self.render
    }

    /// Captured control settings, which can already request running during an elapsed Resume.
    /// Actual clock deltas and spatial interpolation describe the interval that was rendered.
    #[must_use]
    pub const fn settings(&self) -> RuntimeTimeSettings {
        self.settings
    }

    /// The validated presented-spatial publication used by these views.
    #[must_use]
    pub const fn spatial_stamp(&self) -> SpatialSnapshotStamp {
        self.spatial_stamp
    }

    /// Whether the spatial owner actually selected an interpolated presentation payload.
    ///
    /// A retained nonzero render alpha does not imply spatial interpolation, including while
    /// paused or when no captured transform pair differs.
    #[must_use]
    pub const fn spatial_interpolated(&self) -> bool {
        self.spatial_interpolated
    }
}
