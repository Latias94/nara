//! Source PNG and cooked Runtime content captured through ordinary headless product runs.

#[path = "content/capture.rs"]
mod capture;
#[path = "content/fixture.rs"]
mod fixture;

use anyhow::{Result, ensure};
use nara_render_wgpu::WgpuRenderInput;

pub const WIDTH: u32 = 12;
pub const HEIGHT: u32 = 4;
pub const RGB: [u8; 3] = [128, 64, 192];
pub const ALPHAS: [u8; 4] = [0, 64, 128, 255];
pub const TARGET_WIDTH: u32 = 96;
pub const TARGET_HEIGHT: u32 = 32;

/// Original sealed inputs whose pixel reservations outlive both retired Runtime owners.
pub(super) struct ContentCaptures {
    pub source: WgpuRenderInput,
    pub processed: WgpuRenderInput,
}

/// Captures the same stock Sprite from real source and source-independent Runtime content.
pub(super) fn capture() -> Result<ContentCaptures> {
    let fixture = fixture::ContentFixture::new()?;
    let source = capture::source(fixture.source_root(), fixture.recipe().clone())?;
    let base = fixture.open_processed()?;
    fixture.destroy_backing()?;
    let processed = capture::processed(base)?;

    source.assert_retired()?;
    processed.assert_retired()?;
    ensure!(
        source.runtime != processed.runtime,
        "Runtime identities must differ"
    );
    ensure!(
        source.input.packet().source_generation() != processed.input.packet().source_generation(),
        "each Runtime must issue its own render source identity"
    );
    ensure!(
        source.source == processed.source,
        "cooking must preserve the logical Scene source"
    );
    ensure!(
        source.provenance == processed.provenance,
        "cooking must preserve the actual Scene entity provenance"
    );
    let source_image = &source.image;
    let processed_image = &processed.image;
    ensure!(
        source_image.identity().key() != processed_image.identity().key(),
        "independent Runtime stores must retain distinct Image identities"
    );
    ensure!(
        source_image.resource().pixels() == processed_image.resource().pixels(),
        "Source and Processed must prepare the same actual decoded pixels"
    );
    ensure!(
        source_image.resource().content_hash() == processed_image.resource().content_hash(),
        "Source and Processed Image content hashes must agree"
    );
    let inputs = ContentCaptures {
        source: source.input,
        processed: processed.input,
    };
    drop((source.image, processed.image));
    Ok(inputs)
}

fn validate_pixels(pixels: &[u8]) -> Result<()> {
    ensure!(
        pixels.len() == (WIDTH * HEIGHT * 4) as usize,
        "the captured Image must contain exactly {WIDTH}x{HEIGHT} RGBA texels"
    );
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let x = index % WIDTH as usize;
        let expected = [RGB[0], RGB[1], RGB[2], ALPHAS[x / 3]];
        ensure!(
            pixel == expected,
            "incorrect source texel at ({x}, {}): {pixel:?}, expected {expected:?}",
            index / WIDTH as usize
        );
    }
    Ok(())
}
