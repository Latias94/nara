//! Independent CPU workflows. Public field focus is used deliberately; these tests grant no
//! presented-input, native drawing, OS input-method or accessibility authority.

use std::{collections::HashSet, sync::Arc};

use nara_text::{TextEdit, TextEngine, TextLimits, bundled_font_face};
use nara_ui_kernel::{
    UiActionId, UiAuthoringSnapshot, UiAuthoringSnapshotBuilder, UiColor, UiCompiler,
    UiCustomLayout, UiEdges, UiEnvironment, UiFrame, UiKey, UiLayer, UiOpaqueId, UiPaintPrimitive,
    UiPoint, UiResolvedBrush, UiRoot, UiRootKey, UiRootStatus, UiSemanticNode, UiSemanticRole,
    UiStyle, UiSurfaceId, UiTheme,
};
use nara_ui_module_consumer::{
    StatusLabelError,
    measured_column::MeasuredColumn,
    styled_property_row::{StyledProperty, StyledPropertyRow},
};
use nara_ui_widgets::{
    UiAppearance, UiControlSize, UiDensity, UiStateBinding, UiStateHandle, UiStateStore, UiTextRole,
};

type Row = UiStateHandle<StyledPropertyRow>;

struct Consumer {
    state: UiStateStore,
    engine: TextEngine,
    compiler: UiCompiler,
}

impl Consumer {
    fn new() -> Self {
        Self::with_fonts(vec![bundled_font_face().unwrap()])
    }

    fn with_fonts(fonts: Vec<nara_text::FontFace>) -> Self {
        let root = UiRootKey::runtime("independent-styled", "properties").unwrap();
        Self {
            state: UiStateStore::new(root, 16).unwrap(),
            engine: TextEngine::with_fonts(fonts.clone(), Default::default()).unwrap(),
            compiler: UiCompiler::with_fonts(fonts, Default::default()).unwrap(),
        }
    }

    fn retain(&mut self, order: &[(&str, &str)], appearance: UiAppearance) -> Vec<Row> {
        let mut pass = self.state.begin();
        let parent = pass.root().child("properties").unwrap();
        let mut rows = Vec::new();
        for &(id, value) in order {
            rows.push(
                pass.try_state(
                    parent.child(id).unwrap(),
                    UiStateBinding::new(UiOpaqueId::new(b"document-one").unwrap(), 1),
                    || -> Result<_, StatusLabelError> {
                        StyledPropertyRow::new(
                            parent.root_key().clone(),
                            "properties",
                            StyledProperty {
                                id,
                                label: "Value",
                                initial_value: value,
                                action_label: "Apply",
                                action: UiActionId::new(format!("property.apply.{id}"))?,
                            },
                            appearance,
                            &mut self.engine,
                        )
                    },
                )
                .unwrap(),
            );
        }
        pass.finish().unwrap();
        rows
    }

    fn appearance(&mut self, row: &Row, appearance: UiAppearance) {
        self.state
            .get_mut(row)
            .unwrap()
            .set_appearance(appearance, &mut self.engine)
            .unwrap();
    }

    fn compile(
        &mut self,
        rows: &[Row],
        width: f32,
        scale: f32,
        revision: u64,
    ) -> (UiAuthoringSnapshot, Arc<UiFrame>) {
        let mut root = UiRoot::new(
            self.state.root().root_key().clone(),
            UiLayer::Base,
            UiStyle::fill().with_padding(UiEdges::all(8.0).unwrap()),
        )
        .with_custom_layout(UiCustomLayout::new(MeasuredColumn { gap: 8.0 }));
        for row in rows {
            root = root.with_child(
                self.state
                    .get_mut(row)
                    .unwrap()
                    .authored_node(width, "Ready", &mut self.engine)
                    .unwrap(),
            );
        }
        let snapshot = UiAuthoringSnapshotBuilder::new(revision)
            .with_root(root)
            .build()
            .unwrap();
        let frame = self
            .compiler
            .compile(
                &snapshot,
                UiEnvironment::new(
                    UiSurfaceId::new(95).unwrap(),
                    width + 16.0,
                    720.0,
                    scale,
                    revision,
                    revision,
                    1,
                    1,
                )
                .unwrap(),
                UiTheme::default(),
            )
            .unwrap();
        assert_eq!(frame.root_outcomes()[0].status(), UiRootStatus::Published);
        assert!(frame.diagnostics().is_empty(), "{:?}", frame.diagnostics());
        (snapshot, frame)
    }

