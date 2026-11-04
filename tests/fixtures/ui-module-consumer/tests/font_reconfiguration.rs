//! Public CPU consumer coverage. The public journal API cannot mint presentation receipts, so
//! this caller admits focus through `UiTextField::focus`. Native input and receipt-stamped
//! interaction remain outside this qualification.

use std::sync::Arc;

use nara_text::{
    FontFace, TextDiagnostic, TextEdit, TextEditError, TextEditor, TextEngine, TextLayout,
    TextLayoutRequest, TextLimits, TextRect, TextWindowError, bundled_font_face,
};
use nara_ui_kernel::{
    UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrame, UiLayer,
    UiPaintPrimitive, UiPoint, UiRect, UiRoot, UiRootDiagnostic, UiRootKey, UiRootStatus,
    UiSemanticRole, UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{UiTextField, UiTextFieldError};

const SIZE: f32 = 18.5;
const WIDTH: f32 = 112.0;
const SCALE: f32 = 1.25;

fn faces() -> (FontFace, FontFace) {
    (
        bundled_font_face().unwrap(),
        FontFace::from_bytes(
            include_bytes!("../../../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf")
                .to_vec(),
            0,
        )
        .unwrap(),
    )
}

fn root() -> UiRootKey {
    UiRootKey::runtime("external-font-host", "field").unwrap()
}

fn environment() -> UiEnvironment {
    // Keep font_revision fixed: immutable font identities, not a version bump, must authorize
    // prepared sources. The fractional scale and origin exercise final raster placement.
    UiEnvironment::new(
        UiSurfaceId::new(93).unwrap(),
        320.0,
        120.0,
        SCALE,
        1,
        1,
        1,
        1,
    )
    .unwrap()
}

fn authored(field: &UiTextField, revision: u64) -> UiAuthoringSnapshot {
    let root = root();
    UiAuthoringSnapshotBuilder::new(revision)
        .with_root(
            UiRoot::new(root.clone(), UiLayer::Base, UiStyle::fill()).with_child(
                field
                    .authored_node(&root, UiStyle::absolute(12.25, 10.5, WIDTH, 44.0), WIDTH)
                    .unwrap(),
            ),
        )
        .build()
        .unwrap()
}

fn compile_field(compiler: &mut UiCompiler, field: &UiTextField, revision: u64) -> Arc<UiFrame> {
    let frame = compiler
        .compile(
            &authored(field, revision),
            environment(),
            UiTheme::default(),
        )
        .unwrap();
    assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
    assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
    let semantic = frame
        .semantics()
        .iter()
        .find(|node| node.key() == field.key())
        .unwrap();
    assert_eq!(semantic.role(), UiSemanticRole::TextInput);
    assert_eq!(semantic.value(), Some(field.editor().text()));
    assert_eq!(semantic.enabled(), field.enabled());
    let content = frame
        .semantics()
        .iter()
        .find(|node| node.role() == UiSemanticRole::Text)
        .unwrap();
    assert_eq!(content.name(), field.editor().text());
    assert_eq!(content.parent(), Some(field.key()));
    assert_eq!(content.value(), None);
    assert_eq!(
        frame
            .focus_order()
            .iter()
            .find(|fact| fact.key() == field.key())
            .unwrap()
            .enabled(),
        field.enabled()
    );
    frame
}

fn painted_text(frame: &UiFrame) -> (UiRect, UiRect, &TextLayout) {
    let mut texts = frame
        .paint()
        .iter()
        .filter_map(|primitive| match primitive {
            UiPaintPrimitive::Text {
                rect,
                clip_rect: Some(clip),
                layout,
                ..
            } => Some((*rect, *clip, layout.as_ref())),
            _ => None,
        });
    let text = texts.next().expect("the field publishes clipped text");
    assert!(texts.next().is_none());
    text
}

fn assert_window(field: &UiTextField, frame: &UiFrame, engine: &mut TextEngine) {
    assert_eq!(field.editor().logical_size(), SIZE);
    let (rect, clip, painted) = painted_text(frame);
    let physical_origin = frame
        .environment()
        .content_transform()
        .physical_content_origin();
    let expected = engine
        .prepare_window_at(
            &field.editor().paint_snapshot(),
            TextRect {
                x0: clip.origin().x(),
                y0: clip.origin().y(),
                x1: clip.max().x(),
                y1: clip.max().y(),
            },
            SCALE,
            [rect.origin().x(), rect.origin().y()],
            [physical_origin.x(), physical_origin.y()],
        )
        .unwrap();
    assert_eq!(painted, expected.as_ref());
    assert_eq!(painted.width(), field.editor().layout().width());
    assert_eq!(painted.height(), field.editor().layout().height());
    assert!(!painted.glyphs().is_empty());
    for glyph in painted.glyphs() {
        assert_eq!(glyph.key().physical_size(), SIZE * SCALE);
        assert!(field.editor().layout().glyphs().iter().any(|source| {
            source.key().face_id() == glyph.key().face_id()
                && source.key().glyph_id() == glyph.key().glyph_id()
                && source.x() == glyph.x()
                && source.y() == glyph.y()
                && source.advance() == glyph.advance()
        }));
        let raster = engine.rasterize(glyph.key()).unwrap();
        assert!(!raster.mask().is_empty());
        assert!(raster.mask().iter().any(|coverage| *coverage != 0));
    }
}

#[test]
fn reordered_fonts_preserve_live_composition_history_and_fresh_field_point_geometry() {
    let (sans, mono) = faces();
    let old_fonts = vec![sans.clone(), mono.clone()];
    let new_fonts = vec![mono.clone(), sans.clone()];
    let mut old_engine = TextEngine::with_fonts(old_fonts.clone(), Default::default()).unwrap();
    let mut new_engine = TextEngine::with_fonts(new_fonts.clone(), Default::default()).unwrap();
    let mut old_compiler = UiCompiler::with_fonts(old_fonts, Default::default()).unwrap();
    let mut new_compiler = UiCompiler::with_fonts(new_fonts, Default::default()).unwrap();

    // Both the primary choice and the fallback direction change with real font coverage.
    let request = TextLayoutRequest::new("Aƀ∩", SIZE).unwrap();
    let old_probe = old_engine.layout(&request).unwrap();
    let new_probe = new_engine.layout(&request).unwrap();
    assert!(old_probe.diagnostics().is_empty());
    assert!(new_probe.diagnostics().is_empty());
    assert_eq!(
        old_probe
            .glyphs()
            .iter()
            .map(|glyph| glyph.key().face_id())
            .collect::<Vec<_>>(),
        [sans.id(), sans.id(), mono.id()]
    );
    assert_eq!(
        new_probe
            .glyphs()
            .iter()
            .map(|glyph| glyph.key().face_id())
            .collect::<Vec<_>>(),
        [mono.id(), sans.id(), mono.id()]
    );

    let mut field = UiTextField::new(root(), "Value", "Aƀ∩", SIZE, &mut old_engine).unwrap();
    let token = field.focus().unwrap();
    let key = field.key().clone();
    let draft = format!("{}ƀ∩Wi", "iiWW".repeat(8));
    for action in [
        TextEdit::SelectAll,
        TextEdit::Commit(draft.clone()),
        TextEdit::Insert("!".into()),
        TextEdit::Undo,
        TextEdit::SelectAll,
    ] {
        field.edit(token, &action, &mut old_engine).unwrap();
    }
    let origin_selection = field.editor().selection();
    let preedit = format!("{}ƀ∩WW", "WWii".repeat(8));
    field
        .edit(
            token,
            &TextEdit::Preedit {
                cursor: Some((preedit.len(), preedit.len())),
                text: preedit.clone(),
            },
            &mut old_engine,
        )
        .unwrap();
    let selection = field.editor().selection();
    let composition = field.editor().composition_range();
    let old_layout = field.editor().layout().clone();
    let old_source = field.editor().paint_snapshot();
    let old_authored = authored(&field, 1);
    let old_frame = compile_field(&mut old_compiler, &field, 1);
    assert_window(&field, &old_frame, &mut old_engine);

    let outcome = field.reconfigure_fonts(&mut new_engine).unwrap();
    assert!(outcome.display_changed);
    assert!(!outcome.text_changed);
    assert!(!outcome.composition_changed);
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert!(field.enabled());
    assert_eq!(field.editor().text(), draft);
    assert_eq!(field.editor().display_text(), preedit);
    assert_eq!(field.editor().selection(), selection);
    assert_eq!(field.editor().composition_range(), composition);
    assert!(field.editor().is_composing());
    assert!(field.editor().can_undo());
    assert!(field.editor().can_redo());
    assert_ne!(field.editor().layout().width(), old_layout.width());
    let fresh = TextEditor::new(&mut new_engine, &preedit, SIZE).unwrap();
    assert_eq!(field.editor().layout(), fresh.layout());
    let new_source = field.editor().paint_snapshot();
    assert!(!old_source.same_source(&new_source));
    assert_eq!(old_source.display_text(), preedit);
    assert_eq!(old_source.width(), old_layout.width());
    old_engine
        .validate_paint_snapshot(&old_source, SCALE)
        .unwrap();
    assert_eq!(
        new_engine.validate_paint_snapshot(&old_source, SCALE),
        Err(TextWindowError::FontSetMismatch)
    );
    assert_eq!(
        old_engine.validate_paint_snapshot(&new_source, SCALE),
        Err(TextWindowError::FontSetMismatch)
    );
    // A compiled field cannot carry a previous ordered font set into the new receiver.
    let refused = new_compiler
        .compile(&old_authored, environment(), UiTheme::default())
        .unwrap();
    assert_eq!(refused.root_outcomes()[0].status(), UiRootStatus::Absent);
    assert!(matches!(
        refused.root_outcomes()[0].diagnostic(),
        Some(UiRootDiagnostic::TextRejected { .. })
    ));
    assert!(refused.paint().is_empty());
    assert!(refused.semantics().is_empty());
    assert!(refused.hit_regions().is_empty());
    assert!(refused.focus_order().is_empty());
    let new_frame = compile_field(&mut new_compiler, &field, 2);
    assert_window(&field, &new_frame, &mut new_engine);
    let (new_rect, clip, painted) = painted_text(&new_frame);
    assert!(painted.glyphs().len() < field.editor().layout().glyphs().len());
    assert!(new_rect.origin().x() < clip.origin().x());
    assert_ne!(
        new_rect.origin().x(),
        painted_text(&old_frame).0.origin().x()
    );
    assert!(
        painted
            .glyphs()
            .iter()
            .any(|glyph| glyph.key().face_id() == sans.id())
    );
    assert!(painted.glyphs().iter().any(|glyph| {
        glyph.key().face_id() == mono.id()
            && glyph.key().glyph_id() == new_probe.glyphs()[2].key().glyph_id()
    }));
    assert!(
        new_frame.semantics().iter().all(|node| {
            node.name() != preedit.as_str() && node.value() != Some(preedit.as_str())
        })
    );

    let retained = old_compiler
        .compile(&authored(&field, 2), environment(), UiTheme::default())
        .unwrap();
    assert_eq!(
        retained.root_outcomes()[0].status(),
        UiRootStatus::RetainedVisual
    );
    assert_eq!(retained.paint(), old_frame.paint());
    assert!(retained.semantics().is_empty());
    assert!(retained.hit_regions().is_empty());
    assert!(retained.focus_order().is_empty());

    field
        .edit(token, &TextEdit::CancelComposition, &mut new_engine)
        .unwrap();
    assert_eq!(field.editor().text(), draft);
    assert_eq!(field.editor().display_text(), draft);
    assert_eq!(field.editor().selection(), origin_selection);
    assert!(!field.editor().is_composing());
    field.edit(token, &TextEdit::Undo, &mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "Aƀ∩");
    assert!(
        !field.editor().can_undo(),
        "font replacement must not record an edit"
    );
    for expected in [draft.clone(), format!("{draft}!")] {
        field.edit(token, &TextEdit::Redo, &mut new_engine).unwrap();
        assert_eq!(field.editor().text(), expected);
    }
    assert!(!field.editor().can_redo());

    // Only the newly compiled geometry supplies point coordinates after the replacement.
    let current = compile_field(&mut new_compiler, &field, 3);
    assert_window(&field, &current, &mut new_engine);
    let field_rect = current
        .layouts()
        .iter()
        .find(|fact| fact.key() == Some(&key))
        .unwrap()
        .rect();
    let (text_rect, clip, painted) = painted_text(&current);
    let line = &field.editor().layout().lines()[0];
    let y = text_rect.origin().y() + (line.block_min() + line.block_max()) * 0.5;
    let point = painted
        .glyphs()
        .iter()
        .map(|glyph| {
            UiPoint::new(
                text_rect.origin().x() + glyph.x() + glyph.advance() * 0.25,
                y,
            )
        })
        .find(|point| clip.contains(*point))
        .unwrap();
    assert_eq!(current.hit_test(point).unwrap().key(), &key);
    let mut expected = TextEditor::new(&mut new_engine, field.editor().text(), SIZE).unwrap();
    expected
        .edit(
            &mut new_engine,
            &TextEdit::Point {
                x: point.x() - text_rect.origin().x(),
                y: point.y() - text_rect.origin().y(),
                extend: false,
            },
        )
        .unwrap();
    field
        .point(
            token,
            UiPoint::new(
                point.x() - field_rect.origin().x(),
                point.y() - field_rect.origin().y(),
            ),
            field_rect.size().width(),
            false,
            &mut new_engine,
        )
        .unwrap();
    assert_eq!(field.editor().selection(), expected.selection());
    assert_eq!(field.editor().caret(), expected.caret());
    assert_eq!(field.input_token(), Some(token));
    let pointed = compile_field(&mut new_compiler, &field, 4);
    assert_window(&field, &pointed, &mut new_engine);
    let caret = pointed
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Rect {
                key: None, rect, ..
            } => Some(*rect),
            _ => None,
        })
        .unwrap();
    let (text_rect, _, _) = painted_text(&pointed);
    let source_caret = expected.caret().unwrap();
    assert!((caret.origin().x() - text_rect.origin().x() - source_caret.x0).abs() < 0.001);
    assert!((caret.origin().y() - text_rect.origin().y() - source_caret.y0).abs() < 0.001);
    let ime = field.ime_area(field_rect.size().width()).unwrap();
    assert!(ime.origin().x() >= 0.0 && ime.max().x() <= field_rect.size().width());
    assert!(field_rect.origin().x() + ime.origin().x() <= caret.origin().x() + 0.001);
    assert!(field_rect.origin().x() + ime.max().x() >= caret.max().x() - 0.001);
}

