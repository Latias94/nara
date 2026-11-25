use std::sync::Arc;

use nara_ui_kernel::UiColor;

/// One immutable mesh vertex in physical surface coordinates.
///
/// Positions retain the exact floating-point content transform. They are not scaled from a
/// snapped rectangle. Color uses the same straight-alpha convention as other UI primitives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPacketMeshVertex {
    position: [f32; 2],
    color: UiColor,
}

impl UiPacketMeshVertex {
    pub(crate) const fn new(position: [f32; 2], color: UiColor) -> Self {
        Self { position, color }
    }

    #[must_use]
    pub const fn position(self) -> [f32; 2] {
        self.position
    }

    #[must_use]
    pub const fn color(self) -> UiColor {
        self.color
    }
}

/// Opaque immutable triangle-list geometry transformed for one packet occurrence.
///
/// Indices address this payload's vertices. The planner preserves authored vertex colors,
/// indices and primitive order; a concrete backend applies the owning primitive's physical clip.
/// Geometry contains no image or glyph resource references and owns no backend handles.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPacketMesh {
    vertices: Arc<[UiPacketMeshVertex]>,
    indices: Arc<[u32]>,
}

impl UiPacketMesh {
    pub(crate) fn new(vertices: Vec<UiPacketMeshVertex>, indices: Vec<u32>) -> Self {
        Self {
            vertices: vertices.into(),
            indices: indices.into(),
        }
    }

    #[must_use]
    pub fn vertices(&self) -> &[UiPacketMeshVertex] {
        &self.vertices
    }

    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }
}
