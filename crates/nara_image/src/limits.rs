use std::{
    fmt::{self, Display, Formatter},
    num::{NonZeroU32, NonZeroU64},
};

use nara_core::ByteLimit;

const MIB: usize = 1024 * 1024;
const DEFAULT_MAX_ENCODED_BYTES: usize = 16 * MIB;
const DEFAULT_MAX_WIDTH: u32 = 16_384;
const DEFAULT_MAX_HEIGHT: u32 = 16_384;
const DEFAULT_MAX_PIXELS: u64 = 16_777_216;
const DEFAULT_MAX_RGBA_BYTES: usize = 64 * MIB;
const DEFAULT_MAX_DECODER_WORK_BYTES: usize = 16 * MIB;
const DEFAULT_MAX_IN_FLIGHT_BYTES: usize = 512 * MIB;

const HARD_MAX_ENCODED_BYTES: usize = 64 * MIB;
const HARD_MAX_WIDTH: u32 = 32_768;
const HARD_MAX_HEIGHT: u32 = 32_768;
const HARD_MAX_PIXELS: u64 = 67_108_864;
const HARD_MAX_RGBA_BYTES: usize = 256 * MIB;
const HARD_MAX_DECODER_WORK_BYTES: usize = 64 * MIB;
const HARD_MAX_IN_FLIGHT_BYTES: usize = if usize::BITS >= 64 {
    2 * 1024 * MIB
} else {
    isize::MAX as usize
};

pub(crate) const PNG_TRACKED_DECODER_BYTES: usize = 8 * MIB;
const PNG_UNTRACKED_BASE_BYTES: usize = 512 * 1024;
const PNG_ROW_SLACK_MULTIPLIER: usize = 3;

/// Version of Nara's conservative logical PNG peak formula.
pub const IMAGE_IMPORT_MEMORY_PLAN_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageImportLimitKind {
    EncodedBytes,
    Width,
    Height,
    Pixels,
    RgbaBytes,
    DecoderWorkBytes,
    AggregateInFlightBytes,
}

impl ImageImportLimitKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EncodedBytes => "encoded-bytes",
            Self::Width => "width",
            Self::Height => "height",
            Self::Pixels => "pixels",
            Self::RgbaBytes => "rgba-bytes",
            Self::DecoderWorkBytes => "decoder-work-bytes",
            Self::AggregateInFlightBytes => "aggregate-in-flight-bytes",
        }
    }
}

impl Display for ImageImportLimitKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageImportLimits {
    max_encoded_bytes: ByteLimit,
    max_width: NonZeroU32,
    max_height: NonZeroU32,
    max_pixels: NonZeroU64,
    max_rgba_bytes: ByteLimit,
    max_decoder_work_bytes: ByteLimit,
    max_in_flight_bytes: ByteLimit,
}

impl Default for ImageImportLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: ByteLimit::new(DEFAULT_MAX_ENCODED_BYTES)
                .expect("default encoded byte limit is non-zero"),
            max_width: NonZeroU32::new(DEFAULT_MAX_WIDTH)
                .expect("default image width limit is non-zero"),
            max_height: NonZeroU32::new(DEFAULT_MAX_HEIGHT)
                .expect("default image height limit is non-zero"),
            max_pixels: NonZeroU64::new(DEFAULT_MAX_PIXELS)
                .expect("default image pixel limit is non-zero"),
            max_rgba_bytes: ByteLimit::new(DEFAULT_MAX_RGBA_BYTES)
                .expect("default RGBA byte limit is non-zero"),
            max_decoder_work_bytes: ByteLimit::new(DEFAULT_MAX_DECODER_WORK_BYTES)
                .expect("default decoder work byte limit is non-zero"),
            max_in_flight_bytes: ByteLimit::new(DEFAULT_MAX_IN_FLIGHT_BYTES)
                .expect("default aggregate image limit is non-zero"),
        }
    }
}

impl ImageImportLimits {
    #[must_use]
    pub const fn max_encoded_bytes(self) -> ByteLimit {
        self.max_encoded_bytes
    }

    #[must_use]
    pub const fn max_width(self) -> NonZeroU32 {
        self.max_width
    }

    #[must_use]
    pub const fn max_height(self) -> NonZeroU32 {
        self.max_height
    }

    #[must_use]
    pub const fn max_pixels(self) -> NonZeroU64 {
        self.max_pixels
    }

    #[must_use]
    pub const fn max_rgba_bytes(self) -> ByteLimit {
        self.max_rgba_bytes
    }

    #[must_use]
    pub const fn max_decoder_work_bytes(self) -> ByteLimit {
        self.max_decoder_work_bytes
    }

    #[must_use]
    pub const fn max_in_flight_bytes(self) -> ByteLimit {
        self.max_in_flight_bytes
    }

