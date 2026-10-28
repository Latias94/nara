//! The Editor facet reuses the same text preparation as the game's source domain.

use nara::project_host::EditorSourceAssetDomain;
use nara_asset::SourceAssetImporter;
use nara_asset_domain_consumer::{TextDomain, TextSettings};

pub struct TextImportDomain;

impl EditorSourceAssetDomain for TextImportDomain {
    type Domain = TextDomain;

    fn default_settings() -> TextSettings {
        TextSettings { max_characters: 64 }
    }
}

pub fn install(
    composition: &mut nara_editor::EditorNativeComposition,
) -> Result<(), nara::ProductRecipeError> {
    composition.register_source_asset_domain::<TextImportDomain>()
}

pub fn install_panel(
    composition: &mut nara_editor::EditorNativeComposition,
) -> Result<(), nara_editor::workspace::EditorPanelRegistrationError> {
    use nara_editor::workspace::{
        EditorPanelDescriptor, EditorPanelMultiplicity, EditorPanelPlacement,
    };
    composition.register_panel(
        EditorPanelDescriptor::new(
            "consumer.text-import",
            "Text Import",
            EditorPanelMultiplicity::Single,
            EditorPanelPlacement::Right,
            true,
        ),
        TextImportPanel::default,
    )
}

#[derive(Default)]
struct TextImportPanel {
    basis: Option<(String, Option<String>)>,
    maximum: i32,
    dispatch: String,
}

impl nara_editor::ImguiEditorPanel for TextImportPanel {
    fn show(
        &mut self,
        ui: &dear_imgui_rs::Ui,
        context: &mut nara_editor::ImguiEditorPanelContext<'_, '_>,
    ) {
        use nara_tooling::{
            EditorAction, EditorSourceAssetImportAttempt, EditorSourceAssetMetadataView,
        };
        let model = context.model();
        let Some(view) = model.frame().source_asset_import() else {
            ui.text_disabled(if model.frame().image_import_work_pending() {
                "Refreshing source metadata"
            } else {
                "No source selected"
            });
            return;
        };
        let (seed, reimport) = match &view.metadata {
            EditorSourceAssetMetadataView::Missing => (
                view.domains
                    .iter()
                    .find(|domain| domain.source_kind == TextDomain::SOURCE_KIND)
                    .map(|domain| domain.default_settings.as_ref()),
                false,
            ),
            EditorSourceAssetMetadataView::Ready {
                source_kind,
                settings,
                ..
            } if source_kind.as_ref() == TextDomain::SOURCE_KIND => (settings.as_deref(), true),
            _ => {
                ui.text_disabled("Text metadata unavailable");
                return;
            }
        };
        let basis = (view.relative_path.to_string(), seed.map(str::to_owned));
        if self.basis.as_ref() != Some(&basis) {
            if let Some(settings) =
                seed.and_then(|seed| serde_json::from_str::<TextSettings>(seed).ok())
            {
                self.maximum = i32::try_from(settings.max_characters).unwrap_or(i32::MAX);
            }
            self.basis = Some(basis);
        }
        ui.text_wrapped(view.relative_path.as_ref());
        ui.input_int("Maximum characters", &mut self.maximum);
        let disabled = ui.begin_disabled_with_cond(
            model.frame().source_asset_import_work_pending() || self.maximum < 0 || seed.is_none(),
        );
        let action = ui
            .button(if reimport { "Reimport" } else { "Import" })
            .then(|| {
                TextImportDomain::import_action(
                    view.relative_path.clone(),
                    &TextSettings {
                        max_characters: self.maximum as usize,
                    },
                )
                .map(EditorAction::SourceAssetImport)
            });
        drop(disabled);
        match &view.attempt {
            EditorSourceAssetImportAttempt::Idle => {}
            EditorSourceAssetImportAttempt::Preparing => ui.text("Validating source"),
            EditorSourceAssetImportAttempt::MetadataPublished { .. } => {
                ui.text("Metadata published")
            }
            EditorSourceAssetImportAttempt::ReconcileRequired => {
                ui.text_wrapped("Refresh source metadata before importing again.")
            }
            EditorSourceAssetImportAttempt::Failed { diagnostics } => {
                for diagnostic in diagnostics.iter() {
                    ui.text_wrapped(diagnostic.summary().as_str());
                }
            }
        }
        ui.text_wrapped(&self.dispatch);
        if let Some(action) = action {
            self.dispatch = match action {
                Ok(action) => match context.submit(action) {
                    Ok(_) => "Import queued".to_owned(),
                    Err(error) => error.to_string(),
                },
                Err(_) => "Invalid text import settings".to_owned(),
            };
        }
    }

    fn action_finished(
        &mut self,
        _: nara_editor::PanelSubmissionId,
        result: nara_editor::PanelActionDispatch,
    ) {
        self.dispatch = format!("{result:?}");
    }
}
