use nara_input::{InputSeatId, InputWindowId};
use nara_ui_kernel::{UiEnvironment, UiEnvironmentError, UiSurfaceId};
use thiserror::Error;

const INITIAL_SURFACE_REVISION: u64 = 1;

/// Host-owned metrics and input-route binding for one stable UI surface lifetime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiSurfaceDescriptor {
    surface_id: UiSurfaceId,
    input_window: InputWindowId,
    input_seat: InputSeatId,
    logical_width: f32,
    logical_height: f32,
    scale_factor: f32,
    input_scale_factor: f32,
    route_revision: u64,
    presentation_revision: u64,
    font_revision: u64,
}

impl UiSurfaceDescriptor {
    /// Creates the first descriptor in one surface lifetime.
    ///
    /// Route, presentation, and font revisions start at one. Once installed, revisions can only
    /// advance through the runtime-owned surface update functions.
    pub fn new(
        surface_id: UiSurfaceId,
        input_window: InputWindowId,
        input_seat: InputSeatId,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> Result<Self, UiEnvironmentError> {
        let descriptor = Self {
            surface_id,
            input_window,
            input_seat,
            logical_width,
            logical_height,
            scale_factor,
            input_scale_factor: 1.0,
            route_revision: INITIAL_SURFACE_REVISION,
            presentation_revision: INITIAL_SURFACE_REVISION,
            font_revision: INITIAL_SURFACE_REVISION,
        };
        descriptor.environment(1)?;
        Ok(descriptor)
    }

    #[must_use]
    pub const fn surface_id(self) -> UiSurfaceId {
        self.surface_id
    }

    #[must_use]
    pub const fn input_window(self) -> InputWindowId {
        self.input_window
    }

    #[must_use]
    pub const fn input_seat(self) -> InputSeatId {
        self.input_seat
    }

    #[must_use]
    pub const fn logical_size(self) -> (f32, f32) {
        (self.logical_width, self.logical_height)
    }

    #[must_use]
    pub const fn scale_factor(self) -> f32 {
        self.scale_factor
    }

    /// Returns the conversion from platform-logical coordinates into the authored design space.
    #[must_use]
    pub const fn input_scale_factor(self) -> f32 {
        self.input_scale_factor
    }

    #[must_use]
    pub const fn route_revision(self) -> u64 {
        self.route_revision
    }

    #[must_use]
    pub const fn presentation_revision(self) -> u64 {
        self.presentation_revision
    }

    #[must_use]
    pub const fn font_revision(self) -> u64 {
        self.font_revision
    }

    pub(super) fn environment(
        self,
        theme_revision: u64,
    ) -> Result<UiEnvironment, UiEnvironmentError> {
        UiEnvironment::new(
            self.surface_id,
            self.logical_width,
            self.logical_height,
            self.scale_factor,
            self.route_revision,
            self.presentation_revision,
            theme_revision,
            self.font_revision,
        )
    }

    /// Configures the render and input mapping before this descriptor is first installed.
    ///
    /// This builder does not advance revisions. An installed surface must use
    /// `update_ui_surface_mapping`, which derives from the runtime-owned current descriptor.
    pub fn with_surface_mapping(
        mut self,
        scale_factor: f32,
        input_scale_factor: f32,
    ) -> Result<Self, UiSurfaceMappingError> {
        validate_input_scale_factor(input_scale_factor)?;
        self.scale_factor = scale_factor;
        self.input_scale_factor = input_scale_factor;
        self.environment(1)?;
        Ok(self)
    }

    pub(super) fn update_logical_size(
        mut self,
        logical_width: f32,
        logical_height: f32,
    ) -> Result<Self, UiSurfaceMappingError> {
        if self.logical_width.to_bits() == logical_width.to_bits()
            && self.logical_height.to_bits() == logical_height.to_bits()
        {
            return Ok(self);
        }

        self.advance_route_revision()?;
        self.advance_presentation_revision()?;
        self.logical_width = logical_width;
        self.logical_height = logical_height;
        self.environment(1)?;
        Ok(self)
    }

    pub(super) fn update_input_endpoint(
        mut self,
        input_window: InputWindowId,
        input_seat: InputSeatId,
    ) -> Result<Self, UiSurfaceMappingError> {
        if self.input_window == input_window && self.input_seat == input_seat {
            return Ok(self);
        }

        self.advance_route_revision()?;
        self.input_window = input_window;
        self.input_seat = input_seat;
        Ok(self)
    }

    pub(super) fn update_surface_mapping(
        mut self,
        scale_factor: f32,
        input_scale_factor: f32,
    ) -> Result<Self, UiSurfaceMappingError> {
        validate_input_scale_factor(input_scale_factor)?;
        let render_changed = self.scale_factor.to_bits() != scale_factor.to_bits();
        let input_changed = self.input_scale_factor.to_bits() != input_scale_factor.to_bits();
        if !render_changed && !input_changed {
            return Ok(self);
        }

        self.advance_route_revision()?;
        if render_changed {
            self.advance_presentation_revision()?;
        }
        self.scale_factor = scale_factor;
        self.input_scale_factor = input_scale_factor;
        self.environment(1)?;
        Ok(self)
    }

    fn advance_route_revision(&mut self) -> Result<(), UiSurfaceMappingError> {
        self.route_revision =
            self.route_revision
                .checked_add(1)
                .ok_or(UiSurfaceMappingError::RevisionExhausted {
                    field: "route_revision",
                })?;
        Ok(())
    }

    fn advance_presentation_revision(&mut self) -> Result<(), UiSurfaceMappingError> {
        self.presentation_revision = self.presentation_revision.checked_add(1).ok_or(
            UiSurfaceMappingError::RevisionExhausted {
                field: "presentation_revision",
            },
        )?;
        Ok(())
    }
}

fn validate_input_scale_factor(input_scale_factor: f32) -> Result<(), UiSurfaceMappingError> {
    if !input_scale_factor.is_finite() || input_scale_factor <= 0.0 || input_scale_factor > 8.0 {
        return Err(UiSurfaceMappingError::InvalidInputScaleFactor {
            value: input_scale_factor,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum UiSurfaceMappingError {
    #[error("UI input scale factor {value} is invalid")]
    InvalidInputScaleFactor { value: f32 },
    #[error("UI surface mapping revision `{field}` was exhausted")]
    RevisionExhausted { field: &'static str },
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
}
