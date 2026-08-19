use egui::{Button, Grid, RichText, ScrollArea, TextEdit, Ui, Vec2};

#[cfg(test)]
use std::{cell::RefCell, collections::HashMap};

const ACTION_WIDTH: f32 = 112.0;
const ACTION_HEIGHT: f32 = 28.0;
const FORM_WIDTH: f32 = 520.0;
const DIAGNOSTIC_HEIGHT: f32 = 320.0;

/// Product-owned draft returned after one Project Shell redraw.
///
/// The egui Adapter never retains this draft. The private Editor product owns the latest value and
/// supplies it again on the next redraw or open attempt.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EguiProjectOpenDraft {
    project_path: String,
    profile: String,
}

impl EguiProjectOpenDraft {
    #[must_use]
    pub fn new(project_path: impl Into<String>, profile: impl Into<String>) -> Self {
        Self {
            project_path: project_path.into(),
            profile: profile.into(),
        }
    }

    #[must_use]
    pub fn project_path(&self) -> &str {
        &self.project_path
    }

    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub(crate) fn project_path_mut(&mut self) -> &mut String {
        &mut self.project_path
    }

    pub(crate) fn profile_mut(&mut self) -> &mut String {
        &mut self.profile
    }
}

/// Immutable Project Manager projection for one redraw.
#[derive(Debug, Clone, Copy)]
pub struct EguiProjectManagerInput<'a> {
    entry_generation: u64,
    draft: &'a EguiProjectOpenDraft,
}

impl<'a> EguiProjectManagerInput<'a> {
    #[must_use]
    pub const fn new(entry_generation: u64, draft: &'a EguiProjectOpenDraft) -> Self {
        Self {
            entry_generation,
            draft,
        }
    }
}

/// Immutable opening-attempt facts for one blocking Project Shell redraw.
#[derive(Debug, Clone, Copy)]
pub struct EguiProjectOpeningInput<'a> {
    entry_generation: u64,
    draft: &'a EguiProjectOpenDraft,
    attempt_generation: u64,
    phase: &'a str,
    relative_source: Option<&'a str>,
    summary: &'a str,
}

impl<'a> EguiProjectOpeningInput<'a> {
    #[must_use]
    pub const fn new(
        entry_generation: u64,
        draft: &'a EguiProjectOpenDraft,
        attempt_generation: u64,
        phase: &'a str,
        summary: &'a str,
    ) -> Self {
        Self {
            entry_generation,
            draft,
            attempt_generation,
            phase,
            relative_source: None,
            summary,
        }
    }

    #[must_use]
    pub const fn with_relative_source(mut self, relative_source: &'a str) -> Self {
        self.relative_source = Some(relative_source);
        self
    }
}

/// One already-classified recovery diagnostic rendered without parsing an error string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EguiProjectRecoveryDiagnostic<'a> {
    code: &'a str,
    phase: &'a str,
    relative_source: Option<&'a str>,
    summary: &'a str,
    impact: &'a str,
    repair_hint: &'a str,
}

impl<'a> EguiProjectRecoveryDiagnostic<'a> {
    #[must_use]
    pub const fn new(
        code: &'a str,
        phase: &'a str,
        relative_source: Option<&'a str>,
        summary: &'a str,
        impact: &'a str,
        repair_hint: &'a str,
    ) -> Self {
        Self {
            code,
            phase,
            relative_source,
            summary,
            impact,
            repair_hint,
        }
    }
}

/// Immutable rejected-attempt projection for one Recovery redraw.
#[derive(Debug, Clone, Copy)]
pub struct EguiProjectRecoveryInput<'a> {
    entry_generation: u64,
    draft: &'a EguiProjectOpenDraft,
    attempt_generation: u64,
    diagnostics: &'a [EguiProjectRecoveryDiagnostic<'a>],
}

