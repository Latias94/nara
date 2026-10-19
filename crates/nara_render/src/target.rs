use nara_asset::Handle;
use nara_window::{PresentMode, WindowId, WindowResolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Extent2d {
    pub width: u32,
    pub height: u32,
}

impl Extent2d {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 {
            None
        } else {
            Some(Self { width, height })
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RenderImage2d {
    pub width: u32,
    pub height: u32,
}

impl RenderImage2d {
    /// Allocates the logical identity of one image target. Retain it across frame and extent changes.
    pub fn allocate_handle() -> Result<Handle<Self>, nara_asset::AssetError> {
        nara_asset::allocate_anonymous_handle()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderTarget {
    PrimaryWindow,
    Window(WindowId),
    Image(Handle<RenderImage2d>),
}

impl Default for RenderTarget {
    fn default() -> Self {
        Self::PrimaryWindow
    }
}

impl RenderTarget {
    #[must_use]
    pub fn window_id(self, primary_window_id: Option<WindowId>) -> Option<WindowId> {
        match self {
            Self::PrimaryWindow => Some(primary_window_id.unwrap_or(WindowId::PRIMARY)),
            Self::Window(window_id) => Some(window_id),
            Self::Image(_) => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ViewportRect {
    pub physical_x: u32,
    pub physical_y: u32,
    pub physical_width: u32,
    pub physical_height: u32,
}

impl ViewportRect {
    #[must_use]
    pub const fn new(
        physical_x: u32,
        physical_y: u32,
        physical_width: u32,
        physical_height: u32,
    ) -> Option<Self> {
        if physical_width == 0 || physical_height == 0 {
            None
        } else {
            Some(Self {
                physical_x,
                physical_y,
                physical_width,
                physical_height,
            })
        }
    }

    #[must_use]
    pub const fn from_extent(extent: Extent2d) -> Option<Self> {
        Self::new(0, 0, extent.width, extent.height)
    }

    #[must_use]
    pub const fn from_window_resolution(resolution: WindowResolution) -> Option<Self> {
        Self::new(0, 0, resolution.physical_width, resolution.physical_height)
    }

    #[must_use]
    pub const fn extent(&self) -> Extent2d {
        Extent2d {
            width: self.physical_width,
            height: self.physical_height,
        }
    }
}

/// Immutable window state admitted for one render-frame submission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderWindowPacket {
    pub id: WindowId,
    pub resolution: WindowResolution,
    pub present_mode: PresentMode,
}

/// Immutable image target admitted for one render-frame submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderImagePacket {
    pub(crate) image: Handle<RenderImage2d>,
    pub(crate) extent: Extent2d,
}

impl RenderImagePacket {
    #[must_use]
    pub const fn image(self) -> Handle<RenderImage2d> {
        self.image
    }

    #[must_use]
    pub const fn extent(self) -> Extent2d {
        self.extent
    }
}

/// The one physical target owned by a render frame transaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderFrameTarget {
    Window(RenderWindowPacket),
    Image(RenderImagePacket),
}

impl RenderFrameTarget {
    #[must_use]
    pub const fn extent(self) -> Extent2d {
        match self {
            Self::Window(window) => Extent2d {
                width: window.resolution.physical_width,
                height: window.resolution.physical_height,
            },
            Self::Image(image) => image.extent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_target_identity_is_distinct_per_view_and_stable_during_resize() {
        let first = RenderImage2d::allocate_handle().unwrap();
        let second = RenderImage2d::allocate_handle().unwrap();
        let initial =
            crate::__private::editor_viewport_image_target(first, Extent2d::new(64, 64).unwrap())
                .unwrap();
        let resized =
            crate::__private::editor_viewport_image_target(first, Extent2d::new(128, 64).unwrap())
                .unwrap();

        assert_ne!(first.id(), second.id());
        assert_eq!(initial.image(), resized.image());
        assert_ne!(initial.extent(), resized.extent());
    }
}
