//! Version-one wire format: a 64-byte header followed by Pickup and Travel PCM16 frames.
//!
//! The header contains an eight-byte magic, u32 version, sixteen source UUID bytes, a u8
//! Product tag and sixteen Product UUID bytes (zero for primary), u32 sample rate, u16 channels,
//! u16 bit depth, u8 signed-little-endian encoding, u16 cue count and two u32 frame counts.
//! All integers are little endian. Every descriptor and the complete byte length are checked
//! before sample allocation; no source names, paths, settings or trusted hashes are encoded.

use nara::asset::{
    AssetProductRef, AssetReferenceDemand, AssetRuntimeError, AssetRuntimeLimits, RuntimeAssetCodec,
};

use super::{
    MAX_CLIP_FRAMES, MAX_RETAINED_BYTES, MAX_SAMPLE_BYTES, Rejection, SAMPLE_RATE, SoundBank,
    SoundBankCodec, bank_product, byte_limit, wav,
};

const MAGIC: &[u8; 8] = b"NARASBNK";
const HEADER_BYTES: usize = 64;
const MAX_RUNTIME_BYTES: usize = HEADER_BYTES + 2 * MAX_CLIP_FRAMES * 2;

impl RuntimeAssetCodec<SoundBank> for SoundBankCodec {
    const RUNTIME_KIND: &'static str = "two_level_game_sound_bank";
    const RUNTIME_VERSION: u32 = 1;

    /// Covers overlapping Vec/Arc sample conversion and encoded Vec/boxed output allocations.
    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            byte_limit(MAX_RUNTIME_BYTES),
            byte_limit(2 * MAX_SAMPLE_BYTES + 2 * MAX_RUNTIME_BYTES + 4096),
            byte_limit(MAX_RETAINED_BYTES),
        )
    }

    fn encode_runtime(product: &SoundBank) -> Result<Box<[u8]>, AssetRuntimeError> {
        validate_product(product).map_err(runtime_error)?;
        let length = HEADER_BYTES + 2 * (product.pickup.len() + product.travel.len());
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&Self::RUNTIME_VERSION.to_le_bytes());
        bytes.extend_from_slice(product.product().source().as_uuid().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&[0; 16]);
        bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&(product.pickup.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(product.travel.len() as u32).to_le_bytes());
        for sample in product.pickup.iter().chain(product.travel.iter()) {
            bytes.extend_from_slice(&((*sample * 32_768.0) as i16).to_le_bytes());
        }
        Ok(bytes.into_boxed_slice())
    }

    fn decode_runtime(bytes: &[u8]) -> Result<SoundBank, AssetRuntimeError> {
        preflight(bytes).map_err(runtime_error)?;
        let pickup_frames = u32_at(bytes, 56) as usize;
        let split = HEADER_BYTES + pickup_frames * 2;
        Ok(SoundBank {
            product: bank_product(),
            pickup: wav::samples(&bytes[HEADER_BYTES..split]),
            travel: wav::samples(&bytes[split..]),
        })
    }

    fn runtime_retained_bytes(product: &SoundBank) -> Result<usize, AssetRuntimeError> {
        validate_product(product).map_err(runtime_error)?;
        Ok(product.retained_bytes())
    }

    fn visit_runtime_dependencies<E>(
        _product: &SoundBank,
        _visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(product: &SoundBank, reference: AssetProductRef) -> bool {
        product.product() == reference && reference == bank_product()
    }
}

fn preflight(bytes: &[u8]) -> Result<(), Rejection> {
    if bytes.len() > MAX_RUNTIME_BYTES {
        return Err(Rejection::RuntimeBounds);
    }
    if bytes.len() < HEADER_BYTES || &bytes[..8] != MAGIC {
        return Err(Rejection::RuntimeHeader);
    }
    if u32_at(bytes, 8) != SoundBankCodec::RUNTIME_VERSION {
        return Err(Rejection::RuntimeVersion);
    }
    if &bytes[12..28] != bank_product().source().as_uuid().as_bytes()
        || bytes[28] != 0
        || bytes[29..45] != [0; 16]
    {
        return Err(Rejection::RuntimeIdentity);
    }
    if u32_at(bytes, 45) != SAMPLE_RATE
        || bytes[49..51] != 1_u16.to_le_bytes()
        || bytes[51..53] != 16_u16.to_le_bytes()
        || bytes[53] != 1
        || bytes[54..56] != 2_u16.to_le_bytes()
    {
        return Err(Rejection::RuntimeFormat);
    }
    let pickup_frames = u32_at(bytes, 56) as usize;
    let travel_frames = u32_at(bytes, 60) as usize;
    if !(1..=MAX_CLIP_FRAMES).contains(&pickup_frames)
        || !(1..=MAX_CLIP_FRAMES).contains(&travel_frames)
        || bytes.len() != HEADER_BYTES + 2 * (pickup_frames + travel_frames)
    {
        return Err(Rejection::RuntimeLength);
    }
    Ok(())
}

fn validate_product(product: &SoundBank) -> Result<(), Rejection> {
    if product.product() != bank_product() {
        return Err(Rejection::RuntimeIdentity);
    }
    for samples in [&product.pickup, &product.travel] {
        if !(1..=MAX_CLIP_FRAMES).contains(&samples.len()) {
            return Err(Rejection::RuntimeLength);
        }
        for &sample in samples.iter() {
            let scaled = sample * 32_768.0;
            if !sample.is_finite()
                || scaled < f32::from(i16::MIN)
                || scaled > f32::from(i16::MAX)
                || (f32::from(scaled as i16) / 32_768.0).to_bits() != sample.to_bits()
            {
                return Err(Rejection::CanonicalSamples);
            }
        }
    }
    Ok(())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn runtime_error(error: Rejection) -> AssetRuntimeError {
    AssetRuntimeError::new(error.diagnostic())
}
