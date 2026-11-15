//! Shared inspection of the original stock input in native and offscreen consumers.

use nara::render_wgpu::{WgpuRenderInput, WgpuStockPolicy};

use super::{Resident, indices_product};

pub(super) struct MaterialPixels {
    pub(super) indices: [u8; 4],
    pub(super) palette: [u8; 4],
}

pub(super) fn material(input: &WgpuRenderInput, current: &Resident) -> MaterialPixels {
    let stock = input.captured::<WgpuStockPolicy>().unwrap();
    let batches = stock.sprite_batches().unwrap().as_slice();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].instances.len(), 1);
    let material = batches[0].material_extension.as_ref().unwrap();
    assert_eq!(
        material.binding.generation().shader().digest(),
        current.material.shader,
    );
    assert_eq!(material.binding.uniform_bytes(), current.uniform);
    assert_eq!(
        material.binding.textures()["palette"],
        current.bound_texture
    );
    assert_eq!(material.textures.len(), 1);
    let palette = &current.images[&current.material.texture].pixels;
    assert_eq!(material.textures[0].1.resource().pixels(), palette);
    let indices = &current.images[&indices_product()].pixels;
    assert_eq!(
        batches[0].image.as_ref().unwrap().resource().pixels(),
        indices,
    );
    MaterialPixels {
        indices: indices.as_slice().try_into().unwrap(),
        palette: palette.as_slice().try_into().unwrap(),
    }
}
