use nara::prelude::{Camera2d, RenderTarget, UiNode, UiStyle};
use nara::sprite_render::SpriteBatchKind;

fn main() {
    let _camera = Camera2d::default();
    let _target = RenderTarget::PrimaryWindow;
    let _node = UiNode::new(UiStyle::absolute(0.0, 0.0, 320.0, 180.0));
    let _closed_sprite_kinds = [SpriteBatchKind::Tilemap, SpriteBatchKind::Sprite];
}