#[test]
fn differing_limits_refuse_atomically_allow_old_engine_edits_and_retry_with_composition_confirm() {
    let (sans, mono) = faces();
    let limits = TextLimits {
        max_text_bytes: 128,
        ..Default::default()
    };
    let old_fonts = vec![sans.clone(), mono.clone()];
    let new_fonts = vec![mono, sans];
    let mut old_engine = TextEngine::with_fonts(old_fonts, limits).unwrap();
    let mut mismatched = TextEngine::with_fonts(
        new_fonts.clone(),
        TextLimits {
            max_text_bytes: 127,
            ..limits
        },
    )
    .unwrap();
    let mut new_engine = TextEngine::with_fonts(new_fonts.clone(), limits).unwrap();
    let mut compiler =
        UiCompiler::with_fonts_and_text_limits(new_fonts, Default::default(), limits).unwrap();
    let mut field = UiTextField::new(root(), "Value", "seed", SIZE, &mut old_engine).unwrap();
    let token = field.focus().unwrap();
    let key = field.key().clone();
    for action in [
        TextEdit::SelectAll,
        TextEdit::Commit("WiWi∩".into()),
        TextEdit::Insert("!".into()),
        TextEdit::Undo,
        TextEdit::SelectAll,
        TextEdit::Preedit {
            text: "W∩i".into(),
            cursor: Some((4, 5)),
        },
    ] {
        field.edit(token, &action, &mut old_engine).unwrap();
    }
    let old_source = field.editor().paint_snapshot();
    let before = field.editor().clone();
    assert!(before.can_undo() && before.can_redo() && before.is_composing());
    // The complete source fits both policies. Even this non-exhausted byte-limit difference
    // must refuse because the editor's original policy is immutable.
    assert!(before.display_text().len() < 127);
    assert!(matches!(
        field.reconfigure_fonts(&mut mismatched),
        Err(UiTextFieldError::Text(TextEditError::EngineMismatch))
    ));
    assert_eq!(field.editor().text(), before.text());
    assert_eq!(field.editor().display_text(), before.display_text());
    assert_eq!(field.editor().selection(), before.selection());
    assert_eq!(field.editor().layout(), before.layout());
    assert_eq!(
        field.editor().composition_range(),
        before.composition_range()
    );
    assert!(old_source.same_source(&field.editor().paint_snapshot()));
    assert!(field.editor().can_undo() && field.editor().can_redo());
    assert_eq!(field.key(), &key);
    assert_eq!(field.input_token(), Some(token));
    assert!(field.enabled());
    old_engine
        .validate_paint_snapshot(&old_source, SCALE)
        .unwrap();

    for (action, expected) in [
        (TextEdit::CancelComposition, "WiWi∩"),
        (TextEdit::Redo, "WiWi∩!"),
        (TextEdit::Undo, "WiWi∩"),
        (TextEdit::Commit("old∩engine".into()), "old∩engine"),
    ] {
        field.edit(token, &action, &mut old_engine).unwrap();
        assert_eq!(field.editor().text(), expected);
    }
    field
        .edit(token, &TextEdit::SelectAll, &mut old_engine)
        .unwrap();
    let committed_selection = field.editor().selection();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "ii∩WW".into(),
                cursor: Some((0, 2)),
            },
            &mut old_engine,
        )
        .unwrap();
    let selection = field.editor().selection();
    let composition = field.editor().composition_range();
    let outcome = field.reconfigure_fonts(&mut new_engine).unwrap();
    assert!(outcome.display_changed && !outcome.text_changed && !outcome.composition_changed);
    assert_eq!(field.editor().text(), "old∩engine");
    assert_eq!(field.editor().display_text(), "ii∩WW");
    assert_eq!(field.editor().selection(), selection);
    assert_eq!(field.editor().composition_range(), composition);
    assert_eq!(field.input_token(), Some(token));
    assert_eq!(field.key(), &key);
    let composing = compile_field(&mut compiler, &field, 1);
    assert_window(&field, &composing, &mut new_engine);

    field
        .edit(token, &TextEdit::Commit("new∩".into()), &mut new_engine)
        .unwrap();
    assert_eq!(field.editor().text(), "new∩");
    assert!(!field.editor().is_composing());
    let confirmed = compile_field(&mut compiler, &field, 2);
    assert_window(&field, &confirmed, &mut new_engine);
    field.edit(token, &TextEdit::Undo, &mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "old∩engine");
    assert_eq!(field.editor().selection(), committed_selection);
    field.edit(token, &TextEdit::Redo, &mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "new∩");
    assert!(!field.editor().can_redo());
    assert_eq!(field.input_token(), Some(token));
    assert_eq!(field.key(), &key);
}

