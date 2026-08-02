use std::io::Cursor;

use nara_asset::{AssetRecord, ImportRequest};
use nara_tasks::TaskCancellationToken;

use crate::budget::{ImageImportCharge, ImageImportReservation};
use crate::limits::{
    ImageImportBudgetError, ImageImportLimits, ImageImportMemoryPlan, PNG_TRACKED_DECODER_BYTES,
};
use crate::{ImageAsset, ImageExtent, ImageFormat, ImageImportLimitKind, ImageSourceMetadata};

use super::{
    ImageImportError, ImageImportStage, ImageImporter, ImagePngFailureKind,
    ImageUnsupportedFeature, validate_extension,
};

impl ImageImporter {
    pub(super) fn decode_png(
        &self,
        request: ImportRequest<'_>,
        encoded_allocation_bytes: usize,
        publication_overlap_bytes: usize,
        reservation: &mut ImageImportReservation,
        cancellation: Option<&TaskCancellationToken>,
    ) -> Result<(ImageAsset, ImageImportMemoryPlan), ImageImportError> {
        validate_extension(request.source().path())?;
        check_cancelled(cancellation, ImageImportStage::Header)?;

        let (preflight, memory_plan) = self.preflight_png_memory_plan(
            request.source(),
            request.source_bytes(),
            encoded_allocation_bytes,
            publication_overlap_bytes,
        )?;
        self.decode_png_with_preflight(request, preflight, memory_plan, reservation, cancellation)
    }

    pub(super) fn decode_png_with_preflight(
        &self,
        request: ImportRequest<'_>,
        preflight: PngHeaderPreflight,
        memory_plan: ImageImportMemoryPlan,
        reservation: &mut ImageImportReservation,
        cancellation: Option<&TaskCancellationToken>,
    ) -> Result<(ImageAsset, ImageImportMemoryPlan), ImageImportError> {
        reservation
            .resize(ImageImportCharge::peak(memory_plan))
            .map_err(|error| ImageImportError::budget(ImageImportStage::Metadata, error))?;

        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(request.source_bytes()),
            png::Limits {
                bytes: PNG_TRACKED_DECODER_BYTES,
            },
        );
        decoder.set_ignore_text_chunk(true);
        decoder.set_ignore_iccp_chunk(true);
        decoder.set_transformations(png::Transformations::normalize_to_color8());

        let header = decoder
            .read_header_info()
            .map_err(|error| map_png_error(ImageImportStage::Header, error))?;
        if header.width != preflight.width
            || header.height != preflight.height
            || header.interlaced != preflight.interlaced
        {
            return Err(ImageImportError::Png {
                stage: ImageImportStage::Header,
                kind: ImagePngFailureKind::DecoderContract,
            });
        }
        let width = preflight.width;
        let height = preflight.height;
        check_cancelled(cancellation, ImageImportStage::Metadata)?;

        let mut reader = decoder
            .read_info()
            .map_err(|error| map_png_error(ImageImportStage::Metadata, error))?;
        if reader.info().animation_control.is_some() {
            return Err(ImageImportError::Unsupported {
                stage: ImageImportStage::Metadata,
                feature: ImageUnsupportedFeature::Animation,
            });
        }
        let (output_color, output_depth) = reader.output_color_type();
        if output_depth != png::BitDepth::Eight {
            return Err(ImageImportError::Unsupported {
                stage: ImageImportStage::Metadata,
                feature: ImageUnsupportedFeature::OutputColorModel,
            });
        }
        let row_bytes = reader.output_line_size(width).ok_or_else(|| {
            ImageImportError::budget(
                ImageImportStage::Metadata,
                ImageImportBudgetError::per_image(
                    ImageImportLimitKind::DecoderWorkBytes,
                    None,
                    memory_plan.decoder_work_bytes() as u64,
                ),
            )
        })?;
        let mut native_row = allocate_zeroed(row_bytes, ImageImportStage::Decode)?;
        let mut rgba = allocate_zeroed(memory_plan.rgba_bytes(), ImageImportStage::Decode)?;
        let rgba_row_bytes = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or_else(|| {
                ImageImportError::budget(
                    ImageImportStage::Decode,
                    ImageImportBudgetError::per_image(
                        ImageImportLimitKind::RgbaBytes,
                        None,
                        self.limits.max_rgba_bytes().get() as u64,
                    ),
                )
            })?;

