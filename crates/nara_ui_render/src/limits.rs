use thiserror::Error;

const DEFAULT_MAX_EXPANDED_PRIMITIVES: usize = 16_384;
const DEFAULT_MAX_REFERENCED_IMAGES: usize = 256;
const DEFAULT_MAX_REFERENCED_GLYPHS: usize = 4_096;
const DEFAULT_MAX_GLYPH_ENTRIES: usize = 4_096;
const DEFAULT_MAX_ATLAS_PAGES: usize = 4;
const DEFAULT_ATLAS_PAGE_EXTENT: u32 = 1_024;
const DEFAULT_MAX_GLYPH_EXTENT: u32 = 256;
const DEFAULT_MAX_ATLAS_BYTES: usize = 4 * 1_024 * 1_024;
const DEFAULT_MAX_NEW_GLYPHS_PER_PLAN: usize = 256;
const DEFAULT_MAX_STAGED_ATLAS_BYTES: usize = 4 * 1_024 * 1_024;
const DEFAULT_MAX_PACKET_RETAINED_BYTES: usize = 64 * 1_024 * 1_024;
const DEFAULT_MAX_UNIFORM_CORNER_RADIUS: f32 = 4_096.0;

/// Hard-capped planner policy. Public methods can only tighten the baseline.
///
/// The expanded-primitive budget counts final renderer-neutral [`crate::UiPacketPrimitive`]
/// values. A concrete backend owns any further expansion into device instances and admits that
/// work against its separate instance-buffer budget.
///
/// The retained-packet budget covers planner-owned primitives and immutable image/glyph payloads.
/// It deliberately does not count the shared [`nara_ui_kernel::UiFrame`] allocation a second time;
/// candidate nodes, shaped glyphs, and logical paint are admitted by the kernel's own budgets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiPresentationLimits {
    max_expanded_primitives: usize,
    max_referenced_images: usize,
    max_referenced_glyphs: usize,
    max_glyph_entries: usize,
    max_atlas_pages: usize,
    atlas_page_extent: u32,
    max_glyph_extent: u32,
    max_atlas_bytes: usize,
    max_new_glyphs_per_plan: usize,
    max_staged_atlas_bytes: usize,
    max_packet_retained_bytes: usize,
    max_uniform_corner_radius: f32,
}

impl Default for UiPresentationLimits {
    fn default() -> Self {
        Self {
            max_expanded_primitives: DEFAULT_MAX_EXPANDED_PRIMITIVES,
            max_referenced_images: DEFAULT_MAX_REFERENCED_IMAGES,
            max_referenced_glyphs: DEFAULT_MAX_REFERENCED_GLYPHS,
            max_glyph_entries: DEFAULT_MAX_GLYPH_ENTRIES,
            max_atlas_pages: DEFAULT_MAX_ATLAS_PAGES,
            atlas_page_extent: DEFAULT_ATLAS_PAGE_EXTENT,
            max_glyph_extent: DEFAULT_MAX_GLYPH_EXTENT,
            max_atlas_bytes: DEFAULT_MAX_ATLAS_BYTES,
            max_new_glyphs_per_plan: DEFAULT_MAX_NEW_GLYPHS_PER_PLAN,
            max_staged_atlas_bytes: DEFAULT_MAX_STAGED_ATLAS_BYTES,
            max_packet_retained_bytes: DEFAULT_MAX_PACKET_RETAINED_BYTES,
            max_uniform_corner_radius: DEFAULT_MAX_UNIFORM_CORNER_RADIUS,
        }
    }
}

impl UiPresentationLimits {
    pub fn with_max_expanded_primitives(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_expanded_primitives = reduce_usize(
            "max_expanded_primitives",
            self.max_expanded_primitives,
            value,
        )?;
        Ok(self)
    }

    pub fn with_max_referenced_images(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_referenced_images =
            reduce_usize("max_referenced_images", self.max_referenced_images, value)?;
        Ok(self)
    }

    pub fn with_max_referenced_glyphs(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_referenced_glyphs =
            reduce_usize("max_referenced_glyphs", self.max_referenced_glyphs, value)?;
        Ok(self)
    }

    pub fn with_max_glyph_entries(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_glyph_entries = reduce_usize("max_glyph_entries", self.max_glyph_entries, value)?;
        Ok(self)
    }

    pub fn with_max_atlas_pages(mut self, value: usize) -> Result<Self, UiPresentationLimitsError> {
        self.max_atlas_pages = reduce_usize("max_atlas_pages", self.max_atlas_pages, value)?;
        Ok(self)
    }

    pub fn with_atlas_page_extent(mut self, value: u32) -> Result<Self, UiPresentationLimitsError> {
        if value == 0 || value > self.atlas_page_extent {
            return Err(UiPresentationLimitsError::CannotBroaden {
                field: "atlas_page_extent",
            });
        }
        self.atlas_page_extent = value;
        self.max_glyph_extent = self.max_glyph_extent.min(value);
        self.validate()?;
        Ok(self)
    }

