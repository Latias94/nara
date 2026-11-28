use std::sync::Arc;

use bevy_ecs::component::Component;
use nara_core::Vec2;
use nara_scene::spatial::Transform2d;
use rapier2d::{math::Pose, prelude::SharedShape};

use crate::{Collider2d, PhysicsError, native_vector, validate_local_shape, validate_shape_pose};

/// One axis-aligned box in a compound collider's local space, in scene units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompoundBox2d {
    pub center: Vec2,
    pub half_extents: Vec2,
}

/// Boxes attached as a single collider, with one pose, material, and collision group.
///
/// The boxes follow the entity's translation and rotation using the same transform rules as
/// [`Collider2d`]. Use a separate entity for each subset of boxes with different layer masks.
#[derive(Component, Clone, Debug)]
#[require(Transform2d)]
pub struct CompoundCollider2d {
    boxes: Arc<[CompoundBox2d]>,
    pub(super) shape: SharedShape,
}

impl CompoundCollider2d {
    /// Validates nonempty local geometry and builds its reusable collision shape.
    pub fn new(boxes: impl Into<Arc<[CompoundBox2d]>>) -> Result<Self, PhysicsError> {
        let boxes = boxes.into();
        if boxes.is_empty() {
            return Err(PhysicsError::InvalidCollider);
        }
        let mut parts = Vec::with_capacity(boxes.len());
        for part in boxes.iter() {
            let shape = Collider2d::Box {
                half_extents: part.half_extents,
            }
            .native_shape()?;
            let pose = Pose::from_translation(native_vector(part.center));
            validate_shape_pose(&shape, &pose)?;
            parts.push((pose, shape));
        }
        let shape = SharedShape::compound(parts);
        validate_local_shape(&shape)?;
        Ok(Self { boxes, shape })
    }

    /// Borrows the local boxes in construction order for debug drawing or inspection.
    #[must_use]
    pub fn boxes(&self) -> &[CompoundBox2d] {
        &self.boxes
    }
}

impl PartialEq for CompoundCollider2d {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.boxes, &other.boxes) || self.boxes == other.boxes
    }
}

/// A rectangle of grid cells starting at `(x, y)` and extending in the positive directions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Covers solid cells with nonoverlapping rectangles in row-major order.
///
/// At each uncovered solid cell, extends the rectangle across the row and then across matching
/// rows. This keeps contiguous horizontal tile surfaces in one box. The predicate runs once per
/// cell in row-major order. Call separately for tile subsets with different collision layers.
#[must_use]
pub fn merge_solid_grid_rects(
    width: u32,
    height: u32,
    mut is_solid: impl FnMut(u32, u32) -> bool,
) -> Vec<GridRect> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let stride = width as usize;
    let mut cells = Vec::new();
    for y in 0..height {
        cells.extend((0..width).map(|x| is_solid(x, y)));
    }

    let mut rectangles = Vec::new();
    for y in 0..height as usize {
        for x in 0..stride {
            let start = y * stride + x;
            if !cells[start] {
                continue;
            }
            let run = cells[start..(y + 1) * stride]
                .iter()
                .take_while(|&&solid| solid)
                .count();
            let mut end = y + 1;
            while end < height as usize
                && cells[end * stride + x..end * stride + x + run]
                    .iter()
                    .all(|&solid| solid)
            {
                end += 1;
            }
            for row in y..end {
                let start = row * stride + x;
                cells[start..start + run].fill(false);
            }
            rectangles.push(GridRect {
                x: x as u32,
                y: y as u32,
                width: run as u32,
                height: (end - y) as u32,
            });
        }
    }
    rectangles
}
