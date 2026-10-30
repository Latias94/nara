use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiImageId, UiLayer,
    UiPaintPrimitive, UiResolvedBrush, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
};

#[test]
fn independent_image_providers_compile_without_asset_or_renderer_dependencies() {
    struct FirstProvider;
    struct SecondProvider;

    let images = [
        UiImageId::new("product:manual/primary").unwrap(),
        UiImageId::external::<FirstProvider>(17),
        UiImageId::external::<SecondProvider>(17),
    ];
    let mut builder = UiAuthoringSnapshotBuilder::new(1);
    for (index, image) in images.iter().enumerate() {
        builder = builder.with_root(
            UiRoot::new(
                UiRootKey::runtime("independent.images", index.to_string()).unwrap(),
                UiLayer::Base,
                UiStyle::absolute(index as f32 * 40.0, 0.0, 32.0, 32.0),
            )
            .with_visual(UiVisual::image(
                image.clone(),
                UiColor::rgba(1.0, 1.0, 1.0, 1.0),
            )),
        );
    }
    let snapshot = builder.build().unwrap();
    let mut compiler =
        UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(3).unwrap(), 160.0, 64.0, 1.0, 1, 1, 1, 1).unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    drop(snapshot);
    drop(compiler);

    let painted = frame
        .paint()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                brush: Some(UiResolvedBrush::Image { image, .. }),
                ..
            } => Some(image.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(painted, images);
    assert_eq!(images[0].as_name(), Some("product:manual/primary"));
    assert_ne!(images[1], images[2]);
    assert!(images[1].as_name().is_none());
    assert!(frame.diagnostics().is_empty());
}
