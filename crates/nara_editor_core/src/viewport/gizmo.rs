use std::f32::consts::FRAC_1_SQRT_2;

use nara_core::Vec2;

use super::{FLIP_Y, GridSnap2D, PanelPoint, ToolMode, ViewportCamera2D, WorldPoint, finite_vec2};

const AXIS_LENGTH: f32 = 48.0;
const CENTER_RADIUS: f32 = 8.0;
const HANDLE_RADIUS: f32 = 6.0;
const RING_RADIUS: f32 = 44.0;
const RING_HIT_HALF_WIDTH: f32 = 6.0;
const MIN_SCALE_FACTOR: f32 = 0.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoHandle {
    Free,
    X,
    Y,
    Rotation,
}

/// World-space geometry for the active tool. Lengths are derived from logical pane pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GizmoGeometry2D {
    Move(GizmoAxes2D),
    Rotate(GizmoRing2D),
    Scale(GizmoAxes2D),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoAxes2D {
    pub pivot: WorldPoint,
    pub x_end: WorldPoint,
    pub y_end: WorldPoint,
    pub center_radius: f32,
    pub handle_radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoRing2D {
    pub pivot: WorldPoint,
    pub radius: f32,
    pub hit_half_width: f32,
}

/// A selection pivot and an orthonormal gizmo basis in the scene's world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gizmo2D {
    pivot: WorldPoint,
    angle: f32,
}

impl Gizmo2D {
    /// A zero angle uses world axes. Positive radians rotate the axes counterclockwise.
    #[must_use]
    pub fn new(pivot: WorldPoint, angle_radians: f32) -> Option<Self> {
        (pivot.0.is_finite() && angle_radians.is_finite()).then_some(Self {
            pivot,
            angle: angle_radians,
        })
    }

    #[must_use]
    pub const fn pivot(self) -> WorldPoint {
        self.pivot
    }

    #[must_use]
    pub const fn angle(self) -> f32 {
        self.angle
    }

    #[must_use]
    pub fn geometry(self, camera: &ViewportCamera2D, mode: ToolMode) -> Option<GizmoGeometry2D> {
        let zoom = camera.zoom();
        match mode {
            ToolMode::Select | ToolMode::TileBrush => None,
            ToolMode::Move | ToolMode::Scale => {
                let x_axis = Vec2::from_angle(self.angle);
                let axis_length = AXIS_LENGTH / zoom;
                let x_end = finite_vec2(self.pivot.0 + x_axis * axis_length)?;
                let y_end = finite_vec2(self.pivot.0 + x_axis.perp() * axis_length)?;
                if (x_end - self.pivot.0).perp_dot(y_end - self.pivot.0) <= 0.0 {
                    return None;
                }
                let axes = GizmoAxes2D {
                    pivot: self.pivot,
                    x_end: WorldPoint(x_end),
                    y_end: WorldPoint(y_end),
                    center_radius: CENTER_RADIUS / zoom,
                    handle_radius: HANDLE_RADIUS / zoom,
                };
                Some(match mode {
                    ToolMode::Move => GizmoGeometry2D::Move(axes),
                    _ => GizmoGeometry2D::Scale(axes),
                })
            }
            ToolMode::Rotate => {
                let radius = RING_RADIUS / zoom;
                let extent = Vec2::splat(radius);
                let min = finite_vec2(self.pivot.0 - extent)?;
                let max = finite_vec2(self.pivot.0 + extent)?;
                (min.cmplt(self.pivot.0).all() && max.cmpgt(self.pivot.0).all()).then_some(
                    GizmoGeometry2D::Rotate(GizmoRing2D {
                        pivot: self.pivot,
                        radius,
                        hit_half_width: RING_HIT_HALF_WIDTH / zoom,
                    }),
                )
            }
        }
    }

