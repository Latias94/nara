#![cfg(feature = "runtime-core")]

use nara::advanced_prelude::{
    SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
    replace_expanded_scene_with_product, replace_scene_with_product,
};
use nara::ecs::Resource;

#[derive(Resource)]
struct ProductState;

impl SceneProductResource for ProductState {}

fn inspect(_writer: &mut SceneProductOverlayWriter<'_>) {}

#[test]
fn advanced_scene_product_surface_is_available() {
    let _ = ProductState;
    let _ = inspect;
    let _ = SceneProductTransactionLimits::new;
    let _ = replace_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
    let _ = replace_expanded_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
}

#[test]
fn product_scene_replacement_is_advanced_only() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/scene_product/ordinary_prelude.rs");
    tests.compile_fail("tests/ui/scene_product/private_module.rs");
    tests.compile_fail("tests/ui/scene_product/raw_replace.rs");
    tests.compile_fail("tests/ui/scene_product/writer_not_constructible.rs");
    tests.compile_fail("tests/ui/scene_product/candidate_not_constructible.rs");
}