        for row_index in 0..height {
            check_cancelled(cancellation, ImageImportStage::Decode)?;
            let interlace = reader
                .read_row(&mut native_row)
                .map_err(|error| map_png_error(ImageImportStage::Decode, error))?
                .ok_or(ImageImportError::Png {
                    stage: ImageImportStage::Decode,
                    kind: ImagePngFailureKind::Truncated,
                })?;
            if !matches!(interlace, png::InterlaceInfo::Null(_)) {
                return Err(ImageImportError::Unsupported {
                    stage: ImageImportStage::Decode,
                    feature: ImageUnsupportedFeature::Interlacing,
                });
            }
            let destination_start = usize::try_from(row_index)
                .ok()
                .and_then(|row| row.checked_mul(rgba_row_bytes))
                .ok_or_else(|| {
                    ImageImportError::budget(
                        ImageImportStage::Decode,
                        ImageImportBudgetError::per_image(
                            ImageImportLimitKind::RgbaBytes,
                            None,
                            self.limits.max_rgba_bytes().get() as u64,
                        ),
                    )
                })?;
            let destination = &mut rgba[destination_start..destination_start + rgba_row_bytes];
            write_rgba8_row(output_color, &native_row, destination, width)?;
        }
        if reader
            .read_row(&mut native_row)
            .map_err(|error| map_png_error(ImageImportStage::Finalize, error))?
            .is_some()
        {
            return Err(ImageImportError::Png {
                stage: ImageImportStage::Finalize,
                kind: ImagePngFailureKind::InvalidData,
            });
        }
        reader
            .finish()
            .map_err(|error| map_png_error(ImageImportStage::Finalize, error))?;
        check_cancelled(cancellation, ImageImportStage::Finalize)?;
        drop(native_row);
        drop(reader);

        let artifact = self.import_record(&request)?;
        let image = ImageAsset::new(
            ImageSourceMetadata::new(
                request.source().stable_id(),
                request.source().path().clone(),
                request.source_hash(),
                artifact,
            ),
            ImageExtent::new(width, height),
            ImageFormat::Rgba8,
            self.color_space,
            rgba,
        )
        .map_err(|_| ImageImportError::Png {
            stage: ImageImportStage::Finalize,
            kind: ImagePngFailureKind::DecoderContract,
        })?;
        Ok((image, memory_plan))
    }

    pub(super) fn preflight_png_memory_plan(
        &self,
        source: &AssetRecord,
        source_bytes: &[u8],
        encoded_allocation_bytes: usize,
        publication_overlap_bytes: usize,
    ) -> Result<(PngHeaderPreflight, ImageImportMemoryPlan), ImageImportError> {
        validate_extension(source.path())?;
        let preflight = preflight_png(source_bytes)?;
        if preflight.interlaced {
            return Err(ImageImportError::Unsupported {
                stage: ImageImportStage::Header,
                feature: ImageUnsupportedFeature::Interlacing,
            });
        }
        reject_unbounded_metadata(source_bytes)?;
        let memory_plan = ImageImportMemoryPlan::for_png(
            self.limits,
            source_bytes.len(),
            encoded_allocation_bytes,
            preflight.width,
            preflight.height,
            publication_overlap_bytes,
        )
        .map_err(|error| ImageImportError::budget(plan_error_stage(error), error))?;
        Ok((preflight, memory_plan))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PngHeaderPreflight {
    width: u32,
    height: u32,
    interlaced: bool,
}

fn preflight_png(bytes: &[u8]) -> Result<PngHeaderPreflight, ImageImportError> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    const IHDR_BYTES: usize = 33;

    if bytes.len() < IHDR_BYTES {
        return Err(ImageImportError::Png {
            stage: ImageImportStage::Header,
            kind: ImagePngFailureKind::Truncated,
        });
    }
    if &bytes[..8] != PNG_SIGNATURE
        || &bytes[8..12] != 13_u32.to_be_bytes().as_slice()
        || &bytes[12..16] != b"IHDR"
    {
        return Err(ImageImportError::Png {
            stage: ImageImportStage::Header,
            kind: ImagePngFailureKind::InvalidData,
        });
    }
    let width =
        u32::from_be_bytes(
            bytes[16..20]
                .try_into()
                .map_err(|_| ImageImportError::Png {
                    stage: ImageImportStage::Header,
                    kind: ImagePngFailureKind::Truncated,
                })?,
        );
    let height =
        u32::from_be_bytes(
            bytes[20..24]
                .try_into()
                .map_err(|_| ImageImportError::Png {
                    stage: ImageImportStage::Header,
                    kind: ImagePngFailureKind::Truncated,
                })?,
        );
    Ok(PngHeaderPreflight {
        width,
        height,
        interlaced: bytes[28] == 1,
    })
}

fn reject_unbounded_metadata(bytes: &[u8]) -> Result<(), ImageImportError> {
    let mut offset = 8_usize;
    while let Some(header_end) = offset.checked_add(8) {
        let Some(header) = bytes.get(offset..header_end) else {
            break;
        };
        let length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let kind = &header[4..8];
        if kind == b"eXIf" {
            return Err(ImageImportError::Unsupported {
                stage: ImageImportStage::Metadata,
                feature: ImageUnsupportedFeature::EmbeddedMetadata,
            });
        }
        let Some(next) = header_end
            .checked_add(length)
            .and_then(|end| end.checked_add(4))
        else {
            break;
        };
        if next > bytes.len() || kind == b"IEND" {
            break;
        }
        offset = next;
    }
    Ok(())
}