    #[must_use]
    pub const fn with_max_encoded_bytes(mut self, limit: ByteLimit) -> Self {
        self.max_encoded_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_max_width(mut self, limit: NonZeroU32) -> Self {
        self.max_width = limit;
        self
    }

    #[must_use]
    pub const fn with_max_height(mut self, limit: NonZeroU32) -> Self {
        self.max_height = limit;
        self
    }

    #[must_use]
    pub const fn with_max_pixels(mut self, limit: NonZeroU64) -> Self {
        self.max_pixels = limit;
        self
    }

    #[must_use]
    pub const fn with_max_rgba_bytes(mut self, limit: ByteLimit) -> Self {
        self.max_rgba_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_max_decoder_work_bytes(mut self, limit: ByteLimit) -> Self {
        self.max_decoder_work_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_max_in_flight_bytes(mut self, limit: ByteLimit) -> Self {
        self.max_in_flight_bytes = limit;
        self
    }

    pub(crate) fn validate(self) -> Result<Self, ImageImportLimitsError> {
        validate_hard_limit(
            ImageImportLimitKind::EncodedBytes,
            self.max_encoded_bytes.get() as u64,
            HARD_MAX_ENCODED_BYTES as u64,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::Width,
            self.max_width.get() as u64,
            HARD_MAX_WIDTH as u64,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::Height,
            self.max_height.get() as u64,
            HARD_MAX_HEIGHT as u64,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::Pixels,
            self.max_pixels.get(),
            HARD_MAX_PIXELS,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::RgbaBytes,
            self.max_rgba_bytes.get() as u64,
            HARD_MAX_RGBA_BYTES as u64,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::DecoderWorkBytes,
            self.max_decoder_work_bytes.get() as u64,
            HARD_MAX_DECODER_WORK_BYTES as u64,
        )?;
        validate_hard_limit(
            ImageImportLimitKind::AggregateInFlightBytes,
            self.max_in_flight_bytes.get() as u64,
            HARD_MAX_IN_FLIGHT_BYTES as u64,
        )?;

        let required =
            file_admission_ceiling(self.max_encoded_bytes.get(), self.max_rgba_bytes.get());
        if self.max_in_flight_bytes.get() < required {
            return Err(ImageImportLimitsError::AggregateBelowFileAdmissionCeiling {
                aggregate: self.max_in_flight_bytes.get() as u64,
                required: required as u64,
            });
        }
        Ok(self)
    }
}

pub(crate) const fn file_admission_ceiling(
    encoded_bytes: usize,
    publication_overlap_bytes: usize,
) -> usize {
    encoded_bytes.saturating_add(publication_overlap_bytes)
}

fn validate_hard_limit(
    kind: ImageImportLimitKind,
    configured: u64,
    hard_maximum: u64,
) -> Result<(), ImageImportLimitsError> {
    if configured > hard_maximum {
        Err(ImageImportLimitsError::HardMaximumExceeded {
            kind,
            configured,
            hard_maximum,
        })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageImportLimitsError {
    HardMaximumExceeded {
        kind: ImageImportLimitKind,
        configured: u64,
        hard_maximum: u64,
    },
    AggregateBelowFileAdmissionCeiling {
        aggregate: u64,
        required: u64,
    },
}

impl Display for ImageImportLimitsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::HardMaximumExceeded {
                kind,
                configured,
                hard_maximum,
            } => write!(
                formatter,
                "image {kind} limit {configured} exceeds the engine hard maximum {hard_maximum}"
            ),
            Self::AggregateBelowFileAdmissionCeiling {
                aggregate,
                required,
            } => write!(
                formatter,
                "aggregate image limit {aggregate} is below the bounded file admission ceiling {required}"
            ),
        }
    }
}

impl std::error::Error for ImageImportLimitsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageImportBudgetError {
    kind: ImageImportLimitKind,
    observed: Option<u64>,
    limit: u64,
    in_use: Option<u64>,
}

impl ImageImportBudgetError {
    pub(crate) const fn per_image(
        kind: ImageImportLimitKind,
        observed: Option<u64>,
        limit: u64,
    ) -> Self {
        Self {
            kind,
            observed,
            limit,
            in_use: None,
        }
    }

    pub(crate) const fn aggregate(requested: u64, in_use: u64, limit: u64) -> Self {
        Self {
            kind: ImageImportLimitKind::AggregateInFlightBytes,
            observed: Some(requested),
            limit,
            in_use: Some(in_use),
        }
    }

    #[must_use]
    pub const fn kind(self) -> ImageImportLimitKind {
        self.kind
    }

    #[must_use]
    pub const fn observed(self) -> Option<u64> {
        self.observed
    }

    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }

    #[must_use]
    pub const fn in_use(self) -> Option<u64> {
        self.in_use
    }
}

impl Display for ImageImportBudgetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "image {} limit was exceeded", self.kind)
    }
}

