use nara_core::{Affine2, EntityUuid, Mat3, Vec2};

use super::{PanelPoint, ViewportCamera2D, WorldPoint};

/// A finite, nonempty rectangle in an entity's local coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalBounds2D {
    min: Vec2,
    max: Vec2,
}

impl LocalBounds2D {
    #[must_use]
    pub fn new(min: Vec2, max: Vec2) -> Option<Self> {
        (min.is_finite() && max.is_finite() && min.cmplt(max).all()).then_some(Self { min, max })
    }

    fn corners(self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }
}

/// An entity's projected selection quad. Its four corners are also the selection outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickCandidate2D {
    entity: EntityUuid,
    corners: [WorldPoint; 4],
    winding: f64,
}

impl PickCandidate2D {
    /// Builds the quad from finite affine geometry, including rotations, shears and reflections.
    /// Rejects singular transforms and quads that collapse at world-coordinate precision.
    #[must_use]
    pub fn new(
        entity: EntityUuid,
        bounds: LocalBounds2D,
        world_from_local: Affine2,
    ) -> Option<Self> {
        if !world_from_local.is_finite() {
            return None;
        }
        let x_axis = world_from_local.matrix2.x_axis.as_dvec2();
        let y_axis = world_from_local.matrix2.y_axis.as_dvec2();
        let determinant = x_axis.perp_dot(y_axis);
        if determinant == 0.0 {
            return None;
        }
        let winding = determinant.signum();
        let translation = world_from_local.translation.as_dvec2();
        let corners = bounds.corners().map(|point| {
            let point = x_axis * f64::from(point.x) + y_axis * f64::from(point.y) + translation;
            WorldPoint(point.as_vec2())
        });
        if !corners.iter().all(|point| point.0.is_finite()) {
            return None;
        }
        let convex = (0..corners.len()).all(|index| {
            let start = corners[index];
            let end = corners[(index + 1) % corners.len()];
            let next = corners[(index + 2) % corners.len()];
            winding * edge_cross(start, end, next) > 0.0
        });
        convex.then_some(Self {
            entity,
            corners,
            winding,
        })
    }

    /// Accepts a homogeneous affine matrix with bottom row `[0, 0, 1]`.
    #[must_use]
    pub fn from_matrix(
        entity: EntityUuid,
        bounds: LocalBounds2D,
        world_from_local: Mat3,
    ) -> Option<Self> {
        if world_from_local.x_axis.z != 0.0
            || world_from_local.y_axis.z != 0.0
            || world_from_local.z_axis.z != 1.0
        {
            return None;
        }
        Self::new(entity, bounds, Affine2::from_mat3(world_from_local))
    }

    #[must_use]
    pub const fn entity(self) -> EntityUuid {
        self.entity
    }

    /// Returns the transformed corners in local bottom-left, bottom-right, top-right, top-left
    /// order. Reflections reverse their world-space winding.
    #[must_use]
    pub const fn world_corners(&self) -> [WorldPoint; 4] {
        self.corners
    }

    /// Tests the convex quad, including its boundary.
    #[must_use]
    pub fn contains(&self, point: WorldPoint) -> bool {
        point.0.is_finite()
            && (0..self.corners.len()).all(|index| {
                let start = self.corners[index];
                let end = self.corners[(index + 1) % self.corners.len()];
                self.winding * edge_cross(start, end, point) >= 0.0
            })
    }
}

impl ViewportCamera2D {
    /// Picks the frontmost containing candidate under an in-pane pointer. Supply selectable
    /// candidates in back-to-front paint order. The last containing candidate wins ties.
    #[must_use]
    pub fn pick<'candidate>(
        self,
        cursor: PanelPoint,
        candidates: impl IntoIterator<Item = &'candidate PickCandidate2D>,
    ) -> Option<EntityUuid> {
        if !self.contains(cursor) {
            return None;
        }
        let world = self.panel_to_world(cursor)?;
        candidates
            .into_iter()
            .filter(|candidate| candidate.contains(world))
            .map(|candidate| candidate.entity)
            .last()
    }
}

fn edge_cross(start: WorldPoint, end: WorldPoint, point: WorldPoint) -> f64 {
    let start = start.0.as_dvec2();
    (end.0.as_dvec2() - start).perp_dot(point.0.as_dvec2() - start)
}
