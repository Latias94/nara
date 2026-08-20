use std::sync::Arc;

use egui::{Button, RichText, Ui};
use nara_tooling::{
    EditorAction, EditorImageColorSpace, EditorImageImportAttemptPhase,
    EditorImageImportAttemptView, EditorImageImportAvailabilityView,
    EditorImageImportDetailRevision, EditorImageImportFailureReason, EditorImageImportFreshness,
    EditorImageImportMetadataIssue, EditorImageImportMetadataView, EditorImageImportSettings,
    EditorImageImportView,
};

#[cfg(test)]
use std::{cell::RefCell, collections::HashMap};

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<HashMap<&'static str, egui::Rect>> =
        RefCell::new(HashMap::new());
    static TEST_CONTROL_ENABLED: RefCell<HashMap<&'static str, bool>> =
        RefCell::new(HashMap::new());
    static TEST_STATUS_LINES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiImageImportDraftKey {
    relative_path: Arc<str>,
    detail_revision: EditorImageImportDetailRevision,
}

impl EguiImageImportDraftKey {
    fn from_view(view: &EditorImageImportView) -> Self {
        Self {
            relative_path: Arc::from(view.relative_path()),
            detail_revision: view.detail_revision(),
        }
    }

    fn matches(&self, view: &EditorImageImportView) -> bool {
        self.relative_path.as_ref() == view.relative_path()
            && self.detail_revision == view.detail_revision()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiImageImportDraft {
    key: EguiImageImportDraftKey,
    settings: EditorImageImportSettings,
}

/// Private egui Adapter for one Host-projected PNG import detail.
///
/// The panel retains exactly one uncommitted settings draft. The selected relative path and detail
/// revision form its complete identity; every other fact is rendered directly from the immutable
/// Host frame.
#[derive(Debug, Default)]
pub(crate) struct EguiImageImportPanel {
    draft: Option<EguiImageImportDraft>,
}

impl EguiImageImportPanel {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn clear(&mut self) {
        self.draft = None;
        #[cfg(test)]
        clear_test_observations();
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        view: &EditorImageImportView,
        host_actions_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        #[cfg(test)]
        clear_test_observations();

        self.synchronize(view);

        ui.heading("Image Import");
        ui.label(view.relative_path());
        render_status(ui, metadata_status_label(view.metadata()));
        render_committed_settings(ui, view.metadata());
        render_availability(ui, view.availability());
        render_status(ui, attempt_status_label(view.attempt()));

        let Some(draft) = self.draft.as_mut() else {
            ui.separator();
            ui.label(
                RichText::new("Import settings are unavailable until metadata is repaired").weak(),
            );
            return;
        };

        ui.separator();
        ui.label("Draft color space");
        let mut color_space = draft.settings.color_space();
        ui.horizontal(|ui| {
            let _srgb = ui.selectable_value(&mut color_space, EditorImageColorSpace::Srgb, "sRGB");
            #[cfg(test)]
            record_test_control("sRGB", &_srgb);
            let _linear =
                ui.selectable_value(&mut color_space, EditorImageColorSpace::Linear, "Linear");
            #[cfg(test)]
            record_test_control("Linear", &_linear);
        });
        if color_space != draft.settings.color_space() {
            draft.settings = EditorImageImportSettings::new(color_space);
        }

        let seed = view.draft_seed();
        ui.horizontal(|ui| {
            let revert = ui.add_enabled(
                seed.is_some_and(|seed| seed != draft.settings),
                Button::new("Revert"),
            );
            #[cfg(test)]
            record_test_control("Revert", &revert);
            if revert.clicked() {
                if let Some(seed) = seed {
                    draft.settings = seed;
                }
            }

            let command_label = if view.metadata().is_reimport() {
                "Reimport"
            } else {
                "Import"
            };
            let submit = ui.add_enabled(
                host_actions_enabled && view.can_submit(),
                Button::new(command_label),
            );
            #[cfg(test)]
            record_test_control(command_label, &submit);
            if submit.clicked() {
                actions.push(EditorAction::ImageImport(
                    view.submit_action(draft.settings),
                ));
            }
        });
    }

    fn synchronize(&mut self, view: &EditorImageImportView) {
        if self
            .draft
            .as_ref()
            .is_some_and(|draft| draft.key.matches(view))
        {
            return;
        }

        self.draft = view.draft_seed().map(|settings| EguiImageImportDraft {
            key: EguiImageImportDraftKey::from_view(view),
            settings,
        });
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn draft_settings(&self) -> Option<EditorImageImportSettings> {
        self.draft.as_ref().map(|draft| draft.settings)
    }
}

fn render_committed_settings(ui: &mut Ui, metadata: EditorImageImportMetadataView) {
    let status = match metadata {
        EditorImageImportMetadataView::Missing => "Committed settings: None",
        EditorImageImportMetadataView::Ready {
            committed_settings, ..
        } => match committed_settings.color_space() {
            EditorImageColorSpace::Srgb => "Committed color space: sRGB",
            EditorImageColorSpace::Linear => "Committed color space: Linear",
        },
        EditorImageImportMetadataView::Invalid { .. }
        | EditorImageImportMetadataView::IdentityConflict { .. } => {
            "Committed settings: Unavailable"
        }
    };
    render_status(ui, status);
}

fn render_availability(ui: &mut Ui, availability: EditorImageImportAvailabilityView) {
    match availability {
        EditorImageImportAvailabilityView::Unavailable => {
            render_status(ui, "Last-good image: Unavailable");
        }
        EditorImageImportAvailabilityView::LastGood {
            active_settings,
            dimensions,
            freshness,
            ..
        } => {
            render_status(ui, freshness_status_label(freshness));
            render_status(ui, active_settings_status_label(active_settings));
            render_status(
                ui,
                format!(
                    "Dimensions: {} × {}",
                    dimensions.width(),
                    dimensions.height()
                ),
            );
        }
    }
}

pub(crate) fn active_settings_status_label(settings: EditorImageImportSettings) -> &'static str {
    match settings.color_space() {
        EditorImageColorSpace::Srgb => "Active color space: sRGB",
        EditorImageColorSpace::Linear => "Active color space: Linear",
    }
}

fn metadata_status_label(metadata: EditorImageImportMetadataView) -> &'static str {
    match metadata {
        EditorImageImportMetadataView::Missing => "Metadata: Missing",
        EditorImageImportMetadataView::Ready { .. } => "Metadata: Ready",
        EditorImageImportMetadataView::Invalid { issue } => match issue {
            EditorImageImportMetadataIssue::SourceUnavailable => "Metadata: Source unavailable",
            EditorImageImportMetadataIssue::SourceChanged => "Metadata: Source changed",
            EditorImageImportMetadataIssue::MetadataUnavailable => "Metadata: Sidecar unavailable",
            EditorImageImportMetadataIssue::MetadataChanged => "Metadata: Sidecar changed",
            EditorImageImportMetadataIssue::InvalidFormat => "Metadata: Invalid format",
            EditorImageImportMetadataIssue::PathMismatch => "Metadata: Path mismatch",
            EditorImageImportMetadataIssue::SourceKindMismatch => "Metadata: Source kind mismatch",
            EditorImageImportMetadataIssue::InvalidSettings => "Metadata: Invalid settings",
        },
        EditorImageImportMetadataView::IdentityConflict { .. } => "Metadata: Identity conflict",
    }
}

pub(crate) fn freshness_status_label(freshness: EditorImageImportFreshness) -> &'static str {
    match freshness {
        EditorImageImportFreshness::Fresh => "Last-good image: Current",
        EditorImageImportFreshness::SourceChanged => "Last-good image: Available; source changed",
        EditorImageImportFreshness::SettingsChanged => {
            "Last-good image: Available; settings changed"
        }
        EditorImageImportFreshness::DesiredStateUnavailable => {
            "Last-good image: Available; desired settings unavailable"
        }
    }
}

