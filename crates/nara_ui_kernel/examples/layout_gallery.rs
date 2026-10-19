use std::{error::Error, sync::Arc};

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiColorRole, UiCompiler, UiDimension, UiEdges,
    UiEnvironment, UiFlexDirection, UiInteraction, UiLayer, UiLocalKey, UiNode, UiOverflow,
    UiPaintPrimitive, UiRoot, UiRootKey, UiRootStatus, UiSemanticRole, UiSemantics, UiStyle,
    UiSurfaceId, UiText, UiTheme, UiVisual,
};

const DOMAIN: &str = "nara.example.layout-gallery";

fn main() -> Result<(), Box<dyn Error>> {
    let snapshot = gallery_snapshot()?;
    let mut compiler = UiCompiler::new(bundled_font_face()?, Default::default())?;

    let unit_scale = compiler.compile(&snapshot, environment(1.0)?, UiTheme::default())?;
    let frozen_layouts = unit_scale.layouts().to_vec();
    let fractional_scale = compiler.compile(&snapshot, environment(1.25)?, UiTheme::default())?;
    let degraded = degraded_gallery_snapshot(&snapshot)?;
    let retained = compiler.compile(&degraded, environment(1.25)?, UiTheme::default())?;

    assert_gallery_contract(&unit_scale, &fractional_scale);
    assert_root_envelope_contract(&retained);
    assert_eq!(unit_scale.layouts(), frozen_layouts);

    println!(
        "layout gallery: roots={}, layouts={}, paint={}, semantics={}, scales=[1.0, 1.25]",
        unit_scale.root_outcomes().len(),
        unit_scale.layouts().len(),
        unit_scale.paint().len(),
        unit_scale.semantics().len(),
    );
    Ok(())
}

fn degraded_gallery_snapshot(
    baseline: &UiAuthoringSnapshot,
) -> Result<UiAuthoringSnapshot, Box<dyn Error>> {
    let base_key = root_key("base")?;
    let overlay_key = root_key("overlay")?;
    let base = baseline
        .roots()
        .iter()
        .find(|root| root.key() == &base_key)
        .expect("baseline gallery contains its base root")
        .clone();
    let overlay = baseline
        .roots()
        .iter()
        .find(|root| root.key() == &overlay_key)
        .expect("baseline gallery contains its overlay root")
        .clone();
    let duplicate_key = local_key("invalid-modal-node")?;
    let duplicate = || UiNode::new(UiStyle::fill()).with_key(duplicate_key.clone());
    let invalid_modal = UiRoot::new(root_key("modal")?, UiLayer::Modal, UiStyle::fill())
        .with_children([duplicate(), duplicate()]);

    Ok(UiAuthoringSnapshotBuilder::new(2)
        .with_root(invalid_modal)
        .with_root(base)
        .with_root(overlay)
        .build()?)
}

