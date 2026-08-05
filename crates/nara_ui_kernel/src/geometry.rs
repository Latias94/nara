/// A logical point in the UI design space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiPoint {
    x: f32,
    y: f32,
}

impl UiPoint {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    pub(crate) fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A non-negative logical extent.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiSize {
    width: f32,
    height: f32,
}

impl UiSize {
    #[must_use]
    pub fn new(width: f32, height: f32) -> Option<Self> {
        (width.is_finite() && height.is_finite() && width >= 0.0 && height >= 0.0)
            .then_some(Self { width, height })
    }

    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// A logical axis-aligned rectangle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiRect {
    origin: UiPoint,
    size: UiSize,
}

impl UiRect {
    #[must_use]
    pub fn new(origin: UiPoint, size: UiSize) -> Option<Self> {
        origin.is_finite().then_some(Self { origin, size })
    }

    #[must_use]
    pub fn from_origin_size(x: f32, y: f32, width: f32, height: f32) -> Option<Self> {
        Self::new(UiPoint::new(x, y), UiSize::new(width, height)?)
    }

    #[must_use]
    pub const fn origin(self) -> UiPoint {
        self.origin
    }

    #[must_use]
    pub const fn size(self) -> UiSize {
        self.size
    }

    #[must_use]
    pub fn max(self) -> UiPoint {
        UiPoint::new(
            self.origin.x + self.size.width,
            self.origin.y + self.size.height,
        )
    }

    #[must_use]
    pub fn contains(self, point: UiPoint) -> bool {
        if !point.is_finite() || self.size.is_empty() {
            return false;
        }
        let max = self.max();
        point.x >= self.origin.x && point.y >= self.origin.y && point.x < max.x && point.y < max.y
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Option<Self> {
        let min_x = self.origin.x.max(other.origin.x);
        let min_y = self.origin.y.max(other.origin.y);
        let max_x = self.max().x.min(other.max().x);
        let max_y = self.max().y.min(other.max().y);
        Self::from_origin_size(min_x, min_y, max_x - min_x, max_y - min_y)
            .filter(|rect| !rect.size.is_empty())
    }
}

/// Physical snapping policy carried by each immutable environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiSnapPolicy {
    /// Snap each logical rectangle edge independently to its nearest physical pixel.
    #[default]
    NearestEdge,
}

/// A snapped rectangle in physical pixel coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiPhysicalRect {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl UiPhysicalRect {
    #[must_use]
    pub const fn min_x(self) -> i32 {
        self.min_x
    }

    #[must_use]
    pub const fn min_y(self) -> i32 {
        self.min_y
    }

    #[must_use]
    pub const fn max_x(self) -> i32 {
        self.max_x
    }

    #[must_use]
    pub const fn max_y(self) -> i32 {
        self.max_y
    }

    pub(crate) fn snap(rect: UiRect, scale_factor: f32, policy: UiSnapPolicy) -> Self {
        match policy {
            UiSnapPolicy::NearestEdge => {
                let max = rect.max();
                Self {
                    min_x: snap_coordinate(rect.origin.x, scale_factor),
                    min_y: snap_coordinate(rect.origin.y, scale_factor),
                    max_x: snap_coordinate(max.x, scale_factor),
                    max_y: snap_coordinate(max.y, scale_factor),
                }
            }
        }
    }
}

fn snap_coordinate(value: f32, scale_factor: f32) -> i32 {
    let physical = (value * scale_factor).round();
    physical.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}
