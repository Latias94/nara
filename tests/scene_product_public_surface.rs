#![cfg(feature = "runtime-core")]

use nara::advanced_prelude::{
    SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
    replace_expanded_scene_with_product, replace_expanded_scene_with_product_and_reservations,
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
    let _ = replace_expanded_scene_with_product_and_reservations::<
        fn(&mut SceneProductOverlayWriter<'_>),
    >;
}
