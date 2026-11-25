use nara_app::{FixedTime, RealTime, RenderTime, RuntimeTimeSettings, VirtualTime};

/// Immutable App time captured with the extracted views.
///
/// This sample retains the original clock values, including exact durations and fixed-step
/// policy. The App's real frame counter is independent of the render packet's frame index:
/// an exact fixed step advances App time without extracting another set of views.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapturedRenderTime {
    real: RealTime,
    virtual_time: VirtualTime,
    render: RenderTime,
    settings: RuntimeTimeSettings,
}

impl CapturedRenderTime {
    pub(crate) const fn new(
        real: RealTime,
        virtual_time: VirtualTime,
        render: RenderTime,
        settings: RuntimeTimeSettings,
    ) -> Self {
        Self {
            real,
            virtual_time,
            render,
            settings,
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
        self.render.fixed()
    }

    #[must_use]
    pub const fn render(&self) -> RenderTime {
        self.render
    }

    /// Captured control settings, which can already request running during an elapsed Resume.
    /// Clock deltas describe the interval that was rendered.
    #[must_use]
    pub const fn settings(&self) -> RuntimeTimeSettings {
        self.settings
    }
}
