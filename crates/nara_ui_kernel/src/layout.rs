use std::{fmt, sync::Arc};

use thiserror::Error;

use crate::{UiRect, UiSize};

/// A logical content-space constraint. Definite space is a wrapping hint, not an exact size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UiAvailableSpace {
    /// Available content width/height; child authored size and intrinsic overflow still apply.
    Definite(f32),
    /// Minimum intrinsic content query. Wrapping text uses the shaper's min-content width.
    MinContent,
    /// Natural intrinsic content query; wrapping text has no available-width limit.
    MaxContent,
}

/// Available content space for one repeatable layout or child measurement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiLayoutConstraints {
    pub width: UiAvailableSpace,
    pub height: UiAvailableSpace,
}

impl UiLayoutConstraints {
    pub const MAX_CONTENT: Self = Self {
        width: UiAvailableSpace::MaxContent,
        height: UiAvailableSpace::MaxContent,
    };

    pub fn new(width: UiAvailableSpace, height: UiAvailableSpace) -> Result<Self, UiLayoutError> {
        let constraints = Self { width, height };
        constraints.validate()?;
        Ok(constraints)
    }

    pub(crate) fn validate(self) -> Result<(), UiLayoutError> {
        for axis in [self.width, self.height] {
            if let UiAvailableSpace::Definite(value) = axis
                && (!value.is_finite() || value < 0.0)
            {
                return Err(UiLayoutError::InvalidConstraints);
            }
        }
        Ok(())
    }
}

/// A measured child's complete outer box and optional first horizontal text baseline.
///
/// Text leaves report their actual first line, custom containers may declare a baseline, and
/// ordinary containers propagate the first visible in-flow baseline in authored child order.
/// Boxes without text report `None`; a synthetic box-bottom baseline is not exposed here.
/// A regular container's own painted text does not override its child-based intrinsic layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiLayoutMeasurement {
    size: UiSize,
    first_baseline: Option<f32>,
}

impl UiLayoutMeasurement {
    pub fn new(size: UiSize, first_baseline: Option<f32>) -> Result<Self, UiLayoutError> {
        validate_baseline(first_baseline)?;
        Ok(Self {
            size,
            first_baseline,
        })
    }

    #[must_use]
    pub const fn size(self) -> UiSize {
        self.size
    }

    #[must_use]
    pub const fn first_baseline(self) -> Option<f32> {
        self.first_baseline
    }
}

/// The custom container's natural content size, excluding its authored padding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiLayoutOutput {
    size: UiSize,
    first_baseline: Option<f32>,
}

impl UiLayoutOutput {
    #[must_use]
    pub const fn new(size: UiSize) -> Self {
        Self {
            size,
            first_baseline: None,
        }
    }

    pub fn with_first_baseline(mut self, baseline: f32) -> Result<Self, UiLayoutError> {
        validate_baseline(Some(baseline))?;
        self.first_baseline = Some(baseline);
        Ok(self)
    }

    #[must_use]
    pub const fn size(self) -> UiSize {
        self.size
    }

    #[must_use]
    pub const fn first_baseline(self) -> Option<f32> {
        self.first_baseline
    }
}

fn validate_baseline(baseline: Option<f32>) -> Result<(), UiLayoutError> {
    if baseline.is_some_and(|value| !value.is_finite()) {
        return Err(UiLayoutError::InvalidGeometry);
    }
    Ok(())
}

/// Repeatable layout over an immutable description and this node's admitted direct children.
///
/// Capture owned immutable inputs. Calls may be skipped or repeated with different constraints;
/// only private compatible cache/work observations may change. Never edit documents, spawn work,
/// install listeners or depend on a fixed invocation count here. Local state updates belong
/// before composition. Native-process Rust code is not a hot-reload or stable plugin ABI.
pub trait UiLayout: Send + Sync + 'static {
    fn layout(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError>;
}

/// An immutable custom-layout description retained by an authored snapshot, not by paint facts.
///
/// Clones share one description. Equality means that exact immutable instance, not equivalent
/// code or an external revision. Changed captured inputs require a new description. The compiler
/// owns all tree storage, text work, candidate caches and final geometry.
#[derive(Clone)]
pub struct UiCustomLayout(Arc<dyn UiLayout>);

impl UiCustomLayout {
    #[must_use]
    pub fn new(layout: impl UiLayout) -> Self {
        Self(Arc::new(layout))
    }

