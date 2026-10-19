use nara_app::{EditedPluginGroup, PluginGroup, PluginGroupBuilder, PluginGroupId};
#[cfg(feature = "runtime-ui")]
use nara_project::ProductCapability;
use nara_project::ProductCapabilitySet;

use super::ProductRecipe;
#[cfg(feature = "runtime-ui")]
use super::ProductRecipeError;

#[derive(Default, Clone)]
pub(super) struct ProductDefaultSettings {
    #[cfg(feature = "runtime-ui")]
    ui_font: Option<nara_ui::FontFace>,
}

impl std::fmt::Debug for ProductDefaultSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[allow(unused_mut)]
        let mut debug = formatter.debug_struct("ProductDefaultSettings");
        #[cfg(feature = "runtime-ui")]
        debug.field("ui_font", &self.ui_font.as_ref().map(nara_ui::FontFace::id));
        debug.finish()
    }
}

impl ProductDefaultSettings {
    pub(super) const fn new() -> Self {
        Self {
            #[cfg(feature = "runtime-ui")]
            ui_font: None,
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        #[cfg(feature = "runtime-ui")]
        if self.ui_font.is_some() {
            return false;
        }
        true
    }

    pub(super) fn unsupported_plugin(
        &self,
        capabilities: ProductCapabilitySet,
    ) -> Option<nara_app::PluginId> {
        let _ = capabilities;
        #[cfg(feature = "runtime-ui")]
        if self.ui_font.is_some() && !capabilities.contains(ProductCapability::RuntimeUi) {
            return Some(nara_ui::UI_PLUGIN_ID);
        }
        None
    }

    pub(super) fn apply<G: PluginGroup>(
        self,
        defaults: EditedPluginGroup<G>,
    ) -> EditedPluginGroup<G> {
        #[cfg(feature = "runtime-ui")]
        if let Some(face) = self.ui_font {
            let defaults = defaults.configure_slot(super::SLOT_UI, nara_ui::plugin(face.clone()));
            #[cfg(feature = "presentation-ui")]
            let defaults = defaults
                .configure_slot_if_present(super::SLOT_UI_RENDER, nara_ui_render::plugin(face));
            return defaults;
        }
        defaults
    }
}

impl ProductRecipe {
    /// Selects one admitted immutable face for UI compilation and any UI presentation default.
    ///
    /// The enclosing defaults must install `RuntimeUiPlugins`. Presentation, when installed,
    /// receives the same face automatically. File-backed Hosts supply that enclosing group;
    /// direct `App` users supply it through [`Self::with_defaults`]. Repeated selection is rejected.
    #[cfg(feature = "runtime-ui")]
    pub fn with_ui_font(mut self, font: nara_ui::FontFace) -> Result<Self, ProductRecipeError> {
        if self.defaults.ui_font.is_some() {
            return Err(ProductRecipeError::DuplicateDefaultConfiguration {
                plugin: nara_ui::UI_PLUGIN_ID,
            });
        }
        self.defaults.ui_font = Some(font);
        Ok(self)
    }

    /// Returns the immutable font selection without constructing a UI runtime.
    #[cfg(feature = "runtime-ui")]
    #[must_use]
    pub const fn ui_font(&self) -> Option<&nara_ui::FontFace> {
        self.defaults.ui_font.as_ref()
    }

    /// Composes this recipe with the group that owns its configurable defaults.
    ///
    /// All default edits apply inside this shared owner before normal plugin-plan resolution.
    /// Genuine duplicate plugins, incompatible slots and divergent definitions still reject.
    /// A recipe with configured defaults must use this method for direct `App` composition;
    /// adding it beside an unrelated defaults group cannot confer ownership of that group.
    #[must_use]
    pub fn with_defaults<G: PluginGroup>(self, defaults: G) -> impl PluginGroup {
        self.with_edited_defaults(defaults.edit())
    }

    pub(crate) fn with_edited_defaults<G: PluginGroup>(
        self,
        defaults: EditedPluginGroup<G>,
    ) -> ProductPlugins<G> {
        ProductPlugins {
            defaults,
            recipe: self,
        }
    }

    pub(crate) fn unsupported_default_plugin(
        &self,
        capabilities: ProductCapabilitySet,
    ) -> Option<nara_app::PluginId> {
        self.defaults.unsupported_plugin(capabilities)
    }
}

pub(crate) struct ProductPlugins<G> {
    defaults: EditedPluginGroup<G>,
    recipe: ProductRecipe,
}

impl<G: PluginGroup> PluginGroup for ProductPlugins<G> {
    const ID: PluginGroupId = PluginGroupId::new("nara.product-with-defaults");

    fn build(mut self) -> PluginGroupBuilder {
        let settings = std::mem::take(&mut self.recipe.defaults);
        self.recipe.build_entries(
            PluginGroupBuilder::new().add_edited_group(settings.apply(self.defaults)),
        )
    }
}

pub(super) struct MissingProductDefaults;

impl PluginGroup for MissingProductDefaults {
    const ID: PluginGroupId = PluginGroupId::new("nara.product-missing-defaults");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
    }
}