fn gallery_snapshot() -> Result<UiAuthoringSnapshot, Box<dyn Error>> {
    let title = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::percent(1.0)?)
            .with_height(UiDimension::px(32.0)?),
    )
    .with_key(local_key("title")?)
    .with_visual(UiVisual::role(UiColorRole::Panel))
    .with_text(UiText::new("Renderer-neutral layout gallery", 18.0)?)
    .with_semantics(UiSemantics::new(
        UiSemanticRole::Status,
        "Layout gallery title",
    )?);

    let sidebar = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::percent(0.30)?)
            .with_height(UiDimension::percent(1.0)?)
            .with_padding(UiEdges::all(8.0)?),
    )
    .with_key(local_key("sidebar")?)
    .with_visual(UiVisual::role(UiColorRole::Panel))
    .with_text(UiText::new("30% nested panel", 14.0)?);

    let clipped_label = UiNode::new(UiStyle::absolute(72.0, 22.0, 150.0, 28.0))
        .with_key(local_key("clipped-label")?)
        .with_text(UiText::new(
            "This text is deliberately clipped by its parent.",
            14.0,
        )?);
    let absolute_badge = UiNode::new(
        UiStyle::absolute(0.0, 8.0, 76.0, 24.0)
            .with_left(UiDimension::percent(0.62)?)
            .with_z_index(2),
    )
    .with_key(local_key("absolute-badge")?)
    .with_visual(UiVisual::role(UiColorRole::Accent))
    .with_text(UiText::new("absolute", 13.0)?);
    let padded_label = UiNode::new(
        UiStyle::absolute(12.0, 56.0, 100.0, 1.0)
            .with_height(UiDimension::auto())
            .with_padding(UiEdges::symmetric(20.0, 5.0)?),
    )
    .with_key(local_key("padded-label")?)
    .with_visual(UiVisual::role(UiColorRole::Panel))
    .with_text(UiText::new("One two three four", 14.0)?);
    let content = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::percent(0.70)?)
            .with_height(UiDimension::percent(1.0)?)
            .with_overflow(UiOverflow::Clip)
            .with_stacking_context(true),
    )
    .with_key(local_key("content")?)
    .with_visual(UiVisual::role(UiColorRole::Surface))
    .with_children([clipped_label, absolute_badge, padded_label]);

    let nested_row = UiNode::new(
        UiStyle::flex()
            .with_width(UiDimension::percent(1.0)?)
            .with_flex_grow(1.0)
            .with_flex_direction(UiFlexDirection::Row)
            .with_gap(10.0),
    )
    .with_key(local_key("nested-row")?)
    .with_children([sidebar, content]);

    let base = UiRoot::new(
        root_key("base")?,
        UiLayer::Base,
        UiStyle::fill()
            .with_flex_direction(UiFlexDirection::Column)
            .with_padding(UiEdges::all(12.0)?)
            .with_gap(8.0),
    )
    .with_visual(UiVisual::role(UiColorRole::Surface))
    .with_children([title, nested_row]);

    let overlay = UiRoot::new(
        root_key("overlay")?,
        UiLayer::Overlay,
        UiStyle::absolute(232.0, 12.0, 76.0, 28.0),
    )
    .with_visual(UiVisual::role(UiColorRole::Warning))
    .with_text(UiText::new("overlay", 13.0)?);

    let modal_action = UiNode::new(
        UiStyle::absolute(16.0, 14.0, 96.0, 32.0).with_padding(UiEdges::symmetric(6.0, 4.0)?),
    )
    .with_key(local_key("modal-action")?)
    .with_visual(UiVisual::role(UiColorRole::Accent))
    .with_text(UiText::new("Activate", 15.0)?)
    .with_interaction(UiInteraction::focus_target())
    .with_semantics(UiSemantics::new(
        UiSemanticRole::Button,
        "Activate gallery action",
    )?);
    let modal = UiRoot::new(
        root_key("modal")?,
        UiLayer::Modal,
        UiStyle::absolute(96.0, 86.0, 128.0, 60.0),
    )
    .with_visual(UiVisual::role(UiColorRole::Panel))
    .with_child(modal_action);

    Ok(UiAuthoringSnapshotBuilder::new(1)
        .with_root(base)
        .with_root(overlay)
        .with_root(modal)
        .build()?)
}

fn environment(scale_factor: f32) -> Result<UiEnvironment, Box<dyn Error>> {
    Ok(UiEnvironment::new(
        UiSurfaceId::new(1).expect("gallery surface is non-zero"),
        320.0,
        180.0,
        scale_factor,
        1,
        1,
        1,
        1,
    )?)
}

