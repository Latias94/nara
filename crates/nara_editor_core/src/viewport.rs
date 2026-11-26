//! Headless interaction math for a 2D editor viewport.
//!
//! Panel positions are logical pixels relative to the pane's top-left corner. World Y increases
//! upward, and panel Y increases downward. The shell supplies pane-local pointer positions.

mod gizmo;
mod picking;
#[cfg(test)]
mod tests;

use nara_core::{Mat3, Vec2};

pub use gizmo::{
    Gizmo2D, GizmoAxes2D, GizmoDelta2D, GizmoDrag2D, GizmoGeometry2D, GizmoHandle, GizmoRing2D,
};
pub use picking::{LocalBounds2D, PickCandidate2D};

const FLIP_Y: Vec2 = Vec2::new(1.0, -1.0);

/// A position in pane-local logical pixels, including positions outside the pane during capture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPoint(pub Vec2);

/// A position in the 2D scene's world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldPoint(pub Vec2);

/// A finite, positive size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelSize(Vec2);

impl PanelSize {
    #[must_use]
    pub fn new(width: f32, height: f32) -> Option<Self> {
        let size = Vec2::new(width, height);
        (size.is_finite() && size.cmpgt(Vec2::ZERO).all()).then_some(Self(size))
    }

    #[must_use]
    pub const fn as_vec2(self) -> Vec2 {
        self.0
    }
}

/// A pan/zoom camera with uniform logical pixels per world unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCamera2D {
    panel_size: PanelSize,
    center: WorldPoint,
    zoom: f32,
}

impl ViewportCamera2D {
    pub const MIN_ZOOM: f32 = 1.0 / 64.0;
    pub const MAX_ZOOM: f32 = 64.0;

    /// Creates a camera with a finite center and positive, finite zoom clamped to the zoom range.
    #[must_use]
    pub fn new(panel_size: PanelSize, center: WorldPoint, zoom: f32) -> Option<Self> {
        (center.0.is_finite() && zoom.is_finite() && zoom > 0.0).then(|| Self {
            panel_size,
            center,
            zoom: zoom.clamp(Self::MIN_ZOOM, Self::MAX_ZOOM),
        })
    }

    #[must_use]
    pub const fn panel_size(self) -> PanelSize {
        self.panel_size
    }

    #[must_use]
    pub const fn center(self) -> WorldPoint {
        self.center
    }

    #[must_use]
    pub const fn zoom(self) -> f32 {
        self.zoom
    }

    /// Changes the logical extent while preserving the world center and zoom.
    pub fn resize(&mut self, panel_size: PanelSize) {
        self.panel_size = panel_size;
    }

    /// Tests the half-open pane rectangle, including the top and left edges.
    #[must_use]
    pub fn contains(self, point: PanelPoint) -> bool {
        point.0.cmpge(Vec2::ZERO).all() && point.0.cmplt(self.panel_size.0).all()
    }

    /// Projects a world position, including off-panel points. Returns `None` on non-finite input
    /// or result.
    #[must_use]
    pub fn world_to_panel(self, point: WorldPoint) -> Option<PanelPoint> {
        let offset = point.0.as_dvec2() - self.center.0.as_dvec2();
        let panel =
            offset * FLIP_Y.as_dvec2() * f64::from(self.zoom) + self.panel_size.0.as_dvec2() * 0.5;
        finite_vec2(panel.as_vec2()).map(PanelPoint)
    }

    /// Unprojects a logical pane position without clipping it to the pane.
    #[must_use]
    pub fn panel_to_world(self, point: PanelPoint) -> Option<WorldPoint> {
        let offset = point.0.as_dvec2() - self.panel_size.0.as_dvec2() * 0.5;
        let world = self.center.0.as_dvec2() + offset * FLIP_Y.as_dvec2() / f64::from(self.zoom);
        finite_vec2(world.as_vec2()).map(WorldPoint)
    }

    /// Projects into clip space, with the pane's top-left at `(-1, 1)`.
    ///
    /// The projection uses logical width and height independently of the target raster size.
    /// Returns `None` if its coefficients cannot be represented as a finite `Mat3`.
    #[must_use]
    pub fn world_to_clip_matrix(self) -> Option<Mat3> {
        let scale = self.panel_size.0.as_dvec2().recip() * (2.0 * f64::from(self.zoom));
        let translation = finite_vec2((-self.center.0.as_dvec2() * scale).as_vec2())?;
        let scale = finite_vec2(scale.as_vec2())?;
        scale
            .cmpgt(Vec2::ZERO)
            .all()
            .then(|| Mat3::from_translation(translation) * Mat3::from_scale(scale))
    }

    /// Moves scene content by a logical drag delta. An invalid delta leaves the camera unchanged.
    pub fn pan(&mut self, delta_logical: Vec2) -> Option<()> {
        let movement = delta_logical.as_dvec2() * FLIP_Y.as_dvec2() / f64::from(self.zoom);
        let center = self.center.0.as_dvec2() - movement;
        self.center = WorldPoint(finite_vec2(center.as_vec2())?);
        Some(())
    }

    /// Multiplies zoom around a logical cursor while keeping its world position fixed.
    /// Invalid input leaves the camera unchanged. The anchor can be outside the pane.
    pub fn zoom_at(&mut self, anchor: PanelPoint, factor: f32) -> Option<()> {
        if !anchor.0.is_finite() || !factor.is_finite() || factor <= 0.0 {
            return None;
        }
        let zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let offset = anchor.0.as_dvec2() - self.panel_size.0.as_dvec2() * 0.5;
        let shift = f64::from(self.zoom).recip() - f64::from(zoom).recip();
        let center = self.center.0.as_dvec2() + offset * FLIP_Y.as_dvec2() * shift;
        self.center = WorldPoint(finite_vec2(center.as_vec2())?);
        self.zoom = zoom;
        Some(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
    TileBrush,
}

/// A square grid with positive spacing in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSnap2D(f32);

impl GridSnap2D {
    #[must_use]
    pub fn new(step: f32) -> Option<Self> {
        (step.is_finite() && step > 0.0).then_some(Self(step))
    }

    #[must_use]
    pub const fn step(self) -> f32 {
        self.0
    }

    /// Rounds to the nearest world grid intersection, with ties away from zero.
    #[must_use]
    pub fn snap_point(self, point: WorldPoint) -> Option<WorldPoint> {
        let snapped = point.0.as_dvec2().map(|value| self.snap_distance(value));
        finite_vec2(snapped.as_vec2()).map(WorldPoint)
    }

    fn snap_distance(self, distance: f64) -> f64 {
        let step = f64::from(self.0);
        (distance / step).round() * step
    }
}

fn finite_vec2(value: Vec2) -> Option<Vec2> {
    value.is_finite().then_some(value)
}