    /// Tests the tool in logical pixels. The center takes priority over the nearest axis, and
    /// exact axis ties favor X. Only pointers inside the pane can acquire a handle.
    #[must_use]
    pub fn hit_test(
        self,
        camera: &ViewportCamera2D,
        mode: ToolMode,
        cursor: PanelPoint,
    ) -> Option<GizmoHandle> {
        if !camera.contains(cursor) {
            return None;
        }
        let geometry = self.geometry(camera, mode)?;
        let pivot = camera.world_to_panel(self.pivot)?;
        let offset = cursor.0.as_dvec2() - pivot.0.as_dvec2();
        let zoom = f64::from(camera.zoom());
        match geometry {
            GizmoGeometry2D::Move(axes) | GizmoGeometry2D::Scale(axes) => {
                let center = f64::from(axes.center_radius) * zoom;
                if offset.length_squared() <= center * center {
                    return Some(GizmoHandle::Free);
                }
                let x_end = camera.world_to_panel(axes.x_end)?;
                let y_end = camera.world_to_panel(axes.y_end)?;
                let x_distance = segment_distance_squared(cursor, pivot, x_end);
                let y_distance = segment_distance_squared(cursor, pivot, y_end);
                let width = f64::from(axes.handle_radius) * zoom;
                let (distance, handle) = if x_distance <= y_distance {
                    (x_distance, GizmoHandle::X)
                } else {
                    (y_distance, GizmoHandle::Y)
                };
                (distance <= width * width).then_some(handle)
            }
            GizmoGeometry2D::Rotate(ring) => {
                let distance = (offset.length() - f64::from(ring.radius) * zoom).abs();
                (distance <= f64::from(ring.hit_half_width) * zoom).then_some(GizmoHandle::Rotation)
            }
        }
    }

    /// Starts a drag using the camera and gizmo basis captured at the press.
    #[must_use]
    pub fn begin_drag(
        self,
        camera: &ViewportCamera2D,
        mode: ToolMode,
        cursor: PanelPoint,
    ) -> Option<GizmoDrag2D> {
        let handle = self.hit_test(camera, mode, cursor)?;
        let x_axis = Vec2::from_angle(self.angle);
        let axis = match handle {
            GizmoHandle::X => Some(x_axis),
            GizmoHandle::Y => Some(x_axis.perp()),
            GizmoHandle::Free | GizmoHandle::Rotation => None,
        };
        let operation = match mode {
            ToolMode::Move => DragOperation::Move {
                axis,
                zoom: camera.zoom(),
            },
            ToolMode::Rotate => {
                let pivot = camera.world_to_panel(self.pivot)?;
                DragOperation::Rotate {
                    pivot,
                    direction: rotation_direction(cursor, pivot),
                    radians: 0.0,
                }
            }
            ToolMode::Scale => DragOperation::Scale {
                direction: axis.unwrap_or(Vec2::splat(FRAC_1_SQRT_2)),
                affected: match handle {
                    GizmoHandle::X => Vec2::X,
                    GizmoHandle::Y => Vec2::Y,
                    GizmoHandle::Free | GizmoHandle::Rotation => Vec2::ONE,
                },
            },
            ToolMode::Select | ToolMode::TileBrush => return None,
        };
        Some(GizmoDrag2D {
            gizmo: self,
            handle,
            start: cursor,
            operation,
        })
    }
}

/// Gesture output relative to the original press. The caller applies it to its edit baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GizmoDelta2D {
    /// A displacement in world units.
    Translation(Vec2),
    /// Counterclockwise rotation in radians about the captured pivot.
    Rotation(f32),
    /// Multiplicative factors along the captured gizmo basis.
    Scale(Vec2),
}

/// State for a single captured pointer drag.
#[derive(Debug)]
pub struct GizmoDrag2D {
    gizmo: Gizmo2D,
    handle: GizmoHandle,
    start: PanelPoint,
    operation: DragOperation,
}

#[derive(Debug)]
enum DragOperation {
    Move {
        axis: Option<Vec2>,
        zoom: f32,
    },
    Rotate {
        pivot: PanelPoint,
        direction: Option<Vec2>,
        radians: f64,
    },
    Scale {
        direction: Vec2,
        affected: Vec2,
    },
}