    fn key(&self, row: &Row) -> UiKey {
        self.state.get(row).unwrap().field().key().clone()
    }
}

#[test]
fn actual_fallback_field_baseline_survives_density_scaling_preedit_and_full_history() {
    let cjk = nara_text::FontFace::from_bytes(
        include_bytes!("../../../../crates/nara_text/tests/fixtures/NotoSansSC-subset.otf")
            .to_vec(),
        0,
    )
    .unwrap();
    let mut consumer = Consumer::with_fonts(vec![bundled_font_face().unwrap(), cjk.clone()]);
    let base = UiAppearance::default();
    let rows = consumer.retain(&[("alpha", "before")], base);
    let key = consumer.key(&rows[0]);
    let button = consumer.state.get(&rows[0]).unwrap().button().key().clone();
    let value = format!("{}x", "中".repeat(1365));
    assert_eq!(value.len(), 4096);
    let field = consumer.state.get_mut(&rows[0]).unwrap().field_mut();
    let token = field.focus().unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut consumer.engine)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Commit(value.clone()),
            &mut consumer.engine,
        )
        .unwrap();
    field
        .edit(token, &TextEdit::SelectAll, &mut consumer.engine)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Preedit {
                text: "文".into(),
                cursor: Some((3, 3)),
            },
            &mut consumer.engine,
        )
        .unwrap();
    for (index, (density, text_scale)) in [
        (UiDensity::Normal, 1.0),
        (UiDensity::Compact, 1.0),
        (UiDensity::Compact, 2.0),
        (UiDensity::Normal, 2.0),
    ]
    .into_iter()
    .enumerate()
    {
        consumer.appearance(
            &rows[0],
            base.with_density(density)
                .with_text_scale(text_scale)
                .unwrap(),
        );
        let (_, frame) = consumer.compile(&rows, 1300.0, 1.25, index as u64 + 1);
        let parent = semantic(&frame, &key).parent().unwrap();
        let content = text_child(&frame, &key, &value);
        let baseline = text_baseline(&frame, content.key());
        for slot in [
            text_child(&frame, parent, "Value").key(),
            &button,
            text_child(&frame, parent, "Ready").key(),
        ] {
            close(text_baseline(&frame, slot), baseline);
        }
        let field = consumer.state.get(&rows[0]).unwrap().field();
        assert_eq!(field.input_token(), Some(token));
        assert!(field.editor().is_composing());
        assert_eq!(field.editor().text(), value);
        assert_eq!(
            field.editor().layout().glyphs()[0].key().face_id(),
            cjk.id()
        );
        let rect = semantic(&frame, &key).rect();
        let candidate = field.ime_area(rect.size().width()).unwrap();
        assert!(candidate.max().y() <= rect.size().height() + 0.02);
        assert!(candidate.origin().y() >= 0.0);
    }
    let field = consumer.state.get_mut(&rows[0]).unwrap().field_mut();
    field
        .edit(token, &TextEdit::CancelComposition, &mut consumer.engine)
        .unwrap();
    field
        .edit(token, &TextEdit::Undo, &mut consumer.engine)
        .unwrap();
    assert_eq!(field.editor().text(), "before");
    field
        .edit(token, &TextEdit::Redo, &mut consumer.engine)
        .unwrap();
    assert_eq!(field.editor().text(), value);
}

fn semantic<'a>(frame: &'a UiFrame, key: &UiKey) -> &'a UiSemanticNode {
    frame
        .semantics()
        .iter()
        .find(|node| node.key() == key)
        .unwrap()
}

fn text_child<'a>(frame: &'a UiFrame, parent: &UiKey, name: &str) -> &'a UiSemanticNode {
    frame
        .semantics()
        .iter()
        .find(|node| {
            node.parent() == Some(parent)
                && node.role() == UiSemanticRole::Text
                && node.name() == name
        })
        .unwrap()
}