impl std::error::Error for ImageImportBudgetError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageImportMemoryPlan {
    encoded_bytes: usize,
    encoded_allocation_bytes: usize,
    rgba_bytes: usize,
    decoder_work_bytes: usize,
    publication_overlap_bytes: usize,
    peak_bytes: usize,
    publication_bytes: usize,
}

impl ImageImportMemoryPlan {
    pub(crate) fn for_png(
        limits: ImageImportLimits,
        encoded_bytes: usize,
        encoded_allocation_bytes: usize,
        width: u32,
        height: u32,
        publication_overlap_bytes: usize,
    ) -> Result<Self, ImageImportBudgetError> {
        enforce_limit(
            ImageImportLimitKind::EncodedBytes,
            encoded_bytes as u64,
            limits.max_encoded_bytes.get() as u64,
        )?;
        enforce_limit(
            ImageImportLimitKind::Width,
            width as u64,
            limits.max_width.get() as u64,
        )?;
        enforce_limit(
            ImageImportLimitKind::Height,
            height as u64,
            limits.max_height.get() as u64,
        )?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::Pixels,
                    None,
                    limits.max_pixels.get(),
                )
            })?;
        enforce_limit(
            ImageImportLimitKind::Pixels,
            pixels,
            limits.max_pixels.get(),
        )?;
        let rgba_bytes_u64 = pixels.checked_mul(4).ok_or_else(|| {
            ImageImportBudgetError::per_image(
                ImageImportLimitKind::RgbaBytes,
                None,
                limits.max_rgba_bytes.get() as u64,
            )
        })?;
        enforce_limit(
            ImageImportLimitKind::RgbaBytes,
            rgba_bytes_u64,
            limits.max_rgba_bytes.get() as u64,
        )?;
        let rgba_bytes = usize::try_from(rgba_bytes_u64).map_err(|_| {
            ImageImportBudgetError::per_image(
                ImageImportLimitKind::RgbaBytes,
                None,
                limits.max_rgba_bytes.get() as u64,
            )
        })?;
        let rgba_row_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::DecoderWorkBytes,
                    None,
                    u64::MAX,
                )
            })?;
        let decoder_work_bytes = PNG_TRACKED_DECODER_BYTES
            .checked_add(PNG_UNTRACKED_BASE_BYTES)
            .and_then(|bytes| {
                rgba_row_bytes
                    .checked_mul(PNG_ROW_SLACK_MULTIPLIER)
                    .and_then(|rows| bytes.checked_add(rows))
            })
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::DecoderWorkBytes,
                    None,
                    u64::MAX,
                )
            })?;
        enforce_limit(
            ImageImportLimitKind::DecoderWorkBytes,
            decoder_work_bytes as u64,
            limits.max_decoder_work_bytes.get() as u64,
        )?;
        let publication_bytes = rgba_bytes
            .checked_add(publication_overlap_bytes)
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::AggregateInFlightBytes,
                    None,
                    limits.max_in_flight_bytes.get() as u64,
                )
            })?;
        let peak_bytes = encoded_allocation_bytes
            .checked_add(decoder_work_bytes)
            .and_then(|bytes| bytes.checked_add(publication_bytes))
            .ok_or_else(|| {
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::AggregateInFlightBytes,
                    None,
                    limits.max_in_flight_bytes.get() as u64,
                )
            })?;

        Ok(Self {
            encoded_bytes,
            encoded_allocation_bytes,
            rgba_bytes,
            decoder_work_bytes,
            publication_overlap_bytes,
            peak_bytes,
            publication_bytes,
        })
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        IMAGE_IMPORT_MEMORY_PLAN_VERSION
    }

    #[must_use]
    pub const fn encoded_bytes(self) -> usize {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn encoded_allocation_bytes(self) -> usize {
        self.encoded_allocation_bytes
    }

    #[must_use]
    pub const fn rgba_bytes(self) -> usize {
        self.rgba_bytes
    }

    #[must_use]
    pub const fn decoder_work_bytes(self) -> usize {
        self.decoder_work_bytes
    }

    #[must_use]
    pub const fn publication_overlap_bytes(self) -> usize {
        self.publication_overlap_bytes
    }

    #[must_use]
    pub const fn peak_bytes(self) -> usize {
        self.peak_bytes
    }

    pub(crate) const fn publication_bytes(self) -> usize {
        self.publication_bytes
    }
}

fn enforce_limit(
    kind: ImageImportLimitKind,
    observed: u64,
    limit: u64,
) -> Result<(), ImageImportBudgetError> {
    if observed > limit {
        Err(ImageImportBudgetError::per_image(
            kind,
            Some(observed),
            limit,
        ))
    } else {
        Ok(())
    }
}
