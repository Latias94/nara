#![forbid(unsafe_code)]

#[path = "support/scene.rs"]
#[allow(dead_code)]
mod scene;

use nara_stock_composition_consumer::{CompositionPolicy, RetainedInputs};

#[test]
fn ordinary_scene_materialization_reaches_the_public_stock_collector() {
    for canvas in [false, true] {
        let input = scene::capture(canvas);
        let capture = input.captured::<CompositionPolicy>().unwrap();
        assert!(capture.stock.ui_packets().is_empty());
        assert_eq!(input.packet().target().extent().width, scene::WIDTH);
        assert_eq!(input.packet().target().extent().height, scene::HEIGHT);
        assert_eq!(
            input
                .packet()
                .world_canvas()
                .map(|frame| frame.extent().width),
            canvas.then_some(scene::WORLD_WIDTH)
        );
    }
}

#[test]
fn an_unconsumed_cpu_capture_survives_until_its_original_runtime_closes() {
    let mut runtime = scene::capture_runtime(false).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let original = runtime.world().resource::<RetainedInputs>().0[0].clone();
    let stock = std::sync::Arc::clone(&original.captured::<CompositionPolicy>().unwrap().stock);
    runtime
        .finish()
        .unwrap_or_else(|retirement| panic!("{retirement:?}"));
    assert!(std::sync::Arc::ptr_eq(
        &stock,
        &original.captured::<CompositionPolicy>().unwrap().stock
    ));
    assert_eq!(original.packet().views().len(), 1);
}