fn assert_gallery_contract(
    unit_scale: &Arc<nara_ui_kernel::UiFrame>,
    fractional_scale: &Arc<nara_ui_kernel::UiFrame>,
) {
    assert_eq!(unit_scale.root_outcomes().len(), 3);
    assert!(unit_scale.diagnostics().is_empty());
    assert!(fractional_scale.diagnostics().is_empty());
    assert_ne!(unit_scale.revision(), fractional_scale.revision());

    let unit_content = layout(unit_scale, "content");
    let scaled_content = layout(fractional_scale, "content");
    assert_eq!(unit_content.rect(), scaled_content.rect());
    assert_ne!(unit_content.physical_rect(), scaled_content.physical_rect());
    assert!(unit_content.clip_rect().is_none());

    let clipped = layout(unit_scale, "clipped-label");
    assert!(clipped.clip_rect().is_some());
    assert!(
        unit_scale
            .paint()
            .iter()
            .any(|primitive| primitive.key() == clipped.key() && primitive.clip_rect().is_some())
    );

    let sidebar = layout(unit_scale, "sidebar");
    assert!(sidebar.rect().size().width() < unit_content.rect().size().width());
    for frame in [unit_scale, fractional_scale] {
        let padded = layout(frame, "padded-label");
        let (content, text_layout) = frame
            .paint()
            .iter()
            .find_map(|primitive| match primitive {
                UiPaintPrimitive::Text {
                    key, rect, layout, ..
                } if key.as_ref() == padded.key() => Some((*rect, layout)),
                _ => None,
            })
            .expect("the padded gallery label must publish text");
        assert_eq!(padded.rect().size().width(), 100.0);
        assert_eq!(content.size().width(), 60.0);
        assert_eq!(content.origin().x(), padded.rect().origin().x() + 20.0);
        assert_eq!(content.origin().y(), padded.rect().origin().y() + 5.0);
        assert_eq!(padded.rect().size().height(), text_layout.height() + 10.0);
        assert!(text_layout.lines().len() > 1);
        assert!(frame.paint().iter().any(|primitive| matches!(
            primitive,
            UiPaintPrimitive::Rect { key, rect, .. }
                if key.as_ref() == padded.key() && *rect == padded.rect()
        )));
        let action = layout(frame, "modal-action");
        assert!(
            frame
                .hit_regions()
                .iter()
                .any(|hit| { Some(hit.key()) == action.key() && hit.rect() == action.rect() })
        );
    }
    assert!(
        layout(unit_scale, "absolute-badge").structural_index()
            > layout(unit_scale, "clipped-label").structural_index()
    );

    let paint_roots = unit_scale
        .paint()
        .iter()
        .filter_map(UiPaintPrimitive::key)
        .map(|key| key.root_key())
        .collect::<Vec<_>>();
    let base = root_key("base").expect("static gallery root key is valid");
    let overlay = root_key("overlay").expect("static gallery root key is valid");
    let modal = root_key("modal").expect("static gallery root key is valid");
    let base_index = paint_roots.iter().position(|key| *key == &base).unwrap();
    let overlay_index = paint_roots.iter().position(|key| *key == &overlay).unwrap();
    let modal_index = paint_roots.iter().position(|key| *key == &modal).unwrap();
    assert!(base_index < overlay_index && overlay_index < modal_index);
    assert!(
        unit_scale
            .paint()
            .iter()
            .any(|primitive| matches!(primitive, UiPaintPrimitive::Text { .. }))
    );
    assert_eq!(unit_scale.focus_order().len(), 1);
    assert!(
        unit_scale
            .semantics()
            .iter()
            .any(|node| node.name() == "Layout gallery title")
    );
    assert!(
        unit_scale
            .semantics()
            .iter()
            .any(|node| node.name() == "Activate gallery action")
    );
}

fn assert_root_envelope_contract(frame: &Arc<nara_ui_kernel::UiFrame>) {
    let modal = root_key("modal").expect("static gallery root key is valid");
    let base = root_key("base").expect("static gallery root key is valid");
    let overlay = root_key("overlay").expect("static gallery root key is valid");
    assert_eq!(frame.root_outcomes().len(), 3);
    assert_eq!(frame.root_outcomes()[0].key(), &modal);
    assert_eq!(
        frame.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert!(
        frame
            .root_outcomes()
            .iter()
            .filter(|outcome| outcome.key() == &base || outcome.key() == &overlay)
            .all(|outcome| outcome.status() == UiRootStatus::Published)
    );
    assert!(frame.blocks_unhandled_input());
    assert!(frame.hit_regions().is_empty());
    assert!(frame.focus_order().is_empty());
    assert!(
        frame
            .semantics()
            .iter()
            .all(|node| node.name() != "Activate gallery action")
    );
    assert!(
        frame
            .paint()
            .iter()
            .any(|primitive| primitive.key().is_some_and(|key| key.root_key() == &modal))
    );
}

fn layout<'a>(frame: &'a nara_ui_kernel::UiFrame, local: &str) -> &'a nara_ui_kernel::UiLayoutFact {
    frame
        .layouts()
        .iter()
        .find(|fact| {
            fact.key()
                .and_then(|key| key.local_key())
                .is_some_and(|key| key == &local_key(local).expect("static gallery key is valid"))
        })
        .unwrap_or_else(|| panic!("missing gallery layout for {local}"))
}

fn root_key(local: &str) -> Result<UiRootKey, Box<dyn Error>> {
    Ok(UiRootKey::runtime(DOMAIN, local)?)
}

fn local_key(local: &str) -> Result<UiLocalKey, Box<dyn Error>> {
    Ok(UiLocalKey::runtime(DOMAIN, local)?)
}
