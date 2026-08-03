use nara::prelude::{SceneProductOverlayWriter, replace_scene_with_product};

fn main() {
    let _ = replace_scene_with_product::<fn(&mut SceneProductOverlayWriter<'_>)>;
}
