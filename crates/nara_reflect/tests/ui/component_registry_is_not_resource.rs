use nara_ecs::Resource;
use nara_reflect::ComponentRegistry;

fn assert_resource<T: Resource>() {}

fn main() {
    assert_resource::<ComponentRegistry>();
}