fn text_baseline(frame: &UiFrame, key: &UiKey) -> f32 {
    frame
        .paint()
        .iter()
        .find_map(|primitive| match primitive {
            UiPaintPrimitive::Text {
                key: Some(painted),
                rect,
                layout,
                ..
            } if painted == key => {
                Some(rect.origin().y() + layout.lines().first().unwrap().baseline())
            }
            _ => None,
        })
        .unwrap()
}

fn row_paint(frame: &UiFrame, field: &UiKey) -> Vec<UiPaintPrimitive> {
    let row = semantic(frame, field).parent().unwrap();
    let mut keys = HashSet::from([row.clone()]);
    // Semantics arrive in parent-first order. This includes the full prepared field text.
    for node in frame.semantics() {
        if node.parent().is_some_and(|parent| keys.contains(parent)) {
            keys.insert(node.key().clone());
        }
    }
    frame
        .paint()
        .iter()
        .filter(|paint| paint.key().is_some_and(|key| keys.contains(key)))
        .cloned()
        .collect()
}

fn background(frame: &UiFrame, key: &UiKey) -> UiColor {
    frame
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Rect {
                key: Some(painted),
                brush: Some(UiResolvedBrush::Solid(color)),
                ..
            } if painted == key => Some(*color),
            _ => None,
        })
        .unwrap()
}

fn close(left: f32, right: f32) {
    assert!((left - right).abs() < 0.02, "{left} differs from {right}");
}

#[test]
fn equal_labels_keep_keyed_drafts_preedit_selection_focus_and_history_through_style_changes() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let original = consumer.retain(&[("alpha", "Alpha"), ("beta", "Beta")], base);
    let alpha_key = consumer.key(&original[0]);
    let beta_key = consumer.key(&original[1]);
    assert_ne!(alpha_key, beta_key);
    let field = consumer.state.get_mut(&original[0]).unwrap().field_mut();
    let token = field.focus().unwrap();
    for edit in [
        TextEdit::SelectAll,
        TextEdit::Commit("Draft α e\u{301}".to_owned()),
        TextEdit::Insert("!".to_owned()),
        TextEdit::Undo,
        TextEdit::SelectAll,
        TextEdit::Preedit {
            text: "中文".to_owned(),
            cursor: Some((0, 6)),
        },
    ] {
        field.edit(token, &edit, &mut consumer.engine).unwrap();
    }
    let selection = field.editor().selection();
    let composition = field.editor().composition_range();
    assert!(field.editor().can_undo() && field.editor().can_redo());
    let (_, before) = consumer.compile(&original, 760.0, 1.25, 1);
    let old_paint = before.paint().to_vec();
    let mut palette = base.palette();
    palette.input = UiColor::rgba(0.08, 0.03, 0.12, 1.0);
    palette.text = UiColor::rgba(0.93, 0.87, 0.68, 1.0);
    let colored = base.with_palette(palette).unwrap();
    for (index, appearance) in [
        colored,
        colored.with_density(UiDensity::Compact),
        colored.with_text_scale(1.5).unwrap(),
        base,
    ]
    .into_iter()
    .enumerate()
    {
        consumer.appearance(&original[0], appearance);
        let reordered = consumer.retain(&[("beta", "ignored"), ("alpha", "ignored")], base);
        assert_eq!(reordered, [original[1].clone(), original[0].clone()]);
        assert_eq!(consumer.state.last_changes().initialized, 0);
        let field = consumer.state.get(&original[0]).unwrap().field();
        assert_eq!(field.key(), &alpha_key);
        assert_eq!(field.input_token(), Some(token));
        assert_eq!(field.editor().text(), "Draft α e\u{301}");
        assert_eq!(field.editor().display_text(), "中文");
        assert_eq!(field.editor().selection(), selection);
        assert_eq!(field.editor().composition_range(), composition);
        assert!(field.editor().can_undo() && field.editor().can_redo());
        let (_, frame) = consumer.compile(&reordered, 760.0, 1.25, index as u64 + 2);
        let inputs: Vec<_> = frame
            .semantics()
            .iter()
            .filter(|node| node.role() == UiSemanticRole::TextInput)
            .collect();
        assert_eq!(inputs.len(), 2);
        assert!(inputs.iter().all(|node| node.name() == "Value"));
        assert_eq!(
            semantic(&frame, &alpha_key).value(),
            Some("Draft α e\u{301}")
        );
        assert_eq!(semantic(&frame, &beta_key).value(), Some("Beta"));
    }
    assert_eq!(
        before.paint(),
        old_paint.as_slice(),
        "published prior paint remains immutable"
    );
    let field = consumer.state.get_mut(&original[0]).unwrap().field_mut();
    for (edit, expected) in [
        (TextEdit::CancelComposition, "Draft α e\u{301}"),
        (TextEdit::Redo, "Draft α e\u{301}!"),
        (TextEdit::Undo, "Draft α e\u{301}"),
        (TextEdit::Undo, "Alpha"),
        (TextEdit::Redo, "Draft α e\u{301}"),
    ] {
        field.edit(token, &edit, &mut consumer.engine).unwrap();
        assert_eq!(field.editor().text(), expected);
    }
    assert_eq!(
        consumer
            .state
            .get(&original[1])
            .unwrap()
            .field()
            .editor()
            .text(),
        "Beta"
    );
}