impl<'a> EguiProjectRecoveryInput<'a> {
    #[must_use]
    pub const fn new(
        entry_generation: u64,
        draft: &'a EguiProjectOpenDraft,
        attempt_generation: u64,
        diagnostics: &'a [EguiProjectRecoveryDiagnostic<'a>],
    ) -> Self {
        Self {
            entry_generation,
            draft,
            attempt_generation,
            diagnostics,
        }
    }
}

/// One borrowed, immutable Project Shell state projection.
#[derive(Debug, Clone, Copy)]
pub enum EguiProjectShellInput<'a> {
    ProjectManager(EguiProjectManagerInput<'a>),
    OpeningInitial(EguiProjectOpeningInput<'a>),
    OpeningReplacement(EguiProjectOpeningInput<'a>),
    Recovery(EguiProjectRecoveryInput<'a>),
}

impl EguiProjectShellInput<'_> {
    fn entry_key(self) -> EguiProjectShellEntryKey {
        match self {
            Self::ProjectManager(input) => EguiProjectShellEntryKey {
                kind: EguiProjectShellEntryKind::ProjectManager,
                generation: input.entry_generation,
            },
            Self::OpeningInitial(input) => EguiProjectShellEntryKey {
                kind: EguiProjectShellEntryKind::OpeningInitial,
                generation: input.entry_generation,
            },
            Self::OpeningReplacement(input) => EguiProjectShellEntryKey {
                kind: EguiProjectShellEntryKind::OpeningReplacement,
                generation: input.entry_generation,
            },
            Self::Recovery(input) => EguiProjectShellEntryKey {
                kind: EguiProjectShellEntryKind::Recovery,
                generation: input.entry_generation,
            },
        }
    }
}

/// One transient action emitted by the Project Shell during a redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiProjectShellAction {
    OpenProject,
    Retry,
    Cancel,
    CloseProject,
    ReturnToProjectManager,
    Exit,
}

/// Updated product-owned draft plus at most one Project Shell action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EguiProjectShellOutput {
    draft: EguiProjectOpenDraft,
    action: Option<EguiProjectShellAction>,
}

impl EguiProjectShellOutput {
    #[must_use]
    pub fn draft(&self) -> &EguiProjectOpenDraft {
        &self.draft
    }

    #[must_use]
    pub const fn action(&self) -> Option<EguiProjectShellAction> {
        self.action
    }

    #[must_use]
    pub fn into_parts(self) -> (EguiProjectOpenDraft, Option<EguiProjectShellAction>) {
        (self.draft, self.action)
    }
}

/// Concrete egui Adapter for Project Manager, Opening, and Recovery surfaces.
///
/// This panel retains only entry-scoped focus bookkeeping. Project lifecycle, open attempts,
/// filesystem capability, diagnostics, and the editable draft remain owned by the private Editor
/// product.
#[derive(Debug, Default)]
pub struct EguiProjectShellPanel {
    focused_entry: Option<EguiProjectShellEntryKey>,
}

