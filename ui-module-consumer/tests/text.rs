use nara_ui_kernel::UiActivationSource;
use nara_ui_module_consumer::{activate_retry_button, compile_status_panel, prepare_status_label};
use nara_ui_widgets::UiIntent;

#[test]
fn direct_consumer_shapes_without_ecs_scene_or_render_dependencies() {
    let (layout, rasters) = prepare_status_label("HP 20 / 20").unwrap();
    assert!(layout.width() > 0.0);
    assert_eq!(rasters.len(), layout.glyphs().len());
    assert!(rasters.iter().any(|raster| !raster.mask().is_empty()));
}

#[test]
fn direct_consumer_compiles_an_immutable_ui_frame() {
    let frame = compile_status_panel("Wave 2 / 8").unwrap();
    assert_eq!(frame.root_outcomes().len(), 1);
    assert_eq!(frame.layouts().len(), 2);
    assert_eq!(frame.paint().len(), 2);
    assert!(frame.diagnostics().is_empty());
}

#[test]
fn direct_consumer_reduces_a_presented_button_to_a_typed_intent() {
    let intent = activate_retry_button().unwrap();
    assert!(matches!(
        intent,
        UiIntent::Activate {
            source: UiActivationSource::Keyboard,
            ..
        }
    ));
}