#[test]
fn nearest_scope_override_and_removal_restore_exact_paint_without_changing_a_sibling() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let rows = consumer.retain(&[("alpha", "Alpha"), ("beta", "Beta")], base);
    let alpha = consumer.key(&rows[0]);
    let beta = consumer.key(&rows[1]);
    let (_, original) = consumer.compile(&rows, 760.0, 1.25, 1);
    let source = consumer
        .state
        .get(&rows[0])
        .unwrap()
        .field()
        .editor()
        .paint_snapshot();
    let mut palette = base.palette();
    palette.panel = UiColor::rgba(0.10, 0.04, 0.14, 1.0);
    palette.input = UiColor::rgba(0.03, 0.06, 0.12, 1.0);
    palette.text = UiColor::rgba(0.96, 0.82, 0.62, 1.0);
    palette.muted_text = UiColor::rgba(0.70, 0.60, 0.83, 1.0);
    let resolve_scope = |local: Option<UiAppearance>| local.unwrap_or(base);
    consumer.appearance(
        &rows[0],
        resolve_scope(Some(base.with_palette(palette).unwrap())),
    );
    let (_, overridden) = consumer.compile(&rows, 760.0, 1.25, 2);
    assert_ne!(row_paint(&original, &alpha), row_paint(&overridden, &alpha));
    assert_eq!(row_paint(&original, &beta), row_paint(&overridden, &beta));
    assert_eq!(background(&overridden, &alpha), palette.input);
    assert!(
        source.same_source(
            &consumer
                .state
                .get(&rows[0])
                .unwrap()
                .field()
                .editor()
                .paint_snapshot()
        )
    );
    consumer.appearance(&rows[0], resolve_scope(None));
    let (_, restored) = consumer.compile(&rows, 760.0, 1.25, 3);
    assert_eq!(row_paint(&original, &alpha), row_paint(&restored, &alpha));
    assert_eq!(row_paint(&original, &beta), row_paint(&restored, &beta));
    assert_eq!(
        semantic(&original, &alpha).rect(),
        semantic(&restored, &alpha).rect()
    );
    assert_eq!(consumer.state.get(&rows[1]).unwrap().appearance(), base);
}