impl GizmoDrag2D {
    #[must_use]
    pub const fn gizmo(&self) -> Gizmo2D {
        self.gizmo
    }

    #[must_use]
    pub const fn handle(&self) -> GizmoHandle {
        self.handle
    }

    /// Samples the captured pointer, including positions outside the pane. Rotation accumulates
    /// the signed angle between consecutive samples and reacquires direction after crossing the
    /// pivot's one-logical-pixel dead zone. Uniform scale increases on an up-right drag. Scale
    /// factors stop at `0.01` so the gesture keeps the starting signs.
    pub fn update(&mut self, cursor: PanelPoint) -> Option<GizmoDelta2D> {
        self.resolve(cursor, None)
    }

    /// Samples the pointer with translation snapped relative to the press. Constrained moves
    /// snap along the captured axis in world units. Rotation and scale use their usual units.
    pub fn update_snapped(&mut self, cursor: PanelPoint, grid: GridSnap2D) -> Option<GizmoDelta2D> {
        self.resolve(cursor, Some(grid))
    }

    fn resolve(&mut self, cursor: PanelPoint, grid: Option<GridSnap2D>) -> Option<GizmoDelta2D> {
        if !cursor.0.is_finite() {
            return None;
        }
        let movement = (cursor.0.as_dvec2() - self.start.0.as_dvec2()) * FLIP_Y.as_dvec2();
        match &mut self.operation {
            DragOperation::Move { axis, zoom } => {
                let world = movement / f64::from(*zoom);
                let world = match axis {
                    Some(axis) => {
                        let axis = axis.as_dvec2();
                        let distance = world.dot(axis);
                        axis * grid.map_or(distance, |grid| grid.snap_distance(distance))
                    }
                    None => grid.map_or(world, |grid| world.map(|value| grid.snap_distance(value))),
                };
                finite_vec2(world.as_vec2()).map(GizmoDelta2D::Translation)
            }
            DragOperation::Rotate {
                pivot,
                direction,
                radians,
            } => {
                let Some(next) = rotation_direction(cursor, *pivot) else {
                    *direction = None;
                    return None;
                };
                let step = direction.map_or(0.0, |previous| {
                    let previous = previous.as_dvec2();
                    let next = next.as_dvec2();
                    previous.perp_dot(next).atan2(previous.dot(next))
                });
                let total = *radians + step;
                let result = total as f32;
                if !result.is_finite() {
                    return None;
                }
                *radians = total;
                *direction = Some(next);
                Some(GizmoDelta2D::Rotation(result))
            }
            DragOperation::Scale {
                direction,
                affected,
            } => {
                let distance = movement.dot(direction.as_dvec2());
                let factor = (1.0 + distance / f64::from(AXIS_LENGTH))
                    .max(f64::from(MIN_SCALE_FACTOR)) as f32;
                let factors = Vec2::ONE - *affected + *affected * factor;
                finite_vec2(factors).map(GizmoDelta2D::Scale)
            }
        }
    }
}

fn segment_distance_squared(point: PanelPoint, start: PanelPoint, end: PanelPoint) -> f64 {
    let start = start.0.as_dvec2();
    let segment = end.0.as_dvec2() - start;
    let length_squared = segment.length_squared();
    if length_squared == 0.0 {
        return f64::INFINITY;
    }
    let offset = point.0.as_dvec2() - start;
    let along = (offset.dot(segment) / length_squared).clamp(0.0, 1.0);
    (offset - along * segment).length_squared()
}

fn rotation_direction(cursor: PanelPoint, pivot: PanelPoint) -> Option<Vec2> {
    let offset = (cursor.0.as_dvec2() - pivot.0.as_dvec2()) * FLIP_Y.as_dvec2();
    let length = offset.length();
    (length > 1.0).then(|| (offset / length).as_vec2())
}
