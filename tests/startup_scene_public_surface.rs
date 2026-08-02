#![cfg(all(feature = "runtime-2d", feature = "serde"))]

#[test]
fn startup_scene_activation_is_advanced_only() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/startup_scene/advanced_surface.rs");
    tests.compile_fail("tests/ui/startup_scene/activation_pair_not_constructible.rs");
    tests.compile_fail("tests/ui/startup_scene/activation_view_not_resource.rs");
    tests.compile_fail("tests/ui/startup_scene/direct_module.rs");
    tests.compile_fail("tests/ui/startup_scene/ordinary_prelude.rs");
    tests.compile_fail("tests/ui/startup_scene/source_not_clone.rs");
}