fn attempt_status_label(attempt: EditorImageImportAttemptView) -> &'static str {
    match attempt {
        EditorImageImportAttemptView::Idle => "Attempt: Idle",
        EditorImageImportAttemptView::Pending { phase, .. } => match phase {
            EditorImageImportAttemptPhase::DecodeAndValidate => "Attempt: Decoding and validating",
        },
        EditorImageImportAttemptView::Succeeded { .. } => "Attempt: Succeeded",
        EditorImageImportAttemptView::Failed { reason, .. } => match reason {
            EditorImageImportFailureReason::CatalogUnavailable => {
                "Attempt failed: Catalog unavailable"
            }
            EditorImageImportFailureReason::MetadataInvalid => "Attempt failed: Metadata invalid",
            EditorImageImportFailureReason::IdentityUnavailable => {
                "Attempt failed: Identity unavailable"
            }
            EditorImageImportFailureReason::CapacityExceeded => "Attempt failed: Capacity exceeded",
            EditorImageImportFailureReason::SourceUnavailable => {
                "Attempt failed: Source unavailable"
            }
            EditorImageImportFailureReason::MetadataPublication => {
                "Attempt failed: Metadata publication"
            }
            EditorImageImportFailureReason::ImageProcessing => "Attempt failed: Image processing",
        },
        EditorImageImportAttemptView::Stale { .. } => "Attempt: Stale",
        EditorImageImportAttemptView::Cancelled { .. } => "Attempt: Cancelled",
        EditorImageImportAttemptView::ReconcileRequired { .. } => {
            "Attempt: Refresh required before retry"
        }
    }
}

fn render_status(ui: &mut Ui, status: impl Into<String>) {
    let status = status.into();
    #[cfg(test)]
    TEST_STATUS_LINES.with(|lines| lines.borrow_mut().push(status.clone()));
    ui.label(RichText::new(status).weak());
}

#[cfg(test)]
fn clear_test_observations() {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
    TEST_CONTROL_ENABLED.with(|enabled| enabled.borrow_mut().clear());
    TEST_STATUS_LINES.with(|lines| lines.borrow_mut().clear());
}

#[cfg(test)]
fn record_test_control(label: &'static str, response: &egui::Response) {
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(label, response.rect);
    });
    TEST_CONTROL_ENABLED.with(|enabled| {
        enabled.borrow_mut().insert(label, response.enabled());
    });
}

#[cfg(test)]
pub(crate) fn test_image_import_control_rect(label: &str) -> Option<egui::Rect> {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow().get(label).copied())
}

#[cfg(test)]
pub(crate) fn test_image_import_control_enabled(label: &str) -> Option<bool> {
    TEST_CONTROL_ENABLED.with(|enabled| enabled.borrow().get(label).copied())
}

#[cfg(test)]
pub(crate) fn test_image_import_status_lines() -> Vec<String> {
    TEST_STATUS_LINES.with(|lines| lines.borrow().clone())
}
