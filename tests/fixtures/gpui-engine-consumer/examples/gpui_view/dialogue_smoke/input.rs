use super::*;
use open_gpui::{Pixels, Point};
use open_gpui_ui_components::gpui_adapter::{OverlayLayerPhase, WindowOverlayRuntime};
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, MouseButton as NativeButton},
    keyboard::Key,
};

pub(super) fn popup_settled(viewer: &Viewer, index: usize, expected: usize) -> Result<bool> {
    viewer.application.as_ref().unwrap().update(|cx| {
        viewer.root.unwrap().update(cx, |_, window, cx| {
            let snapshot = WindowOverlayRuntime::for_window(window, cx).snapshot(window, cx)?;
            let active = snapshot
                .layers()
                .iter()
                .filter(|layer| {
                    matches!(
                        layer.phase(),
                        OverlayLayerPhase::Open
                            | OverlayLayerPhase::CloseRequested
                            | OverlayLayerPhase::Closing
                    )
                })
                .count();
            ensure!(
                active <= 1,
                "the Dialogue selector left multiple Window overlays open: {active}"
            );
            let editor = viewer.dialogue_panel.as_ref().unwrap().read(cx).editors[index].read(cx);
            Ok(active == expected && editor.selector_open == (expected > 0))
        })
    })?
}

pub(super) fn replace(viewer: &Viewer, index: usize, field: Hit, text: &str) -> Result<()> {
    let length = viewer.application.as_ref().unwrap().update(|cx| {
        let panel = viewer.dialogue_panel.as_ref().unwrap().read(cx);
        if field == Hit::ResourceQuery {
            return panel.resources[index]
                .read(cx)
                .view()
                .path_filter()
                .encode_utf16()
                .count();
        }
        let editor = match field {
            Hit::NativeText(_) | Hit::NativeSpeaker(_) => &panel.node_editors[index],
            _ => &panel.editors[index],
        }
        .read(cx);
        let draft = editor.draft.value();
        match field {
            Hit::X => &draft.position[0],
            Hit::Y => &draft.position[1],
            Hit::Speaker | Hit::NativeSpeaker(_) => &draft.speaker,
            Hit::ChoiceLabel(index) => &draft.choices[index].label,
            _ => &draft.text,
        }
        .encode_utf16()
        .count()
    });
    viewer
        .surface
        .as_ref()
        .unwrap()
        .with_input_handler(|handler| handler.replace_text_in_range(Some(0..length), text))?
        .context("Dialogue field has no accepted focused text handler")?;
    Ok(())
}

pub(super) fn assert_textarea(
    viewer: &Viewer,
    index: usize,
    expected: &str,
    caret: usize,
) -> Result<()> {
    viewer.application.as_ref().unwrap().update(|cx| {
        let editor = viewer.dialogue_panel.as_ref().unwrap().read(cx).editors[index].read(cx);
        ensure!(
            editor.draft.value().text == expected,
            "ordinary keyboard input produced {:?}; expected {expected:?}",
            editor.draft.value().text
        );
        Ok::<_, anyhow::Error>(())
    })?;
    let selection = viewer
        .surface
        .as_ref()
        .unwrap()
        .with_input_handler(|handler| handler.selected_text_range(false))?
        .flatten()
        .context("the focused Dialogue textarea has no native text selection")?;
    ensure!(
        selection.range == (caret..caret) && !selection.reversed,
        "ordinary keyboard input left caret {:?}; expected {caret}",
        selection
    );
    Ok(())
}

pub(super) fn type_text(viewer: &Viewer, text: &str) -> Result<()> {
    for character in text.chars() {
        let value = character.to_string();
        let key = Key::Character(value.clone().into());
        viewer.keyboard_input(key.clone(), Some(value), ElementState::Pressed, false)?;
        viewer.keyboard_input(key, None, ElementState::Released, false)?;
    }
    Ok(())
}

pub(super) fn key(viewer: &mut Viewer, key: NamedKey) -> Result<()> {
    viewer.keyboard_input(Key::Named(key), None, ElementState::Pressed, false)?;
    viewer.keyboard_input(Key::Named(key), None, ElementState::Released, false)
}

pub(super) fn click(viewer: &mut Viewer, frame: &AcceptedFrame, hit: Hit) -> Result<()> {
    let at = frame.center(hit)?;
    mouse(viewer, at, ElementState::Pressed)?;
    mouse(viewer, at, ElementState::Released)
}

pub(super) fn mouse(viewer: &mut Viewer, at: Point<Pixels>, state: ElementState) -> Result<()> {
    let scale = f64::from(viewer.surface.as_ref().unwrap().geometry().scale_factor());
    viewer.input(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(
            f64::from(f32::from(at.x)) * scale,
            f64::from(f32::from(at.y)) * scale,
        ),
    })?;
    viewer.input(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state,
        button: NativeButton::Left,
    })
}
