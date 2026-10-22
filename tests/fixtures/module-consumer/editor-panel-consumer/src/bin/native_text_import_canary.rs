use std::path::PathBuf;

use nara::project_host::EditorSourceAssetDomain;

use nara_editor::{
    EditorLaunchOptions, EditorNativeComposition, ImguiEditorPanel, ImguiEditorPanelContext,
    PanelActionDispatch, PanelSubmissionId,
    workspace::{EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement},
};
use nara_editor_panel_module_consumer::text_import;
use nara_tooling::{
    EditorAction, EditorActionOutcome, EditorSourceAssetImportActionOutcome,
    EditorSourceAssetImportAttempt, EditorSourceAssetMetadataView, EditorWorkspaceIntent,
    EditorWorkspaceIntentAction,
};

#[derive(Default)]
struct Canary {
    stage: u8,
    stable_id: Option<nara::asset::StableAssetId>,
}

impl ImguiEditorPanel for Canary {
    fn show(&mut self, ui: &dear_imgui_rs::Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
        ui.text("External text import qualification");
        let frame = context.model().frame();
        if self.stage == 0 {
            if frame
                .project_browser()
                .entry("assets/caption.txt")
                .is_none()
                || frame.image_import_work_pending()
            {
                return;
            }
            self.stage = 1;
            eprintln!("native text canary submitting Import");
            context.submit(import(64)).unwrap();
            return;
        }
        if let Some(view) = frame.source_asset_import() {
            if let EditorSourceAssetImportAttempt::Failed { diagnostics } = &view.attempt {
                panic!("native source import failed: {diagnostics:?}");
            }
            if let EditorSourceAssetImportAttempt::MetadataPublished { stable_id } = view.attempt {
                if self.stage == 2 {
                    self.stable_id = Some(stable_id);
                    if !matches!(view.metadata, EditorSourceAssetMetadataView::Ready { .. }) {
                        return;
                    }
                    self.stage = 3;
                    eprintln!("native text canary submitting Reimport");
                    context.submit(import(128)).unwrap();
                } else if self.stage == 4 {
                    assert_eq!(self.stable_id, Some(stable_id));
                    let EditorSourceAssetMetadataView::Ready {
                        settings: Some(settings),
                        ..
                    } = &view.metadata
                    else {
                        return;
                    };
                    let settings: nara_asset_domain_consumer::TextSettings =
                        serde_json::from_str(settings).unwrap();
                    if settings.max_characters != 128 || frame.source_asset_import_work_pending() {
                        return;
                    }
                    self.stage = 5;
                    println!("NARA_NATIVE_TEXT_IMPORT_OK {stable_id}");
                    context
                        .submit(EditorAction::WorkspaceIntent(
                            EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::Exit),
                        ))
                        .unwrap();
                }
            }
        }
    }

    fn action_finished(&mut self, _: PanelSubmissionId, result: PanelActionDispatch) {
        if self.stage == 1 || self.stage == 3 {
            assert!(
                matches!(
                    result,
                    PanelActionDispatch::Dispatched(EditorActionOutcome::SourceAssetImport(
                        EditorSourceAssetImportActionOutcome::Started
                    ))
                ),
                "{result:?}"
            );
            self.stage += 1;
        }
    }
}

fn import(maximum: usize) -> EditorAction {
    EditorAction::SourceAssetImport(
        text_import::TextImportDomain::import_action(
            "assets/caption.txt",
            &nara_asset_domain_consumer::TextSettings {
                max_characters: maximum,
            },
        )
        .unwrap(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("expected project directory")?,
    );
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition)?;
    text_import::install_panel(&mut composition)?;
    composition.register_panel(
        EditorPanelDescriptor::new(
            "consumer.text-canary",
            "Text qualification",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Floating,
            true,
        ),
        || {
            eprintln!("public native runner created the text import canary");
            Canary::default()
        },
    )?;
    eprintln!("starting the public native text import runner");
    composition.run(EditorLaunchOptions::new(root))?;
    Ok(())
}
