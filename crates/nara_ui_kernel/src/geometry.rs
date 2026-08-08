use thiserror::Error;

const MAX_AUTHORED_EXTENT: f32 = 16_384.0;
const MAX_PHYSICAL_EXTENT: f32 = 131_072.0;
const MAX_SCALE_FACTOR: f32 = 8.0;

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

/// Physical safe-area insets applied before authored content is fitted into a surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UiSafeAreaInsets {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl UiSafeAreaInsets {
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

    #[must_use]
    pub const fn left(self) -> f32 {
        self.left
    }

    #[must_use]
    pub const fn top(self) -> f32 {
        self.top
    }

    #[must_use]
    pub const fn right(self) -> f32 {
        self.right
    }

    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.bottom
    }
}

/// Exact backend-neutral mapping between authored, platform-logical and physical surface space.
///
/// Safe-area insets are removed from the physical surface first. The authored extent is then fitted
/// uniformly and centered inside the remaining area. The same immutable value is used for paint,
/// semantic bounds and inverse input mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSurfaceContentTransform {
    authored_logical_size: UiSize,
    physical_surface_size: UiSize,
    safe_area: UiSafeAreaInsets,
    physical_content_origin: UiPoint,
    physical_content_size: UiSize,
    platform_scale_factor: f32,
    content_scale_factor: f32,
}

impl UiSurfaceContentTransform {
    pub fn new(
        authored_logical_size: UiSize,
        physical_surface_size: UiSize,
        safe_area: UiSafeAreaInsets,
        platform_scale_factor: f32,
    ) -> Result<Self, UiSurfaceContentTransformError> {
        validate_positive_size(
            authored_logical_size,
            MAX_AUTHORED_EXTENT,
            UiSurfaceContentTransformError::InvalidAuthoredLogicalSize {
                width: authored_logical_size.width(),
                height: authored_logical_size.height(),
            },
        )?;
        validate_positive_size(
            physical_surface_size,
            MAX_PHYSICAL_EXTENT,
            UiSurfaceContentTransformError::InvalidPhysicalSurfaceSize {
                width: physical_surface_size.width(),
                height: physical_surface_size.height(),
            },
        )?;
        if !platform_scale_factor.is_finite()
            || platform_scale_factor <= 0.0
            || platform_scale_factor > MAX_SCALE_FACTOR
        {
            return Err(UiSurfaceContentTransformError::InvalidPlatformScaleFactor {
                value: platform_scale_factor,
            });
        }

        let available_width = physical_surface_size.width() - safe_area.left() - safe_area.right();
        let available_height =
            physical_surface_size.height() - safe_area.top() - safe_area.bottom();
        if !available_width.is_finite()
            || !available_height.is_finite()
            || available_width <= 0.0
            || available_height <= 0.0
        {
            return Err(UiSurfaceContentTransformError::SafeAreaConsumesSurface {
                surface_width: physical_surface_size.width(),
                surface_height: physical_surface_size.height(),
            });
        }

        let content_scale_factor = (available_width / authored_logical_size.width())
            .min(available_height / authored_logical_size.height());
        if !content_scale_factor.is_finite()
            || content_scale_factor <= 0.0
            || content_scale_factor > MAX_SCALE_FACTOR
        {
            return Err(UiSurfaceContentTransformError::InvalidContentScaleFactor {
                value: content_scale_factor,
            });
        }
        let content_width = authored_logical_size.width() * content_scale_factor;
        let content_height = authored_logical_size.height() * content_scale_factor;
        let physical_content_size = UiSize::new(content_width, content_height)
            .expect("validated content size is finite and positive");
        let physical_content_origin = UiPoint::new(
            safe_area.left() + (available_width - content_width) * 0.5,
            safe_area.top() + (available_height - content_height) * 0.5,
        );

        Ok(Self {
            authored_logical_size,
            physical_surface_size,
            safe_area,
            physical_content_origin,
            physical_content_size,
            platform_scale_factor,
            content_scale_factor,
        })
    }

    /// Builds the zero-origin mapping used by headless and direct-surface consumers.
    pub fn direct(
        authored_logical_size: UiSize,
        scale_factor: f32,
    ) -> Result<Self, UiSurfaceContentTransformError> {
        if !scale_factor.is_finite() || scale_factor <= 0.0 || scale_factor > MAX_SCALE_FACTOR {
            return Err(UiSurfaceContentTransformError::InvalidContentScaleFactor {
                value: scale_factor,
            });
        }
        let physical_surface_size = UiSize::new(
            authored_logical_size.width() * scale_factor,
            authored_logical_size.height() * scale_factor,
        )
        .ok_or(UiSurfaceContentTransformError::InvalidPhysicalSurfaceSize {
            width: authored_logical_size.width() * scale_factor,
            height: authored_logical_size.height() * scale_factor,
        })?;
        Self::new(
            authored_logical_size,
            physical_surface_size,
            UiSafeAreaInsets::default(),
            scale_factor,
        )
    }

    #[must_use]
    pub const fn authored_logical_size(self) -> UiSize {
        self.authored_logical_size
    }

    #[must_use]
    pub const fn physical_surface_size(self) -> UiSize {
        self.physical_surface_size
    }

