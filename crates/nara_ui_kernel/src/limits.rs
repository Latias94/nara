use thiserror::Error;

const HARD_MAX_ROOTS: usize = 64;
const HARD_MAX_NODES: usize = 4_096;
const HARD_MAX_DEPTH: usize = 64;
const HARD_MAX_KEY_BYTES: usize = 256;
const HARD_MAX_PAINT_PRIMITIVES: usize = 8_192;
const HARD_MAX_DIAGNOSTICS: usize = 64;
const HARD_MAX_CACHE_ROOTS: usize = 64;
const HARD_MAX_SURFACES: usize = 16;

/// Owner-local resource limits. Callers may reduce but never widen the hard ceilings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiLimits {
    pub(crate) max_roots: usize,
    pub(crate) max_nodes: usize,
    pub(crate) max_depth: usize,
    pub(crate) max_key_bytes: usize,
    pub(crate) max_paint_primitives: usize,
    pub(crate) max_diagnostics: usize,
    pub(crate) max_cache_roots: usize,
    pub(crate) max_surfaces: usize,
}

impl Default for UiLimits {
    fn default() -> Self {
        Self {
            max_roots: HARD_MAX_ROOTS,
            max_nodes: HARD_MAX_NODES,
            max_depth: HARD_MAX_DEPTH,
            max_key_bytes: HARD_MAX_KEY_BYTES,
            max_paint_primitives: HARD_MAX_PAINT_PRIMITIVES,
            max_diagnostics: HARD_MAX_DIAGNOSTICS,
            max_cache_roots: HARD_MAX_CACHE_ROOTS,
            max_surfaces: HARD_MAX_SURFACES,
        }
    }
}

impl UiLimits {
    pub fn with_max_roots(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_roots", value, HARD_MAX_ROOTS)?;
        self.max_roots = value;
        Ok(self)
    }

    pub fn with_max_nodes(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_nodes", value, HARD_MAX_NODES)?;
        self.max_nodes = value;
        Ok(self)
    }

    pub fn with_max_depth(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_depth", value, HARD_MAX_DEPTH)?;
        self.max_depth = value;
        Ok(self)
    }

    pub fn with_max_key_bytes(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_key_bytes", value, HARD_MAX_KEY_BYTES)?;
        self.max_key_bytes = value;
        Ok(self)
    }

    pub fn with_max_paint_primitives(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_paint_primitives", value, HARD_MAX_PAINT_PRIMITIVES)?;
        self.max_paint_primitives = value;
        Ok(self)
    }

    pub fn with_max_diagnostics(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_diagnostics", value, HARD_MAX_DIAGNOSTICS)?;
        self.max_diagnostics = value;
        Ok(self)
    }

    pub fn with_max_cache_roots(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_cache_roots", value, HARD_MAX_CACHE_ROOTS)?;
        self.max_cache_roots = value;
        Ok(self)
    }

    pub fn with_max_surfaces(mut self, value: usize) -> Result<Self, UiLimitsError> {
        validate("max_surfaces", value, HARD_MAX_SURFACES)?;
        self.max_surfaces = value;
        Ok(self)
    }

    pub(crate) fn validate(self) -> Result<(), UiLimitsError> {
        validate("max_roots", self.max_roots, HARD_MAX_ROOTS)?;
        validate("max_nodes", self.max_nodes, HARD_MAX_NODES)?;
        validate("max_depth", self.max_depth, HARD_MAX_DEPTH)?;
        validate("max_key_bytes", self.max_key_bytes, HARD_MAX_KEY_BYTES)?;
        validate(
            "max_paint_primitives",
            self.max_paint_primitives,
            HARD_MAX_PAINT_PRIMITIVES,
        )?;
        validate(
            "max_diagnostics",
            self.max_diagnostics,
            HARD_MAX_DIAGNOSTICS,
        )?;
        validate(
            "max_cache_roots",
            self.max_cache_roots,
            HARD_MAX_CACHE_ROOTS,
        )?;
        validate("max_surfaces", self.max_surfaces, HARD_MAX_SURFACES)
    }
}

fn validate(field: &'static str, actual: usize, maximum: usize) -> Result<(), UiLimitsError> {
    if actual == 0 || actual > maximum {
        Err(UiLimitsError::InvalidField {
            field,
            actual,
            maximum,
        })
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum UiLimitsError {
    #[error("UI limit {field} is {actual}; expected 1..={maximum}")]
    InvalidField {
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
}
