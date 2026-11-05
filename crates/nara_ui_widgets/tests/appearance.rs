use std::sync::Arc;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshotBuilder, UiBrush, UiButtonPhase, UiCompiler, UiCustomLayout,
    UiDimension, UiEnvironment, UiInputEvent, UiInteractionSession, UiKey, UiLayer, UiLocalKey,
    UiOverflow, UiPoint, UiPointerButton, UiPointerId, UiPointerInput, UiRect, UiRoot, UiRootKey,
    UiRouteStamp, UiSeatId, UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{
    UiAnchoredPopup, UiAppearance, UiButton, UiButtonState, UiButtonVariant, UiControlGrid,
    UiControlSize, UiDensity, UiIntentBatch, UiTextRole,
};

#[test]
fn density_and_user_text_scale_have_separate_geometry_dependencies() {
    let normal = UiAppearance::default();
    let compact = normal.with_density(UiDensity::Compact);
    let enlarged = compact.with_text_scale(2.0).unwrap();
    assert_eq!(
        normal.typography(UiTextRole::Body).unwrap(),
        compact.typography(UiTextRole::Body).unwrap()
    );
    assert!(normal.gap() > compact.gap());
    let regular = normal.control_metrics(UiControlSize::Regular).unwrap();
    let dense = compact.control_metrics(UiControlSize::Regular).unwrap();
    let large_text = enlarged.control_metrics(UiControlSize::Regular).unwrap();
    assert_eq!(regular.height(), 28.0);
    assert_eq!(dense.height(), 24.0);
    assert_eq!(large_text.height(), 44.0);
    assert_eq!(large_text.padding(), dense.padding());
    assert_eq!(large_text.typography().logical_size(), 28.0);
    for invalid in [f32::NAN, f32::INFINITY, 0.0, -1.0, 4.01] {
        assert!(normal.with_text_scale(invalid).is_err());
    }
    assert!(matches!(
        nara_ui_widgets::UiControlMetrics::new(
            nara_text::TextTypography::new(14.0).unwrap(),
            14.0,
            nara_ui_kernel::UiEdges::default(),
            4.0,
            1.0
        ),
        Err(nara_ui_widgets::UiAppearanceError::AutomaticControlMetrics)
    ));
}

#[test]
fn local_palette_resolution_neither_changes_siblings_nor_recreates_control_identity() {
    let outer = UiAppearance::default();
    let mut palette = outer.palette();
    palette.accent = nara_ui_kernel::UiColor::rgba(0.3, 0.02, 0.05, 1.0);
    let local = outer.with_palette(palette).unwrap();
    let root = UiRootKey::runtime("appearance", "scope").unwrap();
    let button = UiButton::new(
        UiKey::scoped(root, UiLocalKey::runtime("control", "save").unwrap()).unwrap(),
        "Save",
        UiActionId::new("save").unwrap(),
    )
    .unwrap();
    let resolve = |scope: UiAppearance| {
        scope
            .button(
                &button,
                UiButtonState::default(),
                UiButtonVariant::Primary,
                UiControlSize::Regular,
                false,
            )
            .unwrap()
    };
    let original = resolve(outer);
    let overridden = resolve(local);
    assert_ne!(original.visual(), overridden.visual());
    assert_eq!(original.metrics(), overridden.metrics());
    assert_eq!(resolve(outer), original);
    assert_eq!(button.label(), "Save");
    palette.text = nara_ui_kernel::UiColor::rgba(f32::NAN, 0.0, 0.0, 1.0);
    assert!(outer.with_palette(palette).is_err());
}

