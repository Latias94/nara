use nara_ui_module_consumer::prepare_status_label;

#[test]
fn direct_consumer_shapes_without_ecs_scene_or_render_dependencies() {
    let (layout, rasters) = prepare_status_label("HP 20 / 20").unwrap();
    assert!(layout.width() > 0.0);
    assert_eq!(rasters.len(), layout.glyphs().len());
    assert!(rasters.iter().any(|raster| !raster.mask().is_empty()));
}
