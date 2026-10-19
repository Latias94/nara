use nara::advanced_prelude::{
    SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
    replace_scene_with_product, replace_expanded_scene_with_product,
};
use nara::ecs::Resource;

#[derive(Resource)]
struct ProductState;

impl SceneProductResource for ProductState {}

fn inspect(_writer: &mut SceneProductOverlayWriter<'_>) {}

fn main() {
    let _ = inspect;
    let _ = SceneProductTransactionLimits::new;
    let _ = replace_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
    let _ = replace_expanded_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
}