fn allocate_zeroed(length: usize, stage: ImageImportStage) -> Result<Vec<u8>, ImageImportError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| ImageImportError::Png {
            stage,
            kind: ImagePngFailureKind::AllocationFailed,
        })?;
    bytes.resize(length, 0);
    Ok(bytes)
}

fn write_rgba8_row(
    color: png::ColorType,
    source: &[u8],
    destination: &mut [u8],
    width: u32,
) -> Result<(), ImageImportError> {
    let width = usize::try_from(width).map_err(|_| ImageImportError::Png {
        stage: ImageImportStage::Decode,
        kind: ImagePngFailureKind::DecoderContract,
    })?;
    let source_channels = match color {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(ImageImportError::Unsupported {
                stage: ImageImportStage::Decode,
                feature: ImageUnsupportedFeature::OutputColorModel,
            });
        }
    };
    let source_length = width
        .checked_mul(source_channels)
        .ok_or(ImageImportError::Png {
            stage: ImageImportStage::Decode,
            kind: ImagePngFailureKind::DecoderContract,
        })?;
    let destination_length = width.checked_mul(4).ok_or(ImageImportError::Png {
        stage: ImageImportStage::Decode,
        kind: ImagePngFailureKind::DecoderContract,
    })?;
    if source.len() < source_length || destination.len() != destination_length {
        return Err(ImageImportError::Png {
            stage: ImageImportStage::Decode,
            kind: ImagePngFailureKind::DecoderContract,
        });
    }

    match color {
        png::ColorType::Grayscale => {
            for (&value, output) in source[..source_length]
                .iter()
                .zip(destination.chunks_exact_mut(4))
            {
                output.copy_from_slice(&[value, value, value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for (input, output) in source[..source_length]
                .chunks_exact(2)
                .zip(destination.chunks_exact_mut(4))
            {
                output.copy_from_slice(&[input[0], input[0], input[0], input[1]]);
            }
        }
        png::ColorType::Rgb => {
            for (input, output) in source[..source_length]
                .chunks_exact(3)
                .zip(destination.chunks_exact_mut(4))
            {
                output.copy_from_slice(&[input[0], input[1], input[2], 255]);
            }
        }
        png::ColorType::Rgba => destination.copy_from_slice(&source[..source_length]),
        png::ColorType::Indexed => unreachable!("indexed output is rejected above"),
    }
    Ok(())
}

fn check_cancelled(
    cancellation: Option<&TaskCancellationToken>,
    stage: ImageImportStage,
) -> Result<(), ImageImportError> {
    if cancellation.is_some_and(TaskCancellationToken::is_cancelled) {
        Err(ImageImportError::Cancelled { stage })
    } else {
        Ok(())
    }
}

pub(super) fn check_encoded_limit(
    limits: ImageImportLimits,
    encoded_bytes: usize,
) -> Result<(), ImageImportError> {
    let limit = limits.max_encoded_bytes().get();
    if encoded_bytes <= limit {
        Ok(())
    } else {
        Err(ImageImportError::budget(
            ImageImportStage::Admission,
            ImageImportBudgetError::per_image(
                ImageImportLimitKind::EncodedBytes,
                Some(encoded_bytes as u64),
                limit as u64,
            ),
        ))
    }
}

fn plan_error_stage(error: ImageImportBudgetError) -> ImageImportStage {
    match error.kind() {
        ImageImportLimitKind::EncodedBytes => ImageImportStage::SourceRead,
        ImageImportLimitKind::Width
        | ImageImportLimitKind::Height
        | ImageImportLimitKind::Pixels
        | ImageImportLimitKind::RgbaBytes => ImageImportStage::Header,
        ImageImportLimitKind::DecoderWorkBytes | ImageImportLimitKind::AggregateInFlightBytes => {
            ImageImportStage::Metadata
        }
    }
}

fn map_png_error(stage: ImageImportStage, error: png::DecodingError) -> ImageImportError {
    match error {
        png::DecodingError::LimitsExceeded => ImageImportError::budget(
            stage,
            ImageImportBudgetError::per_image(
                ImageImportLimitKind::DecoderWorkBytes,
                None,
                PNG_TRACKED_DECODER_BYTES as u64,
            ),
        ),
        png::DecodingError::IoError(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            ImageImportError::Png {
                stage,
                kind: ImagePngFailureKind::Truncated,
            }
        }
        png::DecodingError::IoError(_) | png::DecodingError::Format(_) => ImageImportError::Png {
            stage,
            kind: ImagePngFailureKind::InvalidData,
        },
        png::DecodingError::Parameter(_) => ImageImportError::Png {
            stage,
            kind: ImagePngFailureKind::DecoderContract,
        },
    }
}
