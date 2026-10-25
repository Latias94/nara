/// Tool policy refusals, separate from source/query/preparation/dispatch errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapRejection {
    InvalidGrid,
    NonFinitePosition,
    PositionOutOfRange,
    ResultOutOfRange,
    ObservationMismatch,
    ValueUnavailable,
}

impl std::fmt::Display for SnapRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGrid => "Grid spacing must be a positive finite number",
            Self::NonFinitePosition => "Both local coordinates must be finite",
            Self::PositionOutOfRange => {
                "Local position exceeds the Transform2d finite f32 decode range"
            }
            Self::ResultOutOfRange => {
                "Snapped position exceeds the Transform2d finite f32 decode range"
            }
            Self::ObservationMismatch => {
                "Preview requires coherent X/Y observations at the exact captured basis"
            }
            Self::ValueUnavailable => {
                "Both Transform2d translation fields must be present F64 source values"
            }
        })
    }
}
impl std::error::Error for SnapRejection {}

/// Nearest local-zero grid point, with exact binary distance ties away from zero.
///
/// Source values stay f64. The range check is the real Transform codec's finite f32
/// range, not an extra coordinate cap or an implicit f32 quantization.
pub fn snap_local_position(position: [f64; 2], grid: f64) -> Result<[f64; 2], SnapRejection> {
    validate_input(position, grid)?;
    let max = f64::from(f32::MAX);
    let result = position.map(|value| {
        let magnitude = value.abs();
        // Do not form value/grid: legal subnormal spacing can overflow that quotient.
        let remainder = magnitude % grid;
        if remainder == 0.0 {
            return value;
        }
        let up = grid - remainder;
        let snapped = if remainder < up {
            magnitude - remainder
        } else {
            magnitude + up
        };
        if snapped == 0.0 {
            0.0
        } else {
            snapped.copysign(value)
        }
    });
    if result
        .iter()
        .any(|value| !value.is_finite() || value.abs() > max)
    {
        return Err(SnapRejection::ResultOutOfRange);
    }
    Ok(result)
}

pub(super) fn validate_input(position: [f64; 2], grid: f64) -> Result<(), SnapRejection> {
    if !grid.is_finite() || grid <= 0.0 {
        return Err(SnapRejection::InvalidGrid);
    }
    if position.iter().any(|value| !value.is_finite()) {
        return Err(SnapRejection::NonFinitePosition);
    }
    let max = f64::from(f32::MAX);
    if position.iter().any(|value| value.abs() > max) {
        return Err(SnapRejection::PositionOutOfRange);
    }
    Ok(())
}
