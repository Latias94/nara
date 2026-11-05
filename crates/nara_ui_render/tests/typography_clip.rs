use std::sync::Arc;

use nara_render::RenderTarget;
use nara_text::{
    TextDirection, TextEditor, TextEngine, TextPaintSnapshot, TextTypography, bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiLayer, UiLocalKey, UiNode, UiOverflow,
    UiPaintPrimitive, UiPoint, UiPreparedText, UiRect, UiRoot, UiRootKey, UiSafeAreaInsets, UiSize,
    UiStyle, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId, UiText, UiTextOverflow,
    UiTheme,
};
use nara_ui_render::{UiPacketPrimitive, UiPaintPacket, UiPreparedImages, UiPresentationPlanner};
use nara_window::WindowId;

fn environment(scale: f32) -> UiEnvironment {
    let transform = UiSurfaceContentTransform::new(
        UiSize::new(160.0, 96.0).unwrap(),
        UiSize::new(160.0 * scale + 8.75, 96.0 * scale + 31.0).unwrap(),
        UiSafeAreaInsets::new(3.25, 4.75, 5.5, 6.25).unwrap(),
        2.0,
    )
    .unwrap();
    assert_eq!(transform.content_scale_factor(), scale);
    assert_eq!(
        transform.physical_content_origin(),
        UiPoint::new(3.25, 14.75)
    );
    UiEnvironment::for_surface(
        UiSurfaceId::new(73).unwrap(),
        transform,
        UiSurfaceGenerations::new(1, 1, 1, 1).unwrap(),
        1,
        1,
    )
    .unwrap()
}

fn packet(node: UiNode, environment: UiEnvironment) -> Arc<UiPaintPacket> {
    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("typography-clip", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(node),
        )
        .build()
        .unwrap();
    let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    UiPresentationPlanner::default()
        .plan(
            frame,
            RenderTarget::Window(WindowId::PRIMARY),
            &UiPreparedImages::default(),
        )
        .unwrap()
}

fn prepared_window(
    snapshot: TextPaintSnapshot,
    draw_offset: UiPoint,
    window: UiRect,
    scroll: f32,
) -> UiNode {
    let text = UiNode::new(UiStyle::absolute(
        -scroll,
        0.0,
        snapshot.width().max(1.0),
        window.size().height(),
    ))
    .with_key(UiLocalKey::runtime("typography-clip", "text").unwrap())
    .with_prepared_text(UiPreparedText::new(snapshot).with_draw_offset(draw_offset));
    UiNode::new(
        UiStyle::absolute(
            window.origin().x(),
            window.origin().y(),
            window.size().width(),
            window.size().height(),
        )
        .with_overflow(UiOverflow::Clip),
    )
    .with_child(text)
}

fn visible_ink(packet: &UiPaintPacket) -> Vec<(i32, i32, u8)> {
    let mut ink = Vec::new();
    for primitive in packet.primitives() {
        let UiPacketPrimitive::Glyph {
            rect,
            clip,
            page_index,
            uv,
            ..
        } = primitive
        else {
            panic!("the text fixture must contain only glyph primitives");
        };
        let page = &packet.glyph_pages()[usize::from(*page_index)];
        let atlas_x = (uv.top_left()[0] * page.width() as f32).round() as usize;
        let atlas_y = (uv.top_left()[1] * page.height() as f32).round() as usize;
        let visible = clip.map_or(*rect, |clip| rect.intersect(clip).unwrap());
        for y in visible.min_y()..visible.max_y() {
            for x in visible.min_x()..visible.max_x() {
                let source = (atlas_y + (y - rect.min_y()) as usize) * page.width() as usize
                    + atlas_x
                    + (x - rect.min_x()) as usize;
                let coverage = page.pixels()[source];
                if coverage != 0 {
                    ink.push((x, y, coverage));
                }
            }
        }
    }
    ink
}

