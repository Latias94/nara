//! Bounded backend-neutral text shaping, layout, and glyph raster requests.

mod engine;
mod face;
mod layout;
mod limits;
mod raster;

pub use engine::{TextEngine, TextEngineCreateError};
pub use face::{
    BUNDLED_FONT_BYTE_LEN, BUNDLED_FONT_LICENSE_SHA256, BUNDLED_FONT_SHA256, FontFace,
    FontFaceError, FontFaceId, MAX_FONT_CONTAINER_BYTES, bundled_font_face,
};
pub use layout::{
    GlyphRasterKey, PositionedGlyph, TextAlignment, TextDiagnostic, TextDirection, TextLayout,
    TextLayoutError, TextLayoutRequest, TextLine, TextRun,
};
pub use limits::TextLimits;
pub use raster::{GlyphRaster, GlyphRasterError};