#[test]
fn admitting_the_real_fallback_adds_supported_intersection_glyphs_to_a_live_preedit() {
    let (sans, mono) = faces();
    let mut old_engine = TextEngine::new(sans.clone(), Default::default()).unwrap();
    let new_fonts = vec![sans.clone(), mono.clone()];
    let mut new_engine = TextEngine::with_fonts(new_fonts.clone(), Default::default()).unwrap();
    let mut compiler = UiCompiler::with_fonts(new_fonts, Default::default()).unwrap();
    let mut field = UiTextField::new(root(), "Value", "A", SIZE, &mut old_engine).unwrap();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut old_engine)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "A∩".into(),
                cursor: Some((4, 4)),
            },
            &mut old_engine,
        )
        .unwrap();
    assert!(
        field
            .editor()
            .layout()
            .diagnostics()
            .contains(&TextDiagnostic::MissingGlyph { scalar: '∩' })
    );
    field.reconfigure_fonts(&mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "A");
    assert_eq!(field.editor().display_text(), "A∩");
    assert_eq!(field.input_token(), Some(token));
    assert!(field.editor().is_composing());
    assert!(field.editor().layout().diagnostics().is_empty());
    assert_eq!(
        field
            .editor()
            .layout()
            .glyphs()
            .iter()
            .map(|glyph| glyph.key().face_id())
            .collect::<Vec<_>>(),
        [sans.id(), mono.id()]
    );
    assert!(
        field
            .editor()
            .layout()
            .glyphs()
            .iter()
            .all(|glyph| glyph.key().glyph_id() != 0)
    );
    let frame = compile_field(&mut compiler, &field, 1);
    assert_window(&field, &frame, &mut new_engine);
    let painted = painted_text(&frame).2;
    assert_eq!(painted.glyphs().len(), 2);
    assert_eq!(painted.glyphs()[1].key().face_id(), mono.id());
    assert_ne!(painted.glyphs()[1].key().glyph_id(), 0);
    field
        .edit(token, &TextEdit::Commit("A∩".into()), &mut new_engine)
        .unwrap();
    assert_eq!(field.editor().text(), "A∩");
    field.edit(token, &TextEdit::Undo, &mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "A");
    assert_eq!(field.editor().selection_bytes(), 0..1);
    assert!(!field.editor().can_undo());
    field.edit(token, &TextEdit::Redo, &mut new_engine).unwrap();
    assert_eq!(field.editor().text(), "A∩");
    assert!(!field.editor().can_redo());
    let confirmed = compile_field(&mut compiler, &field, 2);
    assert_window(&field, &confirmed, &mut new_engine);
}
