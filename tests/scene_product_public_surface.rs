#![cfg(feature = "runtime-core")]

#[test]
fn product_scene_replacement_is_advanced_only() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/scene_product/advanced_surface.rs");
    tests.compile_fail("tests/ui/scene_product/ordinary_prelude.rs");
    tests.compile_fail("tests/ui/scene_product/private_module.rs");
    tests.compile_fail("tests/ui/scene_product/raw_replace.rs");
    tests.compile_fail("tests/ui/scene_product/writer_not_constructible.rs");
}