#[test]
fn density_changes_spacing_independently_of_user_text_scale_and_native_scale() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let rows = consumer.retain(&[("alpha", "Measured value")], base);
    let key = consumer.key(&rows[0]);
    let mut heights = Vec::new();
    for (revision, text_scale) in [(1, 1.0), (3, 1.5)] {
        let normal = base.with_text_scale(text_scale).unwrap();
        consumer.appearance(&rows[0], normal);
        let (_, regular) = consumer.compile(&rows, 900.0, 1.0, revision);
        let compact = normal.with_density(UiDensity::Compact);
        consumer.appearance(&rows[0], compact);
        let (_, dense) = consumer.compile(&rows, 900.0, 1.0, revision + 1);
        let field = consumer.state.get(&rows[0]).unwrap().field();
        assert_eq!(
            field.editor().typography(),
            normal.typography(UiTextRole::Body).unwrap()
        );
        let regular_height = semantic(&regular, &key).rect().size().height();
        let compact_height = semantic(&dense, &key).rect().size().height();
        assert!(compact_height < regular_height);
        let content = text_child(&regular, &key, "Measured value");
        for frame in [&regular, &dense] {
            let painted = frame
                .paint()
                .iter()
                .find_map(|paint| match paint {
                    UiPaintPrimitive::Text {
                        key: Some(painted),
                        layout,
                        ..
                    } if painted == content.key() => Some(layout),
                    _ => None,
                })
                .unwrap();
            close(painted.typography().logical_size(), 14.0 * text_scale);
            close(painted.height(), 20.0 * text_scale);
        }
        heights.push(compact_height);
        let (_, physical) = consumer.compile(&rows, 900.0, 2.0, revision + 5);
        assert_eq!(
            semantic(&dense, &key).rect(),
            semantic(&physical, &key).rect()
        );
    }
    assert!(heights[1] > heights[0]);
}

#[test]
fn actual_child_baselines_align_and_narrow_layout_contains_controls_and_clips_full_field_text() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let value = "VeryLongUnbrokenValue".repeat(12);
    let rows = consumer.retain(&[("alpha", &value)], base);
    let key = consumer.key(&rows[0]);
    let button = consumer.state.get(&rows[0]).unwrap().button().key().clone();
    let (_, wide) = consumer.compile(&rows, 900.0, 1.25, 1);
    let parent = semantic(&wide, &key).parent().unwrap();
    let label = text_child(&wide, parent, "Value");
    let status = text_child(&wide, parent, "Ready");
    let content = text_child(&wide, &key, &value);
    let baseline = text_baseline(&wide, content.key());
    for slot in [label.key(), &button, status.key()] {
        close(text_baseline(&wide, slot), baseline);
    }
    let (_, narrow) = consumer.compile(&rows, 190.0, 1.25, 2);
    let field_rect = semantic(&narrow, &key).rect();
    let row_rect = semantic(&narrow, parent).rect();
    let action_rect = semantic(&narrow, &button).rect();
    assert!(field_rect.size().width() < semantic(&wide, &key).rect().size().width());
    assert!(
        field_rect.origin().y() >= semantic(&narrow, label.key()).rect().max().y() + base.gap()
    );
    assert!(action_rect.origin().y() >= field_rect.max().y() + base.gap());
    for node in narrow.semantics() {
        if node.key() == &key || node.key() == &button || node.parent() == Some(parent) {
            assert!(node.rect().origin().x() >= row_rect.origin().x());
            assert!(node.rect().max().x() <= row_rect.max().x() + 0.02);
            assert!(node.rect().max().y() <= row_rect.max().y() + 0.02);
        }
    }
    let clip = narrow
        .paint()
        .iter()
        .find_map(|paint| match paint {
            UiPaintPrimitive::Text {
                key: Some(painted),
                clip_rect: Some(clip),
                layout,
                ..
            } if painted == content.key() => {
                assert!(layout.width() > clip.size().width());
                Some(*clip)
            }
            _ => None,
        })
        .unwrap();
    assert!(clip.origin().x() >= field_rect.origin().x());
    assert!(clip.max().x() <= field_rect.max().x());
    assert_eq!(semantic(&narrow, &key).value(), Some(value.as_str()));
    assert_eq!(
        consumer
            .state
            .get(&rows[0])
            .unwrap()
            .field()
            .editor()
            .text(),
        value
    );
    assert!(consumer.compiler.work_stats().custom_child_measurements >= 8);
    assert_eq!(
        semantic(&narrow, &button).actions(),
        &[consumer
            .state
            .get(&rows[0])
            .unwrap()
            .button()
            .action()
            .clone()]
    );
    assert!(narrow.focus_order().iter().any(|focus| focus.key() == &key));
    assert!(
        narrow
            .focus_order()
            .iter()
            .any(|focus| focus.key() == &button)
    );
}

