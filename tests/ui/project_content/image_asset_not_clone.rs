use nara::image::ImageAsset;

fn require_clone<T: Clone>() {}

fn main() {
    require_clone::<ImageAsset>();
}