    pub fn with_max_glyph_extent(mut self, value: u32) -> Result<Self, UiPresentationLimitsError> {
        if value == 0 || value > self.max_glyph_extent || value > self.atlas_page_extent {
            return Err(UiPresentationLimitsError::CannotBroaden {
                field: "max_glyph_extent",
            });
        }
        self.max_glyph_extent = value;
        Ok(self)
    }

    pub fn with_max_atlas_bytes(mut self, value: usize) -> Result<Self, UiPresentationLimitsError> {
        self.max_atlas_bytes = reduce_usize("max_atlas_bytes", self.max_atlas_bytes, value)?;
        self.validate()?;
        Ok(self)
    }

    pub fn with_max_new_glyphs_per_plan(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_new_glyphs_per_plan = reduce_usize(
            "max_new_glyphs_per_plan",
            self.max_new_glyphs_per_plan,
            value,
        )?;
        Ok(self)
    }

    pub fn with_max_staged_atlas_bytes(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_staged_atlas_bytes =
            reduce_usize("max_staged_atlas_bytes", self.max_staged_atlas_bytes, value)?;
        self.validate()?;
        Ok(self)
    }

    pub fn with_max_packet_retained_bytes(
        mut self,
        value: usize,
    ) -> Result<Self, UiPresentationLimitsError> {
        self.max_packet_retained_bytes = reduce_usize(
            "max_packet_retained_bytes",
            self.max_packet_retained_bytes,
            value,
        )?;
        Ok(self)
    }

    pub fn with_max_uniform_corner_radius(
        mut self,
        value: f32,
    ) -> Result<Self, UiPresentationLimitsError> {
        if !value.is_finite() || value < 0.0 || value > self.max_uniform_corner_radius {
            return Err(UiPresentationLimitsError::CannotBroaden {
                field: "max_uniform_corner_radius",
            });
        }
        self.max_uniform_corner_radius = value;
        Ok(self)
    }

    pub(crate) fn validate(self) -> Result<Self, UiPresentationLimitsError> {
        let page_bytes = self.checked_page_byte_len()?;
        if self.max_atlas_pages > usize::from(u16::MAX)
            || self.max_glyph_extent > self.atlas_page_extent
            || self.max_atlas_bytes < page_bytes
            || self.max_staged_atlas_bytes < page_bytes
        {
            return Err(UiPresentationLimitsError::InconsistentProfile);
        }
        Ok(self)
    }

    pub(crate) const fn max_expanded_primitives(self) -> usize {
        self.max_expanded_primitives
    }

    pub(crate) const fn max_referenced_images(self) -> usize {
        self.max_referenced_images
    }

    pub(crate) const fn max_referenced_glyphs(self) -> usize {
        self.max_referenced_glyphs
    }

    pub(crate) const fn max_glyph_entries(self) -> usize {
        self.max_glyph_entries
    }

    pub(crate) const fn max_atlas_pages(self) -> usize {
        self.max_atlas_pages
    }

    pub(crate) const fn atlas_page_extent(self) -> u32 {
        self.atlas_page_extent
    }

    pub(crate) const fn max_glyph_extent(self) -> u32 {
        self.max_glyph_extent
    }

    pub(crate) const fn max_atlas_bytes(self) -> usize {
        self.max_atlas_bytes
    }

    pub(crate) const fn max_new_glyphs_per_plan(self) -> usize {
        self.max_new_glyphs_per_plan
    }

    pub(crate) const fn max_staged_atlas_bytes(self) -> usize {
        self.max_staged_atlas_bytes
    }

    pub(crate) const fn max_packet_retained_bytes(self) -> usize {
        self.max_packet_retained_bytes
    }

    pub(crate) const fn max_uniform_corner_radius(self) -> f32 {
        self.max_uniform_corner_radius
    }

    pub(crate) fn checked_page_byte_len(self) -> Result<usize, UiPresentationLimitsError> {
        self.atlas_page_extent
            .checked_mul(self.atlas_page_extent)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or(UiPresentationLimitsError::ArithmeticOverflow)
    }
}

fn reduce_usize(
    field: &'static str,
    current: usize,
    value: usize,
) -> Result<usize, UiPresentationLimitsError> {
    if value > current {
        return Err(UiPresentationLimitsError::CannotBroaden { field });
    }
    Ok(value)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiPresentationLimitsError {
    #[error("UI presentation limit `{field}` cannot broaden the hard-capped baseline")]
    CannotBroaden { field: &'static str },
    #[error("UI presentation limits form an inconsistent bounded profile")]
    InconsistentProfile,
    #[error("UI presentation limit arithmetic overflowed")]
    ArithmeticOverflow,
}
