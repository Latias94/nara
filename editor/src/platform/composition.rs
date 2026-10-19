use std::collections::BTreeMap;

use nara::project_host::TrustedCompiledEditorProjectIntent;

use crate::{
    EditorApplication, EditorLaunchOptions, ImguiEditorFrontend, ImguiEditorPanel,
    workspace::{
        EditorPanelDescriptor, EditorPanelRegistrationError, EditorPanelRegistry,
        EditorPanelWorkspace,
    },
};

use super::EditorRunError;

type PanelFactory = Box<dyn Fn() -> Box<dyn ImguiEditorPanel>>;

/// Trusted source-built native Editor extensions, paired with their frontend factories.
///
/// Ordinary project Open never loads executable panel code. Factories run on the native UI
/// thread once per painted panel instance; they need neither `Send` nor global mutable state.
pub struct EditorNativeComposition {
    registry: EditorPanelRegistry,
    factories: BTreeMap<String, PanelFactory>,
}

impl Default for EditorNativeComposition {
    fn default() -> Self {
        Self {
            registry: EditorPanelWorkspace::default().registry().clone(),
            factories: BTreeMap::new(),
        }
    }
}

impl EditorNativeComposition {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a descriptor and factory together, rejecting empty, duplicate and built-in IDs.
    pub fn register_panel<P: ImguiEditorPanel + 'static>(
        &mut self,
        descriptor: EditorPanelDescriptor,
        factory: impl Fn() -> P + 'static,
    ) -> Result<(), EditorPanelRegistrationError> {
        let kind = descriptor.kind().to_owned();
        self.registry.register(descriptor)?;
        self.factories
            .insert(kind, Box::new(move || Box::new(factory())));
        Ok(())
    }

    /// Installs the same paired composition used by the native runner into an embedded shell.
    ///
    /// Admission leaves the application's workspace unchanged on error. The returned frontend
    /// owns the factories and all subsequently created panel drafts.
    pub fn install(
        self,
        application: &mut EditorApplication,
    ) -> Result<ImguiEditorFrontend, EditorPanelRegistrationError> {
        let descriptors = self.factories.keys().map(|kind| {
            self.registry
                .descriptor(kind)
                .expect("a paired factory has its admitted descriptor")
                .clone()
        });
        application
            .panel_workspace_mut()
            .register_all(descriptors)?;
        let mut frontend = ImguiEditorFrontend::new();
        for (kind, factory) in self.factories {
            frontend.register_panel_factory(kind, factory);
        }
        Ok(frontend)
    }

    /// Runs the ordinary native Editor with this source-built composition.
    pub fn run(self, options: EditorLaunchOptions) -> Result<(), EditorRunError> {
        super::imgui::run_application(EditorApplication::new(options), self)
    }

    /// Runs the native Editor with an explicitly trusted compiled product intent.
    #[doc(hidden)]
    pub fn run_trusted_compiled(
        self,
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<(), EditorRunError> {
        super::imgui::run_application(
            EditorApplication::open_trusted_compiled(options, intent)?,
            self,
        )
    }
}
