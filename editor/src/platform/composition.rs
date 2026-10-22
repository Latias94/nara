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

/// Failure to attach panels or construct the complete compiled Editor composition.
#[derive(Debug, thiserror::Error)]
pub enum EditorCompositionError {
    #[error(transparent)]
    Panel(#[from] EditorPanelRegistrationError),
    #[error("Host contributions require building the complete Editor composition")]
    HostContributionsRequireBuild,
}

/// Trusted source-built native Editor extensions, paired with their frontend factories.
///
/// Ordinary project Open never loads executable panel code. Factories run on the native UI
/// thread once per painted panel instance; they need neither `Send` nor global mutable state.
pub struct EditorNativeComposition {
    registry: EditorPanelRegistry,
    factories: BTreeMap<String, PanelFactory>,
    source_asset_importers: nara::project_host::EditorSourceAssetImporters,
}

impl Default for EditorNativeComposition {
    fn default() -> Self {
        Self {
            registry: EditorPanelWorkspace::default().registry().clone(),
            factories: BTreeMap::new(),
            source_asset_importers: Default::default(),
        }
    }
}

impl EditorNativeComposition {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a trusted compiled source-metadata facet without loading project Runtime code.
    pub fn register_source_asset_domain<D: nara::project_host::EditorSourceAssetDomain>(
        &mut self,
    ) -> Result<(), nara::ProductRecipeError> {
        self.source_asset_importers.add::<D>()
    }

    /// Consumes the complete selection into an application and its paired frontend.
    /// Native `run` uses this same construction before opening any project.
    pub fn build(
        self,
        options: EditorLaunchOptions,
    ) -> Result<(EditorApplication, ImguiEditorFrontend), EditorCompositionError> {
        let mut application = EditorApplication::new_with_source_asset_importers(
            options,
            self.source_asset_importers.clone(),
        );
        let frontend = self.install_panel_factories(&mut application)?;
        Ok((application, frontend))
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

    /// Attaches only panels to an existing application, preserving it on admission failure.
    ///
    /// Selected Host contributions reject before any workspace change; use `build` for the
    /// complete composition. The frontend owns factories and all subsequent panel drafts.
    pub fn install_panels(
        self,
        application: &mut EditorApplication,
    ) -> Result<ImguiEditorFrontend, EditorCompositionError> {
        if !self.source_asset_importers.is_empty() {
            return Err(EditorCompositionError::HostContributionsRequireBuild);
        }
        self.install_panel_factories(application)
            .map_err(Into::into)
    }

    fn install_panel_factories(
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
        let (application, frontend) = self.build(options)?;
        super::imgui::run_application(application, frontend)
    }

    /// Builds the explicitly advanced in-process lane with the same compiled Host selection.
    /// Its initial lane has no generic source index or metadata Import. The selected facets
    /// remain available to later ordinary authoring replacements.
    #[doc(hidden)]
    pub fn build_trusted_compiled(
        self,
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<(EditorApplication, ImguiEditorFrontend), EditorRunError> {
        let mut application = EditorApplication::open_trusted_compiled_with_source_asset_importers(
            options,
            intent,
            self.source_asset_importers.clone(),
        )?;
        let frontend = self.install_panel_factories(&mut application)?;
        Ok((application, frontend))
    }

    /// Runs the native Editor with an explicitly trusted compiled product intent.
    #[doc(hidden)]
    pub fn run_trusted_compiled(
        self,
        options: EditorLaunchOptions,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<(), EditorRunError> {
        let (application, frontend) = self.build_trusted_compiled(options, intent)?;
        super::imgui::run_application(application, frontend)
    }
}
