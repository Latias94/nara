//! Backend-independent window data and events.
//!
//! The `runtime` feature enables App/ECS integration and native handle ownership.

use std::collections::{BTreeMap, btree_map::Values};

#[cfg(feature = "runtime")]
use nara_ecs::{Component, Resource};

#[cfg(feature = "runtime")]
pub mod backend;
#[cfg(feature = "runtime")]
mod runtime;

#[cfg(feature = "runtime")]
pub use runtime::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WindowId(u64);

impl Default for WindowId {
    fn default() -> Self {
        Self::PRIMARY
    }
}

impl WindowId {
    pub const PRIMARY: Self = Self(1);

    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WindowSafeAreaInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Default for WindowSafeAreaInsets {
    fn default() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }
}

impl WindowSafeAreaInsets {
    #[must_use]
    pub fn new(left: f32, top: f32, right: f32, bottom: f32) -> Option<Self> {
        [left, top, right, bottom]
            .into_iter()
            .all(|value| value.is_finite() && value >= 0.0)
            .then_some(Self {
                left,
                top,
                right,
                bottom,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct WindowResolution {
    pub physical_width: u32,
    pub physical_height: u32,
    pub scale_factor: f64,
    pub safe_area_insets: WindowSafeAreaInsets,
}

impl Default for WindowResolution {
    fn default() -> Self {
        Self {
            physical_width: 1280,
            physical_height: 720,
            scale_factor: 1.0,
            safe_area_insets: WindowSafeAreaInsets::default(),
        }
    }
}

impl WindowResolution {
    #[must_use]
    pub fn new(physical_width: u32, physical_height: u32) -> Self {
        Self {
            physical_width,
            physical_height,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    #[must_use]
    pub fn with_safe_area_insets(mut self, safe_area_insets: WindowSafeAreaInsets) -> Self {
        self.safe_area_insets = safe_area_insets;
        self
    }

    #[must_use]
    pub const fn is_zero_sized(&self) -> bool {
        self.physical_width == 0 || self.physical_height == 0
    }
}

/// Native mode requested when the desktop adapter creates a window.
///
/// This startup setting does not promise that later component changes reconfigure
/// an existing native window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindowMode {
    /// A normal window with the configured physical client extent.
    #[default]
    Windowed,
    /// A borderless window covering the primary monitor without changing its video mode.
    /// If the adapter cannot identify a primary monitor, the operating system chooses one.
    BorderlessFullscreen,
    /// Exclusive fullscreen on the primary monitor at the configured physical extent.
    ///
    /// The desktop adapter chooses the matching video mode with the highest refresh
    /// rate, then the highest bit depth. Missing primary monitors or matching video
    /// modes fail startup; this request does not silently become borderless or windowed.
    Fullscreen,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PresentMode {
    #[default]
    AutoVsync,
    AutoNoVsync,
    Fifo,
    Immediate,
    Mailbox,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "runtime", derive(Component))]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Window {
    pub id: WindowId,
    pub title: String,
    pub resolution: WindowResolution,
    pub mode: WindowMode,
    pub present_mode: PresentMode,
    pub resizable: bool,
    pub focused: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            id: WindowId::PRIMARY,
            title: "nara".to_owned(),
            resolution: WindowResolution::default(),
            mode: WindowMode::default(),
            present_mode: PresentMode::default(),
            resizable: true,
            focused: true,
        }
    }
}

impl Window {
    #[must_use]
    pub fn new(title: impl Into<String>, resolution: WindowResolution) -> Self {
        Self {
            title: title.into(),
            resolution,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_id(mut self, id: WindowId) -> Self {
        self.id = id;
        self
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "runtime", derive(Component))]
pub struct PrimaryWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "runtime", derive(Resource))]
pub struct PrimaryWindowId(pub WindowId);

impl Default for PrimaryWindowId {
    fn default() -> Self {
        Self(WindowId::PRIMARY)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WindowEvent {
    Created {
        window_id: WindowId,
    },
    CloseRequested {
        window_id: WindowId,
    },
    Closed {
        window_id: WindowId,
    },
    Resized {
        window_id: WindowId,
        resolution: WindowResolution,
    },
    Focused {
        window_id: WindowId,
        focused: bool,
    },
    ScaleFactorChanged {
        window_id: WindowId,
        scale_factor: f64,
    },
    RedrawRequested {
        window_id: WindowId,
    },
}

impl WindowEvent {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        match self {
            Self::Created { window_id }
            | Self::CloseRequested { window_id }
            | Self::Closed { window_id }
            | Self::Resized { window_id, .. }
            | Self::Focused { window_id, .. }
            | Self::ScaleFactorChanged { window_id, .. }
            | Self::RedrawRequested { window_id } => *window_id,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "runtime", derive(Resource))]
pub struct WindowEvents {
    events: Vec<WindowEvent>,
}

impl WindowEvents {
    pub fn push(&mut self, event: WindowEvent) {
        self.events.push(event);
    }

    #[cfg(feature = "runtime")]
    fn clear(&mut self) {
        self.events.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[WindowEvent] {
        &self.events
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCloseRequest {
    window_id: WindowId,
    canceled: bool,
}

impl WindowCloseRequest {
    #[must_use]
    pub const fn window_id(&self) -> WindowId {
        self.window_id
    }

    #[must_use]
    pub const fn is_canceled(&self) -> bool {
        self.canceled
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "runtime", derive(Resource))]
pub struct WindowCloseRequests {
    requests: BTreeMap<WindowId, WindowCloseRequest>,
}

impl WindowCloseRequests {
    pub fn request(&mut self, window_id: WindowId) {
        self.requests.insert(
            window_id,
            WindowCloseRequest {
                window_id,
                canceled: false,
            },
        );
    }

    pub fn cancel(&mut self, window_id: WindowId) {
        if let Some(request) = self.requests.get_mut(&window_id) {
            request.canceled = true;
        }
    }

    #[must_use]
    pub fn is_requested(&self, window_id: WindowId) -> bool {
        self.requests.contains_key(&window_id)
    }

    #[must_use]
    pub fn is_canceled(&self, window_id: WindowId) -> bool {
        self.requests
            .get(&window_id)
            .is_some_and(WindowCloseRequest::is_canceled)
    }

    #[must_use]
    pub fn has_uncancelled(&self) -> bool {
        self.requests.values().any(|request| !request.is_canceled())
    }

    pub fn iter(&self) -> Values<'_, WindowId, WindowCloseRequest> {
        self.requests.values()
    }

    #[cfg(feature = "runtime")]
    fn clear(&mut self) {
        self.requests.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_has_non_zero_resolution() {
        let window = Window::default();

        assert_eq!(window.id, WindowId::PRIMARY);
        assert_eq!(window.title, "nara");
        assert!(!window.resolution.is_zero_sized());
    }

    #[test]
    fn zero_sized_resolution_is_detected() {
        assert!(WindowResolution::new(0, 720).is_zero_sized());
        assert!(WindowResolution::new(1280, 0).is_zero_sized());
        assert!(!WindowResolution::new(1280, 720).is_zero_sized());
    }
}
