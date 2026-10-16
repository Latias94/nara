use egui::{Color32, RichText, Ui};
use nara_tooling::{
    EditorAction, EditorExternalPlayCompilerLevel, EditorExternalPlayMessageKind,
    EditorExternalPlayOutput, EditorExternalPlayState, EditorExternalPlayView, EditorFrameModel,
    EditorPlayAction, EditorRuntimeUnavailableReason, EditorSceneModel,
};

use super::toolbar_button;

pub(super) fn render_external_play_controls(
    ui: &mut Ui,
    scene: Option<&EditorSceneModel>,
    frame: &EditorFrameModel,
    content_ready: bool,
    can_request: bool,
    play_pending: bool,
    actions: &mut Vec<EditorAction>,
) {
    let default_external = EditorExternalPlayView::new(
        EditorExternalPlayState::Idle,
        None,
        false,
        EditorExternalPlayOutput::default(),
    );
    let external = frame.play().external().unwrap_or(&default_external);
    ui.label(RichText::new(external_play_state_label(external.state())).weak());
    let mut push = |enabled: bool,
                    allow_while_pending: bool,
                    label: &'static str,
                    action: EditorPlayAction| {
        if toolbar_button(
            ui,
            enabled && can_request && (allow_while_pending || !play_pending),
            label,
        ) {
            actions.push(EditorAction::Play(action));
        }
    };
    let play_action = |scene: Option<&EditorSceneModel>| {
        scene.map(|scene| EditorPlayAction::Play {
            document: scene.document,
            base_revision: scene.revision,
        })
    };

    match external.state() {
        EditorExternalPlayState::Idle => {
            if let Some(action) = play_action(scene) {
                push(content_ready, false, "Play", action);
            } else {
                let _ = toolbar_button(ui, false, "Play");
            }
        }
        EditorExternalPlayState::Building => {
            push(true, true, "Cancel", EditorPlayAction::Cancel);
        }
        EditorExternalPlayState::Starting => {
            push(true, true, "Stop", EditorPlayAction::Stop);
        }
        EditorExternalPlayState::Running => {
            push(true, false, "Stop", EditorPlayAction::Stop);
            push(true, false, "Restart", EditorPlayAction::Restart);
        }
        EditorExternalPlayState::Stopping => {}
        EditorExternalPlayState::Faulted => {
            push(true, false, "Stop", EditorPlayAction::Stop);
            if let Some(action) = play_action(scene) {
                push(content_ready, false, "Retry", action);
            }
            push(
                content_ready && external.last_good_available(),
                false,
                "Run Last Good",
                EditorPlayAction::RunLastGood,
            );
        }
        EditorExternalPlayState::CleanupIncomplete => {
            if frame.play().state() == nara_tooling::EditorPlayState::CloseIncomplete {
                push(true, false, "Retry close", EditorPlayAction::RetryClose);
            } else {
                push(
                    true,
                    false,
                    "Retry retirement",
                    EditorPlayAction::RetryRetirement,
                );
            }
        }
    }

    if let Some(generation) = external.generation() {
        ui.label(RichText::new(format!("External generation {generation}")).weak());
    }
    if external.output_truncated() {
        ui.label(RichText::new("External output truncated").weak());
    }
    let diagnostics = external.diagnostics();
    if !diagnostics.is_empty() {
        ui.label(
            RichText::new(format!(
                "Build diagnostics: {} errors, {} warnings, {} retained",
                diagnostics.error_count(),
                diagnostics.warning_count(),
                diagnostics.retained_count(),
            ))
            .weak(),
        );
        if diagnostics.dropped_count() > 0 || diagnostics.truncated_count() > 0 {
            ui.label(RichText::new("Build diagnostics bounded").weak());
        }
    }
}

