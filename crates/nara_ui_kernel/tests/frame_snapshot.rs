use std::fmt::Write;

use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiColorRole, UiCompiler, UiEnvironment, UiInteraction, UiLayer,
    UiLocalKey, UiNode, UiRoot, UiRootKey, UiSemanticRole, UiSemantics, UiStyle, UiSurfaceId,
    UiText, UiTheme, UiVisual,
};

fn product_snapshot() -> nara_ui_kernel::UiAuthoringSnapshot {
    let status = UiNode::new(UiStyle::absolute(12.0, 10.0, 150.0, 24.0))
        .with_key(UiLocalKey::runtime("reference", "wave-status").unwrap())
        .with_visual(UiVisual::role(UiColorRole::Panel))
        .with_text(UiText::new("Wave 3 / 8", 16.0).unwrap())
        .with_semantics(UiSemantics::new(UiSemanticRole::Status, "Wave 3 of 8").unwrap());
    let retry = UiNode::new(UiStyle::absolute(70.0, 55.0, 100.0, 44.0))
        .with_key(UiLocalKey::runtime("reference", "retry").unwrap())
        .with_visual(UiVisual::role(UiColorRole::Accent))
        .with_text(UiText::new("Retry", 18.0).unwrap())
        .with_interaction(UiInteraction::focus_target())
        .with_semantics(UiSemantics::new(UiSemanticRole::Button, "Retry").unwrap());
    UiAuthoringSnapshotBuilder::new(9)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("reference", "hud").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(status),
        )
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("reference", "terminal").unwrap(),
                UiLayer::Modal,
                UiStyle::fill(),
            )
            .with_child(retry),
        )
        .build()
        .unwrap()
}

fn environment() -> UiEnvironment {
    UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 240.0, 135.0, 1.25, 1, 1, 1, 1).unwrap()
}

fn render_snapshot(frame: &nara_ui_kernel::UiFrame) -> String {
    let mut output = String::from("roots\n");
    for root in frame.root_outcomes() {
        writeln!(&mut output, "  {:?}", root.status()).unwrap();
    }
    output.push_str("layouts\n");
    for layout in frame.layouts() {
        let rect = layout.rect();
        writeln!(
            &mut output,
            "  {:?} {} {:.1},{:.1} {:.1}x{:.1} visible={}",
            layout.layer(),
            layout.structural_index(),
            rect.origin().x(),
            rect.origin().y(),
            rect.size().width(),
            rect.size().height(),
            layout.visible()
        )
        .unwrap();
    }
    writeln!(&mut output, "paint {}", frame.paint().len()).unwrap();
    writeln!(&mut output, "hits {}", frame.hit_regions().len()).unwrap();
    writeln!(&mut output, "focus {}", frame.focus_order().len()).unwrap();
    writeln!(&mut output, "semantics {}", frame.semantics().len()).unwrap();
    output
}

#[test]
fn deterministic_headless_frame_matches_the_reviewed_fixture() {
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let snapshot = product_snapshot();
    let first = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();
    let second = compiler
        .compile(&snapshot, environment(), UiTheme::default())
        .unwrap();

    assert_eq!(first.layouts(), second.layouts());
    assert_eq!(first.paint(), second.paint());
    assert_eq!(first.hit_regions(), second.hit_regions());
    assert_eq!(first.focus_order(), second.focus_order());
    assert_eq!(first.semantics(), second.semantics());
    assert_eq!(
        render_snapshot(&first),
        include_str!("fixtures/reference_frame.txt").replace("\r\n", "\n")
    );
}

#[test]
fn panel_and_text_fragments_share_one_forward_stream() {
    let mut compiler = UiCompiler::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let frame = compiler
        .compile(&product_snapshot(), environment(), UiTheme::default())
        .unwrap();
    let keyed = frame
        .paint()
        .iter()
        .filter_map(|primitive| primitive.key())
        .collect::<Vec<_>>();
    assert_eq!(keyed.len(), 4);
    assert_eq!(keyed[0], keyed[1]);
    assert_eq!(keyed[2], keyed[3]);
    assert_ne!(keyed[0], keyed[2]);
}