#[test]
fn default_opaque_text_roles_keep_distinct_readable_foreground_and_surface_pairs() {
    let colors = UiAppearance::default().palette();
    let luminance = |color: nara_ui_kernel::UiColor| {
        let [r, g, b, alpha] = color.components();
        assert_eq!(alpha, 1.0);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    for (foreground, background) in [
        (colors.text, colors.panel),
        (colors.muted_text, colors.panel),
        (colors.text, colors.input),
        (colors.danger_text, colors.panel),
        (colors.on_accent, colors.accent),
        (colors.on_accent, colors.danger),
    ] {
        let foreground = luminance(foreground);
        let background = luminance(background);
        let contrast = (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
        assert!(contrast >= 4.5, "opaque default role contrast {contrast}");
    }
    assert_ne!(colors.danger, colors.danger_text);
}

#[test]
fn focused_pressed_selected_control_keeps_its_focus_border_and_disabled_behavior() {
    let scope = UiAppearance::default();
    let root = UiRootKey::runtime("appearance", "state").unwrap();
    let button = UiButton::new(
        UiKey::scoped(
            root.clone(),
            UiLocalKey::runtime("control", "apply").unwrap(),
        )
        .unwrap(),
        "Apply",
        UiActionId::new("apply").unwrap(),
    )
    .unwrap();
    let initial = scope
        .button(
            &button,
            UiButtonState::default(),
            UiButtonVariant::Secondary,
            UiControlSize::Regular,
            true,
        )
        .unwrap();
    let node = initial
        .authored_node(&button, &root, UiStyle::absolute(10.0, 10.0, 100.0, 28.0))
        .unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(UiRoot::new(root, UiLayer::Base, UiStyle::fill()).with_child(node))
        .build()
        .unwrap();
    let surface = UiSurfaceId::new(17).unwrap();
    let environment = UiEnvironment::new(surface, 180.0, 100.0, 1.25, 1, 1, 1, 1).unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal.install_surface(surface, 1).unwrap();
    let receipt = authority
        .submit(&mut journal, Arc::clone(&frame), 1)
        .unwrap();
    let route = UiRouteStamp::from_receipt(receipt);
    let mut interaction = UiInteractionSession::default();
    let mut state = UiButtonState::default();
    let mut intents = UiIntentBatch::default();
    for pointer in [
        UiPointerInput::moved(
            UiSeatId::PRIMARY,
            UiPointerId::PRIMARY,
            UiPoint::new(20.0, 20.0),
        ),
        UiPointerInput::button(
            UiSeatId::PRIMARY,
            UiPointerId::PRIMARY,
            UiPointerButton::Primary,
            UiButtonPhase::Pressed,
            Some(UiPoint::new(20.0, 20.0)),
        ),
    ] {
        let outcome = interaction
            .dispatch(&journal, route, UiInputEvent::Pointer(pointer))
            .unwrap();
        state
            .reduce_batch(&button, outcome.consumed().unwrap(), &mut intents)
            .unwrap();
    }
    assert!(state.focused() && state.hovered() && state.pressed());
    let active = scope
        .button(
            &button,
            state,
            UiButtonVariant::Secondary,
            UiControlSize::Regular,
            true,
        )
        .unwrap();
    assert_eq!(
        active.visual().border().unwrap().brush(),
        &UiBrush::Solid(scope.palette().focus)
    );
    assert_ne!(active.visual().background(), initial.visual().background());
    assert_eq!(active.metrics(), initial.metrics());
    let unselected = scope
        .button(
            &button,
            state,
            UiButtonVariant::Secondary,
            UiControlSize::Regular,
            false,
        )
        .unwrap();
    assert_ne!(
        active.visual().background(),
        unselected.visual().background()
    );
    assert_eq!(active.visual().border(), unselected.visual().border());
    assert!(intents.intents().is_empty());
    let disabled = button.with_enabled(false);
    let disabled_style = scope
        .button(
            &disabled,
            state,
            UiButtonVariant::Secondary,
            UiControlSize::Regular,
            true,
        )
        .unwrap();
    assert_eq!(
        disabled_style.text_brush(),
        &UiBrush::Solid(scope.palette().disabled_text)
    );
    assert_ne!(
        disabled_style.visual().border().unwrap().brush(),
        active.visual().border().unwrap().brush()
    );
    let valid_field = scope.field(true, true, false).unwrap();
    let invalid_field = scope.field(true, true, true).unwrap();
    assert_eq!(invalid_field.visual.border(), valid_field.visual.border());
    assert_ne!(
        invalid_field.visual.background(),
        valid_field.visual.background()
    );
    assert_eq!(invalid_field.padding, valid_field.padding);
}

#[test]
fn measured_control_group_wraps_without_reauthoring_child_positions() {
    let root = UiRootKey::runtime("appearance", "grid").unwrap();
    let scope = UiAppearance::default();
    let children = ["Save", "Reopen", "Close", "Refresh"].map(|label| {
        let key =
            UiKey::scoped(root.clone(), UiLocalKey::runtime("control", label).unwrap()).unwrap();
        let button = UiButton::new(key, label, UiActionId::new("activate").unwrap()).unwrap();
        scope
            .button(
                &button,
                UiButtonState::default(),
                UiButtonVariant::Secondary,
                UiControlSize::Regular,
                false,
            )
            .unwrap()
            .authored_node(&button, &root, UiStyle::flex())
            .unwrap()
    });
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root, UiLayer::Base, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(UiControlGrid::new(90.0, 6.0).unwrap()))
                .with_children(children),
        )
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let compile = |compiler: &mut UiCompiler, width| {
        compiler
            .compile(
                &snapshot,
                UiEnvironment::new(UiSurfaceId::new(18).unwrap(), width, 200.0, 1.5, 1, 1, 1, 1)
                    .unwrap(),
                UiTheme::default(),
            )
            .unwrap()
    };
    let wide = compile(&mut compiler, 500.0);
    let narrow = compile(&mut compiler, 200.0);
    assert_eq!(wide.semantics().len(), 4);
    assert_eq!(narrow.semantics().len(), 4);
    assert!(
        wide.semantics()
            .iter()
            .all(|node| node.rect().origin().y() == 0.0)
    );
    assert_eq!(narrow.semantics()[2].rect().origin().y(), 34.0);
    for node in narrow.semantics() {
        assert!(node.rect().max().x() <= 200.0);
        assert_eq!(node.rect().size().height(), 28.0);
    }
}