pub(super) fn render_external_play_output(
    ui: &mut Ui,
    frame: &EditorFrameModel,
    rendered: &mut usize,
) {
    let Some(external) = frame.play().external() else {
        return;
    };
    let output = external.output();
    for message in output.messages() {
        *rendered = rendered.saturating_add(1);
        let (owner, color) = match message.kind() {
            EditorExternalPlayMessageKind::Compiler(level) => (
                "Compiler",
                match level {
                    EditorExternalPlayCompilerLevel::Error => Color32::RED,
                    EditorExternalPlayCompilerLevel::Warning => Color32::YELLOW,
                    EditorExternalPlayCompilerLevel::Note
                    | EditorExternalPlayCompilerLevel::Help
                    | EditorExternalPlayCompilerLevel::FailureNote
                    | EditorExternalPlayCompilerLevel::Other => Color32::GRAY,
                },
            ),
            EditorExternalPlayMessageKind::ProductStdout => ("Product stdout", Color32::GRAY),
            EditorExternalPlayMessageKind::ProductStderr => ("Product stderr", Color32::YELLOW),
            EditorExternalPlayMessageKind::OutputNotice => ("Supervisor", Color32::GRAY),
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(owner).strong());
            if let Some(code) = message.code() {
                ui.label(RichText::new(code).color(color));
            }
            ui.label(RichText::new(message.text()).color(color));
        });
    }
    if output.is_truncated() {
        *rendered = rendered.saturating_add(1);
        ui.label(
            RichText::new(format!(
                "External output bounded ({} dropped messages, {} truncated messages)",
                output.dropped_messages(),
                output.truncated_messages(),
            ))
            .weak(),
        );
    }
}

pub(super) fn render_external_play_status(ui: &mut Ui, frame: &EditorFrameModel) {
    let message = frame
        .play()
        .external()
        .map(|external| external_play_state_label(external.state()))
        .unwrap_or("External supervised Play is ready");
    ui.label(RichText::new(message).weak());
    ui.label(
        RichText::new("Authoring remains available while the product runs in its external window")
            .weak(),
    );
    if frame.play().is_out_of_date() {
        ui.label(
            RichText::new(
                "Saved source changed; Restart keeps the admitted generation, while Stop then Play builds saved changes",
            )
            .weak(),
        );
    }
}

pub(super) fn render_runtime_unavailable(ui: &mut Ui, reason: EditorRuntimeUnavailableReason) {
    ui.label(RichText::new(runtime_unavailable_label(reason)).weak());
}

fn external_play_state_label(state: EditorExternalPlayState) -> &'static str {
    match state {
        EditorExternalPlayState::Idle => "Edit",
        EditorExternalPlayState::Building => "Building",
        EditorExternalPlayState::Starting => "Starting external window",
        EditorExternalPlayState::Running => "Running in external window",
        EditorExternalPlayState::Stopping => "Stopping external window",
        EditorExternalPlayState::Faulted => "External Play faulted",
        EditorExternalPlayState::CleanupIncomplete => "External cleanup incomplete",
    }
}

fn runtime_unavailable_label(reason: EditorRuntimeUnavailableReason) -> &'static str {
    match reason {
        EditorRuntimeUnavailableReason::SupervisedPlayContractMissing => concat!(
            "Play unavailable: add this to nara.toml:\n",
            "[profiles.<selected-profile>.execution]\n",
            "contract = \"desktop-supervised-play/v1\"",
        ),
        EditorRuntimeUnavailableReason::SupervisedPlayContractUnsupported => {
            "Play unavailable: the project Play contract is unsupported"
        }
        EditorRuntimeUnavailableReason::ProjectIdentityMissing => {
            "Play unavailable: the project is missing a stable identity"
        }
        EditorRuntimeUnavailableReason::SupervisedPlayPlatformUnsupported => {
            "Play unavailable: this platform cannot supervise the desktop product"
        }
        EditorRuntimeUnavailableReason::SupervisedPlayUnavailable => {
            "Play unavailable: supervised desktop execution is unavailable"
        }
    }
}

#[cfg(test)]
mod tests {
    use nara_tooling::EditorRuntimeUnavailableReason;

    use super::runtime_unavailable_label;

    #[test]
    fn missing_contract_guidance_names_the_exact_manifest_entry() {
        assert_eq!(
            runtime_unavailable_label(
                EditorRuntimeUnavailableReason::SupervisedPlayContractMissing,
            ),
            concat!(
                "Play unavailable: add this to nara.toml:\n",
                "[profiles.<selected-profile>.execution]\n",
                "contract = \"desktop-supervised-play/v1\"",
            )
        );
    }
}
