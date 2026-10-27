//! GUI-independent panel contribution identities and user workspace preferences.

mod layout;
mod panels;

pub use layout::{
    EditorToolkitLayout, EditorWindowBounds, EditorWorkspaceLayout, EditorWorkspacePreferences,
    EditorWorkspaceRestore, EditorWorkspaceRestoreReason,
};
pub use panels::{
    EditorNativeWindowId, EditorPanelDescriptor, EditorPanelInstance, EditorPanelInstanceId,
    EditorPanelModel, EditorPanelMultiplicity, EditorPanelPlacement, EditorPanelRegistrationError,
    EditorPanelRegistry, EditorPanelSourceKind, EditorPanelTarget, EditorRenderViewId,
};

#[derive(Debug)]
pub struct EditorPanelWorkspace {
    registry: EditorPanelRegistry,
    layout: EditorWorkspaceLayout,
    restore_reasons: Vec<EditorWorkspaceRestoreReason>,
    preferences: Option<EditorWorkspacePreferences>,
    preferences_loaded: bool,
}

impl Default for EditorPanelWorkspace {
    fn default() -> Self {
        let mut registry = EditorPanelRegistry::new();
        for descriptor in [
            EditorPanelDescriptor::new(
                "nara.hierarchy",
                "Hierarchy",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Left,
                true,
            ),
            EditorPanelDescriptor::new(
                "nara.project-browser",
                "Project",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Left,
                false,
            ),
            EditorPanelDescriptor::new(
                "nara.authoring-view",
                "Authoring",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Center,
                true,
            ),
            EditorPanelDescriptor::new(
                "nara.game-view",
                "Game",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Center,
                true,
            ),
            EditorPanelDescriptor::new(
                "nara.inspector",
                "Inspector",
                EditorPanelMultiplicity::Multiple,
                EditorPanelPlacement::Right,
                true,
            ),
            EditorPanelDescriptor::new(
                "nara.diagnostics",
                "Diagnostics",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Bottom,
                true,
            ),
            EditorPanelDescriptor::new(
                "nara.source-tool",
                "Source Tool",
                EditorPanelMultiplicity::Single,
                EditorPanelPlacement::Right,
                false,
            ),
        ] {
            registry
                .register(descriptor)
                .expect("built-in contribution identities are distinct");
        }
        let layout = EditorWorkspaceLayout::default_for(&registry);
        Self {
            registry,
            layout,
            restore_reasons: Vec::new(),
            preferences: None,
            preferences_loaded: false,
        }
    }
}

impl EditorPanelWorkspace {
    #[must_use]
    pub const fn registry(&self) -> &EditorPanelRegistry {
        &self.registry
    }
    #[must_use]
    pub const fn layout(&self) -> &EditorWorkspaceLayout {
        &self.layout
    }
    pub fn layout_mut(&mut self) -> &mut EditorWorkspaceLayout {
        &mut self.layout
    }
    pub fn register(
        &mut self,
        descriptor: EditorPanelDescriptor,
    ) -> Result<(), EditorPanelRegistrationError> {
        self.register_all([descriptor])
    }

    pub(crate) fn register_all(
        &mut self,
        descriptors: impl IntoIterator<Item = EditorPanelDescriptor>,
    ) -> Result<(), EditorPanelRegistrationError> {
        let mut registry = self.registry.clone();
        let mut layout = self.layout.clone();
        for descriptor in descriptors {
            let kind = descriptor.kind().to_owned();
            let open = descriptor.initially_open();
            registry.register(descriptor)?;
            if open {
                layout.open(&registry, &kind, EditorPanelTarget::ActiveDocument)?;
            }
        }
        self.registry = registry;
        self.layout = layout;
        Ok(())
    }
    pub fn open(
        &mut self,
        kind: &str,
        target: EditorPanelTarget,
    ) -> Result<EditorPanelInstanceId, EditorPanelRegistrationError> {
        self.layout.open(&self.registry, kind, target)
    }
    pub fn reset(&mut self) {
        self.layout.reset(&self.registry);
        self.restore_reasons.clear();
    }
    pub fn restore(
        &mut self,
        layout: EditorWorkspaceLayout,
        toolkit: &str,
        version: u32,
        displays: &[EditorWindowBounds],
    ) {
        (self.layout, self.restore_reasons) =
            EditorWorkspaceLayout::restore(layout, &self.registry, toolkit, version, displays)
                .into_parts();
    }
    #[must_use]
    pub fn restore_reasons(&self) -> &[EditorWorkspaceRestoreReason] {
        &self.restore_reasons
    }
    pub fn report_preference_error(&mut self, error: impl ToString) {
        self.restore_reasons
            .push(EditorWorkspaceRestoreReason::InvalidPreferences(
                error.to_string(),
            ));
    }

    /// The native product opts in; tests and headless consumers do not touch global user settings.
    pub fn set_preferences(&mut self, preferences: EditorWorkspacePreferences) {
        self.preferences = Some(preferences);
        self.preferences_loaded = false;
    }
    #[must_use]
    pub const fn has_preferences(&self) -> bool {
        self.preferences.is_some()
    }
    pub fn load_preferences(
        &mut self,
        toolkit: &str,
        version: u32,
        displays: &[EditorWindowBounds],
    ) {
        if self.preferences_loaded {
            return;
        }
        self.preferences_loaded = true;
        let Some(preferences) = &self.preferences else {
            return;
        };
        match preferences.load() {
            Ok(Some(saved)) => self.restore(saved, toolkit, version, displays),
            Ok(None) => {}
            Err(error) => {
                self.layout.reset(&self.registry);
                self.report_preference_error(error);
            }
        }
    }
    pub fn save_preferences(&mut self) {
        if let Some(preferences) = &self.preferences
            && let Err(error) = preferences.save(&self.layout)
        {
            self.report_preference_error(error);
        }
    }
}