#[test]
fn short_line_height_windows_preserve_original_ink_above_the_shifted_draw_origin() {
    let automatic = TextTypography::new(20.0).unwrap();
    let short = automatic.with_line_height(6.0).unwrap();
    let mut engine = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    for text in ["M", "M abc אבג", "Mq\u{301}\u{327}"] {
        let mut editor = TextEditor::with_typography(&mut engine, text, automatic).unwrap();
        let original = editor.paint_snapshot();
        if text.contains('א') {
            assert!(
                editor
                    .layout()
                    .runs()
                    .iter()
                    .any(|run| run.direction() == TextDirection::RightToLeft)
            );
        }
        if text.contains('\u{301}') {
            assert!(
                editor
                    .layout()
                    .glyphs()
                    .iter()
                    .any(|glyph| glyph.advance() == 0.0)
            );
        }
        editor.reconfigure(&mut engine, short).unwrap();
        let (block_min, block_max) = editor
            .layout()
            .lines()
            .iter()
            .fold((0.0_f32, editor.layout().height()), |(min, max), line| {
                (min.min(line.block_min()), max.max(line.block_max()))
            });
        assert!(block_min < 0.0);
        let window = UiRect::from_origin_size(12.25, 16.75, 96.5, block_max - block_min).unwrap();
        assert!((window.size().height() - original.height()).abs() < 0.001);
        let scroll = if text == "M" { 0.0 } else { 6.375 };
        for scale in [1.0, 1.25, 1.5] {
            let environment = environment(scale);
            let reference = packet(
                prepared_window(original.clone(), UiPoint::default(), window, scroll),
                environment,
            );
            let actual = packet(
                prepared_window(
                    editor.paint_snapshot(),
                    UiPoint::new(0.0, -block_min),
                    window,
                    scroll,
                ),
                environment,
            );
            let reference_frame = reference.frame();
            let actual_frame = actual.frame();
            let UiPaintPrimitive::Text {
                clip_rect: reference_clip,
                layout: reference_layout,
                ..
            } = &reference_frame.paint()[0]
            else {
                panic!("the original layout must publish text");
            };
            let UiPaintPrimitive::Text {
                rect,
                clip_rect,
                layout,
                ..
            } = &actual_frame.paint()[0]
            else {
                panic!("the short line must publish text");
            };
            assert_eq!(clip_rect, reference_clip);
            assert_eq!(
                layout
                    .glyphs()
                    .iter()
                    .map(|glyph| glyph.key())
                    .collect::<Vec<_>>(),
                reference_layout
                    .glyphs()
                    .iter()
                    .map(|glyph| glyph.key())
                    .collect::<Vec<_>>(),
            );
            let ink = visible_ink(&actual);
            assert_eq!(ink, visible_ink(&reference), "text {text:?}, scale {scale}");
            let origin_y = (environment
                .content_transform()
                .physical_content_origin()
                .y()
                + rect.origin().y() * scale)
                .round() as i32;
            assert!(ink.iter().any(|(_, y, _)| *y < origin_y));
        }
    }
}

#[test]
fn ordinary_clip_and_visible_overflow_follow_published_bounds_at_fractional_scales() {
    let typography = TextTypography::new(20.0)
        .unwrap()
        .with_line_height(6.0)
        .unwrap();
    let window = UiRect::from_origin_size(12.25, 12.75, 96.5, 48.25).unwrap();
    let text_rect = UiRect::from_origin_size(12.25, 24.75, 220.0, 6.0).unwrap();
    for scale in [1.0, 1.25, 1.5] {
        let environment = environment(scale);
        for overflow in [UiOverflow::Clip, UiOverflow::Visible] {
            let compose = |text_overflow| {
                UiNode::new(
                    UiStyle::absolute(
                        window.origin().x(),
                        window.origin().y(),
                        window.size().width(),
                        window.size().height(),
                    )
                    .with_overflow(overflow),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(0.0, 12.0, 220.0, 6.0))
                        .with_key(UiLocalKey::runtime("typography-clip", "plain-text").unwrap())
                        .with_text(
                            UiText::from_typography("M abc אבג q\u{301}\u{327} MMM", typography)
                                .with_overflow(text_overflow),
                        ),
                )
            };
            let visible = packet(compose(UiTextOverflow::Wrap), environment);
            let clipped = packet(compose(UiTextOverflow::Clip), environment);
            let visible_pixels = visible_ink(&visible);
            let content_clip = environment.physical_rect(text_rect).unwrap();
            let expected = visible_pixels
                .iter()
                .copied()
                .filter(|(x, y, _)| {
                    *x >= content_clip.min_x()
                        && *x < content_clip.max_x()
                        && *y >= content_clip.min_y()
                        && *y < content_clip.max_y()
                })
                .collect::<Vec<_>>();
            assert!(!expected.is_empty());
            assert!(
                visible_pixels
                    .iter()
                    .any(|(_, y, _)| *y < content_clip.min_y())
            );
            assert_eq!(
                visible_ink(&clipped),
                expected,
                "scale {scale}, {overflow:?}"
            );
            if overflow == UiOverflow::Visible {
                let parent_bounds = environment.physical_rect(window).unwrap();
                assert!(
                    visible_pixels
                        .iter()
                        .any(|(x, _, _)| *x >= parent_bounds.max_x())
                );
            }
            let bounds = environment
                .physical_rect(if overflow == UiOverflow::Clip {
                    window
                } else {
                    UiRect::new(UiPoint::default(), environment.logical_size()).unwrap()
                })
                .unwrap();
            assert!(visible_pixels.iter().all(|(x, y, _)| {
                *x >= bounds.min_x()
                    && *x < bounds.max_x()
                    && *y >= bounds.min_y()
                    && *y < bounds.max_y()
            }));
        }
    }
}