#[test]
fn anchored_popup_flips_and_shifts_in_logical_space_at_fractional_native_scales() {
    let root = UiRootKey::runtime("appearance", "popup").unwrap();
    let local = UiLocalKey::runtime("popup", "body").unwrap();
    let key = UiKey::scoped(root.clone(), local.clone()).unwrap();
    let anchor = UiRect::from_origin_size(280.0, 160.0, 30.0, 24.0).unwrap();
    let child = nara_ui_kernel::UiNode::new(
        UiStyle::flex()
            .with_height(UiDimension::Px(60.0))
            .with_overflow(UiOverflow::Clip),
    )
    .with_key(local);
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root, UiLayer::Modal, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(
                    UiAnchoredPopup::new(anchor, 272.0, 8.0, 4.0).unwrap(),
                ))
                .with_child(child),
        )
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let frame = compiler
            .compile(
                &snapshot,
                UiEnvironment::new(
                    UiSurfaceId::new(19).unwrap(),
                    320.0,
                    200.0,
                    scale,
                    1,
                    1,
                    1,
                    1,
                )
                .unwrap(),
                UiTheme::default(),
            )
            .unwrap();
        assert!(frame.blocks_unhandled_input());
        let rect = frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&key))
            .unwrap()
            .rect();
        assert_eq!(
            rect,
            UiRect::from_origin_size(38.0, 96.0, 272.0, 60.0).unwrap()
        );
        let physical = frame
            .environment()
            .content_transform()
            .authored_to_physical(rect.origin())
            .unwrap();
        assert_eq!(physical, UiPoint::new(38.0 * scale, 96.0 * scale));
    }
}

#[test]
fn oversized_popup_keeps_its_viewport_inside_available_bounds() {
    let root = UiRootKey::runtime("appearance", "small-popup").unwrap();
    let local = UiLocalKey::runtime("popup", "body").unwrap();
    let key = UiKey::scoped(root.clone(), local.clone()).unwrap();
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(root, UiLayer::Modal, UiStyle::fill())
                .with_custom_layout(UiCustomLayout::new(
                    UiAnchoredPopup::new(
                        UiRect::from_origin_size(-20.0, -10.0, 12.0, 12.0).unwrap(),
                        272.0,
                        8.0,
                        4.0,
                    )
                    .unwrap(),
                ))
                .with_child(
                    nara_ui_kernel::UiNode::new(
                        UiStyle::flex()
                            .with_height(UiDimension::Px(300.0))
                            .with_overflow(UiOverflow::Clip),
                    )
                    .with_key(local),
                ),
        )
        .build()
        .unwrap();
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(
            &snapshot,
            UiEnvironment::new(UiSurfaceId::new(20).unwrap(), 180.0, 96.0, 1.25, 1, 1, 1, 1)
                .unwrap(),
            UiTheme::default(),
        )
        .unwrap();
    assert_eq!(
        frame
            .layouts()
            .iter()
            .find(|layout| layout.key() == Some(&key))
            .unwrap()
            .rect(),
        UiRect::from_origin_size(8.0, 8.0, 164.0, 80.0).unwrap()
    );
    for value in [0.0, -1.0, f32::INFINITY, f32::NAN] {
        assert!(UiAnchoredPopup::new(UiRect::default(), value, 8.0, 4.0).is_err());
    }
}
