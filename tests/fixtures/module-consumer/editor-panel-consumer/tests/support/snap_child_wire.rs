//! Private, one-shot experiment frames. This is not a versioned engine/tool SDK.

pub const REQUEST_BYTES: usize = 40;
pub const RESPONSE_BYTES: usize = 32;
pub const STDERR_BYTES: usize = 4096;
const HEADER: [u8; 8] = [b'N', b'S', b'N', b'P', 1, 0, 0, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    InvalidGrid = 1,
    NonFinitePosition = 2,
    PositionOutOfRange = 3,
    ResultOutOfRange = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

pub fn request(sequence: u64, grid: f64, position: [f64; 2]) -> [u8; REQUEST_BYTES] {
    let mut bytes = [0; REQUEST_BYTES];
    bytes[..8].copy_from_slice(&HEADER);
    bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
    for (offset, value) in [grid, position[0], position[1]].into_iter().enumerate() {
        bytes[16 + offset * 8..24 + offset * 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn read_request(bytes: &[u8; REQUEST_BYTES]) -> Result<(u64, f64, [f64; 2]), Malformed> {
    if bytes[..8] != HEADER {
        return Err(Malformed);
    }
    let sequence = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| Malformed)?);
    let number = |start| -> Result<f64, Malformed> {
        Ok(f64::from_le_bytes(
            bytes[start..start + 8].try_into().map_err(|_| Malformed)?,
        ))
    };
    Ok((sequence, number(16)?, [number(24)?, number(32)?]))
}

pub fn response(sequence: u64, value: Result<[f64; 2], Refusal>) -> [u8; RESPONSE_BYTES] {
    let mut bytes = [0; RESPONSE_BYTES];
    bytes[..8].copy_from_slice(&HEADER);
    bytes[8..16].copy_from_slice(&sequence.to_le_bytes());
    match value {
        Ok(position) => {
            bytes[16..24].copy_from_slice(&position[0].to_le_bytes());
            bytes[24..32].copy_from_slice(&position[1].to_le_bytes());
        }
        Err(reason) => {
            bytes[5] = 1;
            bytes[6] = reason as u8;
        }
    }
    bytes
}

pub fn read_response(bytes: &[u8], sequence: u64) -> Result<Result<[f64; 2], Refusal>, Malformed> {
    if bytes.len() != RESPONSE_BYTES
        || bytes[..5] != HEADER[..5]
        || bytes[7] != 0
        || bytes[8..16] != sequence.to_le_bytes()
    {
        return Err(Malformed);
    }
    match (bytes[5], bytes[6]) {
        (0, 0) => {
            let x = f64::from_le_bytes(bytes[16..24].try_into().map_err(|_| Malformed)?);
            let y = f64::from_le_bytes(bytes[24..32].try_into().map_err(|_| Malformed)?);
            if [x, y]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > f64::from(f32::MAX))
            {
                return Err(Malformed);
            }
            Ok(Ok([x, y]))
        }
        (1, code) if bytes[16..].iter().all(|byte| *byte == 0) => {
            let reason = match code {
                1 => Refusal::InvalidGrid,
                2 => Refusal::NonFinitePosition,
                3 => Refusal::PositionOutOfRange,
                4 => Refusal::ResultOutOfRange,
                _ => return Err(Malformed),
            };
            Ok(Err(reason))
        }
        _ => Err(Malformed),
    }
}
