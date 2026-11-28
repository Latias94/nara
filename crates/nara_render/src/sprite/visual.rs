/// Controls how a sprite's texture fits its local size.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum SpriteScaleMode {
    #[default]
    Stretch,
    /// Preserves the borders of a textured sprite while stretching its center and edges.
    /// Uses stretch until the source image is available.
    Sliced(NineSlice),
}

/// Nine-slice borders measured in pixels of the source image or atlas region.
///
/// Each border keeps one local unit per source pixel. Opposite borders shrink proportionally
/// when they exceed the source region or the sprite's local size. Flips mirror both the border
/// widths and their texture coordinates. Negative and non-finite values are treated as zero.
/// When the source center collapses, any remaining destination center samples the join.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct NineSlice {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl NineSlice {
    #[must_use]
    pub const fn new(left: f32, right: f32, top: f32, bottom: f32) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
        .nonnegative()
    }

    pub(crate) const fn nonnegative(self) -> Self {
        const fn border(value: f32) -> f32 {
            if value.is_finite() && value > 0.0 {
                value
            } else {
                0.0
            }
        }

        Self {
            left: border(self.left),
            right: border(self.right),
            top: border(self.top),
            bottom: border(self.bottom),
        }
    }
}