impl EguiProjectShellPanel {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused_entry: None,
        }
    }

    #[must_use]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        input: EguiProjectShellInput<'_>,
    ) -> EguiProjectShellOutput {
        #[cfg(test)]
        clear_test_observations();

        let entry_key = input.entry_key();
        let request_entry_focus = self.focused_entry != Some(entry_key);
        if request_entry_focus {
            self.focused_entry = Some(entry_key);
        }

        match input {
            EguiProjectShellInput::ProjectManager(input) => {
                render_project_manager(ui, input, request_entry_focus)
            }
            EguiProjectShellInput::OpeningInitial(input) => {
                render_opening(ui, input, "Opening project", request_entry_focus)
            }
            EguiProjectShellInput::OpeningReplacement(input) => render_opening(
                ui,
                input,
                "Opening replacement project",
                request_entry_focus,
            ),
            EguiProjectShellInput::Recovery(input) => {
                render_recovery(ui, input, request_entry_focus)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EguiProjectShellEntryKey {
    kind: EguiProjectShellEntryKind,
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiProjectShellEntryKind {
    ProjectManager,
    OpeningInitial,
    OpeningReplacement,
    Recovery,
}

fn render_project_manager(
    ui: &mut Ui,
    input: EguiProjectManagerInput<'_>,
    request_entry_focus: bool,
) -> EguiProjectShellOutput {
    let mut draft = input.draft.clone();
    let mut action = None;

    ui.heading("Project Manager");
    ui.label("Open an existing Nara project without loading project code.");
    ui.add_space(12.0);

    Grid::new("nara_project_manager_fields")
        .num_columns(2)
        .spacing(Vec2::new(12.0, 8.0))
        .show(ui, |ui| {
            ui.label("Project path");
            let path = ui.add(
                TextEdit::singleline(&mut draft.project_path)
                    .id_salt("nara_project_manager_path")
                    .desired_width(FORM_WIDTH)
                    .hint_text("Path to a Nara project"),
            );
            #[cfg(test)]
            record_test_control("Project path", path.rect);
            if request_entry_focus {
                path.request_focus();
            }
            ui.end_row();

            ui.label("Profile");
            let _profile = ui.add(
                TextEdit::singleline(&mut draft.profile)
                    .id_salt("nara_project_manager_profile")
                    .desired_width(FORM_WIDTH)
                    .hint_text("desktop"),
            );
            #[cfg(test)]
            record_test_control("Profile", _profile.rect);
            ui.end_row();
        });

    ui.add_space(12.0);
    ui.horizontal(|ui| {
        if shell_button(ui, "Open Project", false) {
            action.get_or_insert(EguiProjectShellAction::OpenProject);
        }
        if shell_button(ui, "Exit", false) {
            action.get_or_insert(EguiProjectShellAction::Exit);
        }
    });

    EguiProjectShellOutput { draft, action }
}

fn render_opening(
    ui: &mut Ui,
    input: EguiProjectOpeningInput<'_>,
    title: &'static str,
    request_entry_focus: bool,
) -> EguiProjectShellOutput {
    ui.heading(title);
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(input.summary);
    });
    ui.add_space(12.0);

    Grid::new("nara_project_opening_facts")
        .num_columns(2)
        .spacing(Vec2::new(12.0, 6.0))
        .show(ui, |ui| {
            fact_row(ui, "Project", input.draft.project_path());
            fact_row(ui, "Profile", input.draft.profile());
            fact_row(ui, "Attempt", &input.attempt_generation.to_string());
            fact_row(ui, "Phase", input.phase);
            if let Some(relative_source) = input.relative_source {
                fact_row(ui, "Source", relative_source);
            }
        });

    ui.add_space(12.0);
    let mut action = None;
    if shell_button(ui, "Cancel", request_entry_focus) {
        action = Some(EguiProjectShellAction::Cancel);
    }

    EguiProjectShellOutput {
        draft: input.draft.clone(),
        action,
    }
}

fn render_recovery(
    ui: &mut Ui,
    input: EguiProjectRecoveryInput<'_>,
    request_entry_focus: bool,
) -> EguiProjectShellOutput {
    ui.heading("Project recovery");
    ui.label(
        RichText::new(
            "The authoring project was not published. Runtime actions remain unavailable.",
        )
        .color(ui.visuals().error_fg_color),
    );
    ui.add_space(12.0);

    Grid::new("nara_project_recovery_attempt")
        .num_columns(2)
        .spacing(Vec2::new(12.0, 6.0))
        .show(ui, |ui| {
            fact_row(ui, "Project", input.draft.project_path());
            fact_row(ui, "Profile", input.draft.profile());
            fact_row(ui, "Attempt", &input.attempt_generation.to_string());
            fact_row(ui, "Authoring", "Rejected");
            fact_row(ui, "Runtime", "Unavailable");
        });

    ui.add_space(12.0);
    ScrollArea::vertical()
        .id_salt("nara_project_recovery_diagnostics")
        .max_height(DIAGNOSTIC_HEIGHT)
        .show(ui, |ui| {
            if input.diagnostics.is_empty() {
                ui.label("No structured recovery diagnostic was supplied.");
            }
            for (index, diagnostic) in input.diagnostics.iter().enumerate() {
                ui.push_id(index, |ui| {
                    ui.group(|ui| render_recovery_diagnostic(ui, diagnostic));
                });
                ui.add_space(8.0);
            }
        });

    ui.add_space(12.0);
    let mut action = None;
    ui.horizontal(|ui| {
        if shell_button(ui, "Retry", request_entry_focus) {
            action.get_or_insert(EguiProjectShellAction::Retry);
        }
        if shell_button(ui, "Close Project", false) {
            action.get_or_insert(EguiProjectShellAction::CloseProject);
        }
        if shell_button(ui, "Open Project", false) {
            action.get_or_insert(EguiProjectShellAction::ReturnToProjectManager);
        }
    });

    EguiProjectShellOutput {
        draft: input.draft.clone(),
        action,
    }
}

fn render_recovery_diagnostic(ui: &mut Ui, diagnostic: &EguiProjectRecoveryDiagnostic<'_>) {
    #[cfg(test)]
    record_test_recovery_fact(diagnostic);

    ui.label(RichText::new(diagnostic.code).strong());
    Grid::new("nara_project_recovery_diagnostic")
        .num_columns(2)
        .spacing(Vec2::new(12.0, 4.0))
        .show(ui, |ui| {
            fact_row(ui, "Phase", diagnostic.phase);
            if let Some(relative_source) = diagnostic.relative_source {
                fact_row(ui, "Source", relative_source);
            }
            fact_row(ui, "Summary", diagnostic.summary);
            fact_row(ui, "Impact", diagnostic.impact);
            fact_row(ui, "Repair", diagnostic.repair_hint);
        });
}

fn fact_row(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).strong());
    ui.label(value);
    ui.end_row();
}

