#![cfg(feature = "runtime-core")]

use nara::advanced_prelude::{
    SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
    replace_expanded_scene_with_product, replace_expanded_scene_with_product_and_bindings,
};
use nara::ecs::Resource;
use std::sync::Arc;

#[derive(Resource, Default)]
struct ProductState(Arc<()>);

impl SceneProductResource for ProductState {
    type Baseline = Arc<()>;

    fn capture_baseline(&self) -> Self::Baseline {
        Arc::clone(&self.0)
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        Arc::ptr_eq(&self.0, baseline)
    }
}

fn inspect(_writer: &mut SceneProductOverlayWriter<'_>) {}

#[test]
fn advanced_scene_product_surface_is_available() {
    let _ = ProductState::default();
    let _ = inspect;
    let _ = SceneProductTransactionLimits::new;
    let _ = replace_expanded_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
    let _ =
        replace_expanded_scene_with_product_and_bindings::<fn(&mut SceneProductOverlayWriter<'_>)>;
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
