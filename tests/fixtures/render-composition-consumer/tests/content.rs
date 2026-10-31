#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

//! Real source imports and processed content admission through public owners.
//!
//! This ordinary test covers content only. Stock capture interleaving requires an original
//! presentation driver; HeadlessRun cannot complete the native transport selected by this
//! fixture's Shipping profile. No native rendering or device-loss coverage is claimed here.

#[path = "support/content_fixture.rs"]
mod content_fixture;

use nara::image::{ImageDomain, ImageRuntimeCodec};

use content_fixture::{ContentFixture, GREEN, RED, image_product, scene_product};

#[test]
fn actual_source_imports_encode_admitted_processed_images_without_runtime_presentation_claims() {
    let fixture = ContentFixture::new();
    // Fixture production imports both PNGs through ProjectContentLoader before encoding these
    // exact Runtime Image members. This ordinary test proves content admission, not rendering.
    let base = fixture.open_processed();
    base.validate_deep().unwrap();
    let red = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(false))
        .unwrap();
    let green = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(true))
        .unwrap();
    assert_eq!(red.product().product(), Some(image_product(false)));
    assert_eq!(green.product().product(), Some(image_product(true)));
    assert!(red.product().source().is_none() && green.product().source().is_none());
    assert_eq!(red.product().pixels(), RED);
    assert_eq!(green.product().pixels(), GREEN);
    assert_eq!(
        base.acquire_startup_scene().unwrap().product(),
        scene_product(false)
    );
    assert_eq!(
        base.acquire_scene(scene_product(true)).unwrap().product(),
        scene_product(true)
    );
    fixture.destroy_backing();
    drop(fixture);
    assert_eq!(red.product().pixels(), RED);
    assert_eq!(green.product().pixels(), GREEN);
    assert_eq!(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(false))
            .unwrap()
            .product()
            .pixels(),
        RED
    );
    assert_eq!(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(true))
            .unwrap()
            .product()
            .pixels(),
        GREEN
    );
}