    #[must_use]
    pub const fn safe_area(self) -> UiSafeAreaInsets {
        self.safe_area
    }

    #[must_use]
    pub const fn physical_content_origin(self) -> UiPoint {
        self.physical_content_origin
    }

    #[must_use]
    pub const fn physical_content_size(self) -> UiSize {
        self.physical_content_size
    }

    #[must_use]
    pub const fn platform_scale_factor(self) -> f32 {
        self.platform_scale_factor
    }

    #[must_use]
    pub const fn content_scale_factor(self) -> f32 {
        self.content_scale_factor
    }

    /// Maps an authored point into physical surface coordinates when it is inside the authored
    /// extent. The maximum edge is excluded to preserve hit-test half-open semantics.
    #[must_use]
    pub fn authored_to_physical(self, point: UiPoint) -> Option<UiPoint> {
        self.authored_bounds().contains(point).then(|| {
            UiPoint::new(
                self.physical_content_origin.x() + point.x() * self.content_scale_factor,
                self.physical_content_origin.y() + point.y() * self.content_scale_factor,
            )
        })
    }

    /// Maps a platform-logical point through the platform scale and exact inverse content mapping.
    /// Points in letterbox or safe-area regions are rejected rather than clamped.
    #[must_use]
    pub fn platform_logical_to_authored(self, point: UiPoint) -> Option<UiPoint> {
        if !point.is_finite() {
            return None;
        }
        self.physical_to_authored(UiPoint::new(
            point.x() * self.platform_scale_factor,
            point.y() * self.platform_scale_factor,
        ))
    }

    /// Maps a physical-surface point into authored logical coordinates, rejecting black bars and
    /// safe-area regions.
    #[must_use]
    pub fn physical_to_authored(self, point: UiPoint) -> Option<UiPoint> {
        if !self.physical_content_bounds().contains(point) {
            return None;
        }
        Some(UiPoint::new(
            (point.x() - self.physical_content_origin.x()) / self.content_scale_factor,
            (point.y() - self.physical_content_origin.y()) / self.content_scale_factor,
        ))
    }

    /// Maps and clips authored geometry to the exact physical content rect.
    #[must_use]
    pub fn authored_rect_to_physical(
        self,
        rect: UiRect,
        policy: UiSnapPolicy,
    ) -> Option<UiPhysicalRect> {
        let visible = rect.intersect(self.authored_bounds())?;
        let max = visible.max();
        let min_x =
            self.physical_content_origin.x() + visible.origin().x() * self.content_scale_factor;
        let min_y =
            self.physical_content_origin.y() + visible.origin().y() * self.content_scale_factor;
        let max_x = self.physical_content_origin.x() + max.x() * self.content_scale_factor;
        let max_y = self.physical_content_origin.y() + max.y() * self.content_scale_factor;
        UiPhysicalRect::snap_physical_edges(min_x, min_y, max_x, max_y, policy)
            .filter(|rect| !rect.is_empty())
    }

    fn authored_bounds(self) -> UiRect {
        UiRect::new(UiPoint::default(), self.authored_logical_size)
            .expect("validated authored size produces finite bounds")
    }

    fn physical_content_bounds(self) -> UiRect {
        UiRect::new(self.physical_content_origin, self.physical_content_size)
            .expect("validated content geometry produces finite bounds")
    }
}

fn validate_positive_size(
    size: UiSize,
    maximum: f32,
    error: UiSurfaceContentTransformError,
) -> Result<(), UiSurfaceContentTransformError> {
    if size.is_empty()
        || size.width() > maximum
        || size.height() > maximum
        || !size.width().is_finite()
        || !size.height().is_finite()
    {
        Err(error)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiSurfaceContentTransformError {
    #[error("authored logical UI size {width} x {height} is invalid")]
    InvalidAuthoredLogicalSize { width: f32, height: f32 },
    #[error("physical UI surface size {width} x {height} is invalid")]
    InvalidPhysicalSurfaceSize { width: f32, height: f32 },
    #[error("UI platform scale factor {value} is invalid")]
    InvalidPlatformScaleFactor { value: f32 },
    #[error("UI content scale factor {value} is invalid")]
    InvalidContentScaleFactor { value: f32 },
    #[error("safe area consumes physical UI surface {surface_width} x {surface_height}")]
    SafeAreaConsumesSurface {
        surface_width: f32,
        surface_height: f32,
    },
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

    #[must_use]
    pub const fn width(self) -> i32 {
        self.max_x.saturating_sub(self.min_x)
    }

    #[must_use]
    pub const fn height(self) -> i32 {
        self.max_y.saturating_sub(self.min_y)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.max_x <= self.min_x || self.max_y <= self.min_y
    }

    fn snap_physical_edges(
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
        policy: UiSnapPolicy,
    ) -> Option<Self> {
        match policy {
            UiSnapPolicy::NearestEdge => [min_x, min_y, max_x, max_y]
                .into_iter()
                .all(f32::is_finite)
                .then_some(Self {
                    min_x: snap_physical_coordinate(min_x),
                    min_y: snap_physical_coordinate(min_y),
                    max_x: snap_physical_coordinate(max_x),
                    max_y: snap_physical_coordinate(max_y),
                }),
        }
    }
}

fn snap_physical_coordinate(value: f32) -> i32 {
    value.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}
