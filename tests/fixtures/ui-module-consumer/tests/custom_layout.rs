use nara_text::{TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiCustomLayout, UiEnvironment, UiLayer, UiLocalKey,
    UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme,
};
use nara_ui_module_consumer::{
    keyed_editors::{EditorRow, KeyedEditors},
    measured_column::MeasuredColumn,
};

#[test]
fn independently_authored_container_measures_current_children_and_changes_geometry_with_typography()
{
    let face = bundled_font_face().unwrap();
    let root = UiRootKey::runtime("independent-layout", "panel").unwrap();
    let mut compiler = UiCompiler::new(face, Default::default()).unwrap();
    let compile = |compiler: &mut UiCompiler, size: f32, revision: u64| {
        let snapshot = UiAuthoringSnapshotBuilder::new(revision)
            .with_root(
                UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill())
                    .with_custom_layout(UiCustomLayout::new(MeasuredColumn { gap: 7.0 }))
                    .with_child(
                        UiNode::new(UiStyle::flex())
                            .with_key(UiLocalKey::runtime("text", "first").unwrap())
                            .with_text(UiText::new("Measured title", size).unwrap()),
                    )
                    .with_child(
                        UiNode::new(UiStyle::flex())
                            .with_key(UiLocalKey::runtime("text", "second").unwrap())
                            .with_text(UiText::new("Second row", 16.0).unwrap()),
                    ),
            )
            .build()
            .unwrap();
        compiler
            .compile(
                &snapshot,
                UiEnvironment::new(
                    UiSurfaceId::new(92).unwrap(),
                    280.0,
                    220.0,
                    1.25,
                    revision,
                    revision,
                    1,
                    1,
                )
                .unwrap(),
                UiTheme::default(),
            )
            .unwrap()
    };
    let small = compile(&mut compiler, 12.0, 1);
    let large = compile(&mut compiler, 28.0, 2);
    assert!(small.diagnostics().is_empty() && large.diagnostics().is_empty());
    let first = |frame: &nara_ui_kernel::UiFrame| {
        frame
            .semantics()
            .iter()
            .find(|node| node.name() == "Measured title")
            .unwrap()
            .rect()
    };
    let second = |frame: &nara_ui_kernel::UiFrame| {
        frame
            .semantics()
            .iter()
            .find(|node| node.name() == "Second row")
            .unwrap()
            .rect()
    };
    assert!(first(&large).size().height() > first(&small).size().height());
    assert_eq!(second(&small).origin().y(), first(&small).max().y() + 7.0);
    assert_eq!(second(&large).origin().y(), first(&large).max().y() + 7.0);
    let stats = compiler.work_stats();
    assert!(stats.custom_layout_callbacks >= 2 && stats.custom_child_measurements >= 4);
    assert!(stats.layout_work_units > stats.custom_child_measurements);
}

#[test]
fn keyed_fields_compose_in_user_measured_layout_without_recreating_their_state() {
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let mut component =
        KeyedEditors::new(UiRootKey::runtime("independent-layout", "keyed").unwrap()).unwrap();
    let row = |id| EditorRow {
        id,
        document: "document",
        schema: 1,
        initial: "Draft",
        replace: false,
    };
    let (root, original) = component
        .compose(&[row("first"), row("second")], &mut engine)
        .unwrap();
    let initial = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    let (root, repeated) = component
        .compose(&[row("first"), row("second")], &mut engine)
        .unwrap();
    let rebuilt = UiAuthoringSnapshotBuilder::new(1)
        .with_root(root)
        .build()
        .unwrap();
    assert_eq!(
        initial, rebuilt,
        "compatible composition shares its immutable layout description"
    );
    assert_eq!(original, repeated);
    assert_eq!(component.state.last_changes().reused, 2);
    assert_eq!(component.state.last_changes().initialized, 0);
}