#[test]
fn long_cjk_commit_keeps_full_semantics_and_updated_padding_drives_point_and_ime_geometry() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let rows = consumer.retain(&[("alpha", "Initial")], base);
    let key = consumer.key(&rows[0]);
    let field = consumer.state.get_mut(&rows[0]).unwrap().field_mut();
    let token = field.focus().unwrap();
    let suffix = "中文路径 e\u{301}";
    let value = format!("{}{suffix}", "x".repeat(4_096 - suffix.len()));
    assert_eq!(value.len(), 4_096);
    field
        .edit(token, &TextEdit::SelectAll, &mut consumer.engine)
        .unwrap();
    field
        .edit(
            token,
            &TextEdit::Commit(value.clone()),
            &mut consumer.engine,
        )
        .unwrap();
    let enlarged = base
        .with_density(UiDensity::Compact)
        .with_text_scale(1.5)
        .unwrap();
    for (revision, appearance) in [base, enlarged].into_iter().enumerate() {
        consumer.appearance(&rows[0], appearance);
        let (_, frame) = consumer.compile(&rows, 225.0, 1.5, revision as u64 + 1);
        assert_eq!(semantic(&frame, &key).value(), Some(value.as_str()));
        let text = text_child(&frame, &key, &value);
        assert_eq!(text.name().len(), value.len());
        let viewport = semantic(&frame, &key).rect().size().width();
        let field = consumer.state.get_mut(&rows[0]).unwrap().field_mut();
        assert_eq!(field.editor().text(), value);
        field
            .edit(
                token,
                &TextEdit::Home { extend: false },
                &mut consumer.engine,
            )
            .unwrap();
        let metrics = appearance.control_metrics(UiControlSize::Regular).unwrap();
        let padding = metrics.padding();
        let ime = field.ime_area(viewport).unwrap();
        close(ime.origin().x(), padding.left());
        assert!(ime.origin().y() >= padding.top());
        assert!(ime.max().y() <= metrics.height() + 0.02);
        field
            .point(
                token,
                UiPoint::new(padding.left(), padding.top() + 1.0),
                viewport,
                false,
                &mut consumer.engine,
            )
            .unwrap();
        assert_eq!(field.editor().selection().focus, 0);
        field
            .edit(
                token,
                &TextEdit::End { extend: false },
                &mut consumer.engine,
            )
            .unwrap();
        let end = field.ime_area(viewport).unwrap();
        assert!(end.origin().x() >= padding.left());
        assert!(end.max().x() <= viewport - padding.right() + 1.0);
    }
    let field = consumer.state.get_mut(&rows[0]).unwrap().field_mut();
    field
        .edit(token, &TextEdit::Undo, &mut consumer.engine)
        .unwrap();
    assert_eq!(field.editor().text(), "Initial");
    field
        .edit(token, &TextEdit::Redo, &mut consumer.engine)
        .unwrap();
    assert_eq!(field.editor().text(), value);
}

#[test]
fn incompatible_style_adoption_and_impossible_width_preserve_the_retained_editor() {
    let mut consumer = Consumer::new();
    let base = UiAppearance::default();
    let rows = consumer.retain(&[("alpha", "Preserved")], base);
    let row = consumer.state.get_mut(&rows[0]).unwrap();
    let token = row.field_mut().focus().unwrap();
    let source = row.field().editor().paint_snapshot();
    let mut other = TextEngine::new(
        bundled_font_face().unwrap(),
        TextLimits {
            max_text_bytes: 128,
            ..TextLimits::default()
        },
    )
    .unwrap();
    assert!(
        row.set_appearance(base.with_text_scale(1.5).unwrap(), &mut other)
            .is_err()
    );
    assert_eq!(row.appearance(), base);
    assert!(source.same_source(&row.field().editor().paint_snapshot()));
    assert_eq!(row.field().input_token(), Some(token));
    for width in [0.0, 20.0, f32::NAN, f32::INFINITY] {
        assert!(
            row.authored_node(width, "Ready", &mut consumer.engine)
                .is_err()
        );
        assert_eq!(row.field().editor().text(), "Preserved");
        assert_eq!(row.field().input_token(), Some(token));
    }
}
