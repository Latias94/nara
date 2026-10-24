use nara_ui_kernel::UiActionId;
use nara_ui_module_consumer::{compile_retry_button, compile_status_panel, prepare_status_label};

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
fn direct_consumer_compiles_a_controlled_button_without_private_presentation_authority() {
    let (button, frame) = compile_retry_button().unwrap();
    assert_eq!(button.action(), &UiActionId::new("retry").unwrap());
    assert!(
        frame
            .focus_order()
            .iter()
            .any(|fact| fact.key() == button.key())
    );
    let semantics = frame
        .semantics()
        .iter()
        .find(|node| node.key() == button.key())
        .unwrap();
    assert_eq!(semantics.actions(), &[UiActionId::new("retry").unwrap()]);
}

#[test]
fn direct_consumer_owns_complete_binary_persistent_context_without_scene_types() {
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiKey, UiLayer, UiLocalKey,
        UiOpaqueId, UiPersistentIdentity, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme,
    };
    use nara_ui_widgets::UiButton;

    let mut builder = UiAuthoringSnapshotBuilder::new(1);
    let mut buttons = Vec::new();
    for slot in [1, 2] {
        // This consumer owns its source/slot vocabulary; it does not encode Nara Scene bytes.
        let context = UiOpaqueId::new([0, 7, slot]).unwrap();
        let root = UiRootKey::persistent(UiPersistentIdentity::new(
            context.clone(),
            UiOpaqueId::new("root").unwrap(),
        ))
        .unwrap();
        let local = UiLocalKey::persistent(UiPersistentIdentity::new(
            context,
            UiOpaqueId::new("retry").unwrap(),
        ))
        .unwrap();
        let key = UiKey::scoped(root.clone(), local).unwrap();
        let button = UiButton::new(key, "Retry", UiActionId::new("retry").unwrap()).unwrap();
        builder = builder.with_root(
            UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill()).with_child(
                button
                    .authored_node(
                        &root,
                        UiStyle::absolute(f32::from(slot) * 100.0, 10.0, 80.0, 40.0),
                        nara_ui_kernel::UiVisual::role(nara_ui_kernel::UiColorRole::Accent),
                        nara_ui_kernel::UiBrush::Role(nara_ui_kernel::UiColorRole::PrimaryText),
                        16.0,
                    )
                    .unwrap(),
            ),
        );
        buttons.push(button);
    }
    let snapshot = builder.build().unwrap();
    let mut compiler =
        UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default()).unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(9).unwrap(), 320.0, 180.0, 1.0, 1, 1, 1, 1).unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert_eq!(frame.root_outcomes().len(), 2);
    assert_ne!(buttons[0].key(), buttons[1].key());
    for button in buttons {
        assert!(
            frame
                .focus_order()
                .iter()
                .any(|fact| fact.key() == button.key())
        );
        let semantic = frame
            .semantics()
            .iter()
            .find(|node| node.key() == button.key())
            .unwrap();
        assert_eq!(semantic.actions(), &[UiActionId::new("retry").unwrap()]);
        let recomposed = UiKey::scoped(
            button.key().root_key().clone(),
            button.key().local_key().unwrap().clone(),
        )
        .unwrap();
        assert_eq!(recomposed, *button.key());
    }
}