    pub(crate) fn run(
        &self,
        constraints: UiLayoutConstraints,
        children: &mut UiLayoutChildren<'_>,
    ) -> Result<UiLayoutOutput, UiLayoutError> {
        self.0.layout(constraints, children)
    }
}

impl fmt::Debug for UiCustomLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UiCustomLayout")
            .finish_non_exhaustive()
    }
}

impl PartialEq for UiCustomLayout {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Scoped access to the current custom container's admitted direct children, by local order.
///
/// This grants no arena, stable state identity or presentation authority. Each child must be
/// placed or explicitly hidden once per invocation. Invalid requests poison the root even if
/// the component catches the returned error. Request work is charged through the compiler's
/// candidate-wide layout limit, including repeated measurements and cache hits.
pub struct UiLayoutChildren<'a> {
    backend: &'a mut dyn UiLayoutBackend,
}

impl<'a> UiLayoutChildren<'a> {
    pub(crate) fn new(backend: &'a mut dyn UiLayoutBackend) -> Self {
        Self { backend }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.backend.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Measures a child subtree as an independent root under the supplied available space.
    ///
    /// Definite axes supply the percentage basis. Authored sizes remain in effect; the returned
    /// outer size need not equal available space. Repeating this at another width can change
    /// wrapping. Measurement does not select the final placement.
    /// Measure before placing or hiding this child; measuring afterward refuses the root so
    /// an exploratory pass cannot silently overwrite its final descendant geometry.
    pub fn measure(
        &mut self,
        child: usize,
        constraints: UiLayoutConstraints,
    ) -> Result<UiLayoutMeasurement, UiLayoutError> {
        self.backend.measure(child, constraints)
    }

    /// Observes a child's exact min-content outer width before choosing its allocation.
    ///
    /// Uses the same authored sizes, padding, fonts and typography as regular measurement.
    /// Nested layouts still receive real text line geometry, but wrapped text in this scoped
    /// query produces no drawable layout and defers output line admission to regular measurement
    /// and final paint. Source, grapheme, scale, glyph and candidate layout-work limits still
    /// apply, including cache hits. This query cannot publish or satisfy a final text request.
    ///
    /// Recompute after changing the child description, typography or immutable font policy;
    /// the compiler owns bounded request caching. As with [`Self::measure`], the child must
    /// still be unplaced, and a failed request poisons the root.
    pub fn min_content_width(&mut self, child: usize) -> Result<f32, UiLayoutError> {
        self.backend.min_content_width(child)
    }

    /// Places a child at an exact outer rectangle relative to this container's content origin.
    ///
    /// The compiler resolves its subtree again at this exact size, then uses the same geometry
    /// for paint, clipping, hits, focus and semantics. This replaces the child's outer position
    /// and size allocation; its padding and descendant styles remain in effect. An impossible
    /// allocation (for example, smaller than required padding) refuses the root.
    pub fn place(&mut self, child: usize, rect: UiRect) -> Result<(), UiLayoutError> {
        self.backend.place(child, rect)
    }

    /// Explicitly excludes a child subtree from this layout invocation's visible projections.
    pub fn hide(&mut self, child: usize) -> Result<(), UiLayoutError> {
        self.backend.hide(child)
    }
}

pub(crate) trait UiLayoutBackend {
    fn len(&self) -> usize;
    fn min_content_width(&mut self, child: usize) -> Result<f32, UiLayoutError>;
    fn measure(
        &mut self,
        child: usize,
        constraints: UiLayoutConstraints,
    ) -> Result<UiLayoutMeasurement, UiLayoutError>;
    fn place(&mut self, child: usize, rect: UiRect) -> Result<(), UiLayoutError>;
    fn hide(&mut self, child: usize) -> Result<(), UiLayoutError>;
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiLayoutError {
    #[error("layout constraints must be finite and non-negative")]
    InvalidConstraints,
    #[error("layout geometry and baselines must be finite, with non-negative sizes")]
    InvalidGeometry,
    #[error("layout child {child} is outside the current {children} direct children")]
    InvalidChild { child: usize, children: usize },
    #[error("layout child {child} was placed or hidden more than once")]
    DuplicatePlacement { child: usize },
    #[error("layout child {child} was neither placed nor hidden")]
    MissingPlacement { child: usize },
    #[error("layout child {child} cannot fit its exact allocated outer size")]
    PlacementSizeMismatch { child: usize },
    #[error("layout was aborted by an earlier failed request")]
    Aborted,
    #[error("custom layout rejected: {message}")]
    Rejected { message: String },
}
