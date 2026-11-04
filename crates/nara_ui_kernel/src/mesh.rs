use std::sync::Arc;

use thiserror::Error;

use crate::{UiColor, UiPoint, UiRect};

pub(crate) const HARD_MAX_MESH_VERTICES: usize = 65_536;
pub(crate) const HARD_MAX_MESH_INDICES: usize = 196_608;
const MAX_LOCAL_COORDINATE: f32 = 16_384.0;

/// One local content-space vertex with a straight-alpha color.
///
/// Construction is infallible; [`UiMesh::new`] validates every position and color before the
/// geometry can enter an authoring snapshot. Positions are relative to the owning node's content
/// origin, excluding padding. Colors are interpolated across each triangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiMeshVertex {
    position: UiPoint,
    color: UiColor,
}

impl UiMeshVertex {
    #[must_use]
    pub const fn new(position: UiPoint, color: UiColor) -> Self {
        Self { position, color }
    }

    #[must_use]
    pub const fn position(self) -> UiPoint {
        self.position
    }

    #[must_use]
    pub const fn color(self) -> UiColor {
        self.color
    }
}

/// Validated immutable triangle geometry, independent of any renderer or tessellation library.
///
/// Clones share the original allocation. Each authored and drawn occurrence still consumes its
/// complete vertex/index counts under [`crate::UiLimits`]. Geometry supplies no intrinsic size:
/// the compiler resolves the node's content box and clips this geometry to it. Changed size,
/// pan, zoom or other geometry inputs require the component to author the appropriate mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct UiMesh {
    data: Arc<UiMeshData>,
}

#[derive(Debug, PartialEq)]
struct UiMeshData {
    vertices: Box<[UiMeshVertex]>,
    indices: Box<[u32]>,
    bounds: UiRect,
}

impl UiMesh {
    /// Captures a nonempty indexed triangle list.
    ///
    /// At most 65,536 vertices and 196,608 indices are accepted. Coordinates must be finite in
    /// `-16384..=16384`, all color channels finite in `0..=1`, and every index in range. Degenerate
    /// triangles are permitted; this seam does not choose a tessellator or triangle winding.
    pub fn new(vertices: Vec<UiMeshVertex>, indices: Vec<u32>) -> Result<Self, UiMeshError> {
        if vertices.is_empty() {
            return Err(UiMeshError::EmptyVertices);
        }
        if vertices.len() > HARD_MAX_MESH_VERTICES {
            return Err(UiMeshError::VertexLimitExceeded {
                actual: vertices.len(),
                maximum: HARD_MAX_MESH_VERTICES,
            });
        }
        if indices.is_empty() {
            return Err(UiMeshError::EmptyTriangles);
        }
        if indices.len() > HARD_MAX_MESH_INDICES {
            return Err(UiMeshError::IndexLimitExceeded {
                actual: indices.len(),
                maximum: HARD_MAX_MESH_INDICES,
            });
        }
        if !indices.len().is_multiple_of(3) {
            return Err(UiMeshError::IncompleteTriangle {
                index_count: indices.len(),
            });
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (vertex, value) in vertices.iter().enumerate() {
            let position = value.position;
            if !position.is_finite()
                || position.x().abs() > MAX_LOCAL_COORDINATE
                || position.y().abs() > MAX_LOCAL_COORDINATE
            {
                return Err(UiMeshError::InvalidPosition { vertex, position });
            }
            if !value
                .color
                .components()
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            {
                return Err(UiMeshError::InvalidColor { vertex });
            }
            min_x = min_x.min(position.x());
            min_y = min_y.min(position.y());
            max_x = max_x.max(position.x());
            max_y = max_y.max(position.y());
        }
        for (offset, &index) in indices.iter().enumerate() {
            if index as usize >= vertices.len() {
                return Err(UiMeshError::IndexOutOfRange {
                    offset,
                    index,
                    vertex_count: vertices.len(),
                });
            }
        }

        let enclosing_extent = |minimum: f32, maximum: f32| {
            let extent = maximum - minimum;
            if minimum + extent < maximum {
                extent.next_up()
            } else {
                extent
            }
        };
        let bounds = UiRect::from_origin_size(
            min_x,
            min_y,
            enclosing_extent(min_x, max_x),
            enclosing_extent(min_y, max_y),
        )
        .expect("nonempty bounded finite vertices have finite nonnegative bounds");
        Ok(Self {
            data: Arc::new(UiMeshData {
                vertices: vertices.into_boxed_slice(),
                indices: indices.into_boxed_slice(),
                bounds,
            }),
        })
    }

    #[must_use]
    pub fn vertices(&self) -> &[UiMeshVertex] {
        &self.data.vertices
    }

    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.data.indices
    }

    /// Conservative local bounds of all captured vertices, including unreferenced vertices.
    #[must_use]
    pub fn bounds(&self) -> UiRect {
        self.data.bounds
    }
}

/// Complete occurrence counts, used separately for source admission and resolved output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MeshCounts {
    pub(crate) vertices: usize,
    pub(crate) indices: usize,
}

impl MeshCounts {
    pub(crate) fn of(mesh: &UiMesh) -> Self {
        Self {
            vertices: mesh.vertices().len(),
            indices: mesh.indices().len(),
        }
    }

    pub(crate) fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            vertices: self.vertices.checked_add(other.vertices)?,
            indices: self.indices.checked_add(other.indices)?,
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiMeshError {
    #[error("UI mesh has no vertices")]
    EmptyVertices,
    #[error("UI mesh has no triangles")]
    EmptyTriangles,
    #[error("UI mesh has {actual} vertices; maximum is {maximum}")]
    VertexLimitExceeded { actual: usize, maximum: usize },
    #[error("UI mesh has {actual} indices; maximum is {maximum}")]
    IndexLimitExceeded { actual: usize, maximum: usize },
    #[error("UI mesh has {index_count} indices; triangle lists require a multiple of three")]
    IncompleteTriangle { index_count: usize },
    #[error("UI mesh vertex {vertex} has a non-finite or out-of-range position {position:?}")]
    InvalidPosition { vertex: usize, position: UiPoint },
    #[error("UI mesh vertex {vertex} has a non-finite or out-of-range color channel")]
    InvalidColor { vertex: usize },
    #[error("UI mesh index {offset} references vertex {index}; vertex count is {vertex_count}")]
    IndexOutOfRange {
        offset: usize,
        index: u32,
        vertex_count: usize,
    },
}
