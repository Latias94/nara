//! CPU journal integration. Test authority supplies receipts; this is not native presentation.
use std::sync::Arc;

use nara_text::{TextEdit, TextEngine, bundled_font_face};
use nara_ui_kernel::{
    UiAuthoringSnapshotBuilder, UiCompiler, UiEnvironment, UiFrame, UiInputEvent,
    UiInteractionSession, UiLayer, UiOpaqueId, UiRoot, UiRootKey, UiRouteStamp, UiSeatId, UiStyle,
    UiSurfaceId, UiTheme,
};
use nara_ui_widgets::{UiStateBinding, UiStateHandle, UiStateStore, UiTextField};

fn binding() -> UiStateBinding {
    UiStateBinding::new(UiOpaqueId::new(b"document").unwrap(), 1)
}

fn compile(
    store: &UiStateStore,
    fields: &[UiStateHandle<UiTextField>],
    revision: u64,
) -> Arc<UiFrame> {
    let root_key = store.root().root_key().clone();
    let mut root = UiRoot::new(root_key.clone(), UiLayer::Base, UiStyle::fill());
    for (index, field) in fields.iter().enumerate() {
        root = root.with_child(
            store
                .get(field)
                .unwrap()
                .authored_node(
                    &root_key,
                    UiStyle::absolute(8.0, 8.0 + index as f32 * 48.0, 180.0, 40.0),
                    180.0,
                )
                .unwrap(),
        );
    }
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(
            &UiAuthoringSnapshotBuilder::new(revision)
                .with_root(root)
                .build()
                .unwrap(),
            UiEnvironment::new(
                UiSurfaceId::new(67).unwrap(),
                220.0,
                140.0,
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
}

#[test]
fn routed_focus_reduction_cancels_preedit_and_rejects_old_dispatch_and_scoped_input_after_replacement()
 {
    let mut text = TextEngine::new(bundled_font_face().unwrap(), Default::default()).unwrap();
    let root = UiRootKey::runtime("state-text-test", "panel").unwrap();
    let mut store = UiStateStore::new(root.clone(), 2).unwrap();
    let keys = [
        store.root().child("first").unwrap(),
        store.root().child("second").unwrap(),
    ];
    let mut pass = store.begin();
    let mut fields = Vec::new();
    for (key, value) in keys.iter().zip(["first", "second"]) {
        fields.push(
            pass.try_state::<_, nara_ui_widgets::UiStateError>(key.clone(), binding(), || {
                Ok(UiTextField::new(
                    root.clone(),
                    "Value",
                    value,
                    18.0,
                    &mut text,
                )?)
            })
            .unwrap(),
        );
    }
    pass.finish().unwrap();
    let surface = UiSurfaceId::new(67).unwrap();
    let (mut journal, authority) =
        nara_ui_kernel::__private::new_hosted_frame_journal(Default::default()).unwrap();
    journal.install_surface(surface, 1).unwrap();
    let receipt = authority
        .submit(&mut journal, compile(&store, &fields, 1), 1)
        .unwrap();
    let mut interaction = UiInteractionSession::default();
    let focused = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: store.get(&fields[0]).unwrap().key().clone(),
            },
        )
        .unwrap();
    assert!(
        store
            .reduce_text_focus(focused.consumed().unwrap(), &mut text)
            .unwrap()
    );
    let first = store.text_input(&fields[0]).unwrap().unwrap();
    store
        .edit_text(&first, &TextEdit::SelectAll, &mut text)
        .unwrap();
    store
        .edit_text(
            &first,
            &TextEdit::Preedit {
                text: "pending".into(),
                cursor: Some((0, 7)),
            },
            &mut text,
        )
        .unwrap();
    let next_focus = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: store.get(&fields[1]).unwrap().key().clone(),
            },
        )
        .unwrap();
    store
        .reduce_text_focus(next_focus.consumed().unwrap(), &mut text)
        .unwrap();
    assert_eq!(
        store.get(&fields[0]).unwrap().editor().display_text(),
        "first"
    );
    assert!(
        store
            .edit_text(&first, &TextEdit::Commit("late".into()), &mut text)
            .is_err()
    );
    assert!(store.text_input(&fields[1]).unwrap().is_some());
    let refocus = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: store.get(&fields[0]).unwrap().key().clone(),
            },
        )
        .unwrap();
    store
        .reduce_text_focus(refocus.consumed().unwrap(), &mut text)
        .unwrap();
    let renewed = store.text_input(&fields[0]).unwrap().unwrap();
    assert_ne!(first, renewed);
    assert!(
        store
            .edit_text(&first, &TextEdit::Insert("late".into()), &mut text)
            .is_err()
    );
    let mut pass = store.begin();
    let replacement = pass
        .replace::<_, nara_ui_widgets::UiStateError>(keys[0].clone(), binding(), || {
            Ok(UiTextField::new(
                root.clone(),
                "Value",
                "replacement",
                18.0,
                &mut text,
            )?)
        })
        .unwrap();
    pass.try_state::<UiTextField, nara_ui_widgets::UiStateError>(
        keys[1].clone(),
        binding(),
        || panic!("reuse"),
    )
    .unwrap();
    pass.finish().unwrap();
    fields[0] = replacement;
    assert!(
        !store
            .reduce_text_focus(focused.consumed().unwrap(), &mut text)
            .unwrap()
    );
    assert!(store.text_input(&fields[0]).unwrap().is_none());
    assert!(
        store
            .edit_text(&renewed, &TextEdit::Commit("late".into()), &mut text)
            .is_err()
    );
    let receipt = authority
        .submit(&mut journal, compile(&store, &fields, 2), 2)
        .unwrap();
    if let Some(batch) = interaction
        .reconcile_latest_presentation(&journal, surface, &[])
        .unwrap()
    {
        store.reduce_text_focus(&batch, &mut text).unwrap();
    }
    let focus_new = interaction
        .dispatch(
            &journal,
            UiRouteStamp::from_receipt(receipt),
            UiInputEvent::FocusTarget {
                seat: UiSeatId::PRIMARY,
                target: store.get(&fields[0]).unwrap().key().clone(),
            },
        )
        .unwrap();
    store
        .reduce_text_focus(focus_new.consumed().unwrap(), &mut text)
        .unwrap();
    let current = store.text_input(&fields[0]).unwrap().unwrap();
    store
        .edit_text(&current, &TextEdit::SelectAll, &mut text)
        .unwrap();
    store
        .edit_text(&current, &TextEdit::Commit("accepted".into()), &mut text)
        .unwrap();
    assert_eq!(store.get(&fields[0]).unwrap().editor().text(), "accepted");
    assert_eq!(store.get(&fields[1]).unwrap().editor().text(), "second");
}