fn shell_button(ui: &mut Ui, label: &'static str, request_focus: bool) -> bool {
    let response = ui.add(Button::new(label).min_size(Vec2::new(ACTION_WIDTH, ACTION_HEIGHT)));
    #[cfg(test)]
    record_test_control(label, response.rect);
    if request_focus {
        response.request_focus();
    }
    response.clicked()
}

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<HashMap<String, egui::Rect>> =
        RefCell::new(HashMap::new());
    static TEST_RECOVERY_FACTS: RefCell<Vec<TestRecoveryFact>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TestRecoveryFact {
    pub(crate) code: String,
    pub(crate) phase: String,
    pub(crate) relative_source: Option<String>,
    pub(crate) summary: String,
    pub(crate) impact: String,
    pub(crate) repair_hint: String,
}

#[cfg(test)]
fn record_test_control(label: &'static str, rect: egui::Rect) {
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(label.to_owned(), rect);
    });
}

#[cfg(test)]
pub(crate) fn test_control_rect(label: &str) -> Option<egui::Rect> {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow().get(label).copied())
}

#[cfg(test)]
fn record_test_recovery_fact(diagnostic: &EguiProjectRecoveryDiagnostic<'_>) {
    TEST_RECOVERY_FACTS.with(|facts| {
        facts.borrow_mut().push(TestRecoveryFact {
            code: diagnostic.code.to_owned(),
            phase: diagnostic.phase.to_owned(),
            relative_source: diagnostic.relative_source.map(str::to_owned),
            summary: diagnostic.summary.to_owned(),
            impact: diagnostic.impact.to_owned(),
            repair_hint: diagnostic.repair_hint.to_owned(),
        });
    });
}

#[cfg(test)]
pub(crate) fn test_recovery_facts() -> Vec<TestRecoveryFact> {
    TEST_RECOVERY_FACTS.with(|facts| facts.borrow().clone())
}

#[cfg(test)]
fn clear_test_observations() {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
    TEST_RECOVERY_FACTS.with(|facts| facts.borrow_mut().clear());
}
