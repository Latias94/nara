use nara::prelude::{SceneProductOverlayWriter, replace_expanded_scene_with_product};

fn main() {
    let _ = replace_expanded_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
}
