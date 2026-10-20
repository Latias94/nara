#[cfg(any(test, feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_app::Plugin;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_app::{App, CoreStage, PluginError};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_ecs::schedule::IntoScheduleConfigs;

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::frame::WgpuFrameSet;

#[cfg(feature = "sprite-submitter")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuSpriteSubmitterPlugin;

#[cfg(feature = "sprite-submitter")]
pub const WGPU_SPRITE_SUBMITTER_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.sprite-submitter");
#[cfg(feature = "sprite-submitter")]
const WGPU_SPRITE_SUBMITTER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    crate::WGPU_RENDER_PLUGIN_ID,
    nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
];
#[cfg(feature = "sprite-submitter")]
const WGPU_SPRITE_SUBMITTER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-2d"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
#[cfg(feature = "sprite-submitter")]
pub const WGPU_SPRITE_SUBMITTER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_SPRITE_SUBMITTER_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_SPRITE_SUBMITTER_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_SPRITE_SUBMITTER_PRODUCT_REQUIREMENTS);

#[cfg(feature = "sprite-submitter")]
impl Plugin for WgpuSpriteSubmitterPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_SPRITE_SUBMITTER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::Render,
            crate::collect_wgpu_sprite_payload.in_set(WgpuFrameSet::CollectSprite),
        )?;
        Ok(())
    }
}

#[cfg(feature = "ui-submitter")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuUiSubmitterPlugin;

#[cfg(feature = "ui-submitter")]
pub const WGPU_UI_SUBMITTER_PLUGIN_ID: nara_app::PluginId =
    nara_app::PluginId::new("nara.render-wgpu.ui-submitter");
#[cfg(feature = "ui-submitter")]
const WGPU_UI_SUBMITTER_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    crate::WGPU_RENDER_PLUGIN_ID,
    nara_ui_render::UI_RENDER_PLUGIN_ID,
];
#[cfg(feature = "ui-submitter")]
const WGPU_UI_SUBMITTER_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] = &[
    nara_app::PluginProductCapability::new("presentation-ui"),
    nara_app::PluginProductCapability::new("render-wgpu"),
];
#[cfg(feature = "ui-submitter")]
pub const WGPU_UI_SUBMITTER_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(
        WGPU_UI_SUBMITTER_PLUGIN_ID,
        nara_app::PluginCategory::Backend,
    )
    .requires_plugins(WGPU_UI_SUBMITTER_PLUGIN_REQUIREMENTS)
    .requires_product_capabilities(WGPU_UI_SUBMITTER_PRODUCT_REQUIREMENTS);

#[cfg(feature = "ui-submitter")]
impl Plugin for WgpuUiSubmitterPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &WGPU_UI_SUBMITTER_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            CoreStage::Render,
            (
                crate::collect_wgpu_ui_payload.in_set(WgpuFrameSet::CollectUi),
                crate::complete_wgpu_ui_presentation.in_set(WgpuFrameSet::CompleteUi),
            ),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use nara_app::Plugin;

    use super::*;

    #[test]
    fn base_plugin_declaration_is_feature_invariant() {
        let declaration = crate::WgpuRenderPlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[nara_render::RENDER_PLUGIN_ID]
        );
        assert_eq!(
            declaration.requires_product_capabilities,
            &[nara_app::PluginProductCapability::new("render-wgpu")]
        );
    }

    #[cfg(feature = "sprite-submitter")]
    #[test]
    fn compiling_sprite_support_does_not_install_the_contribution() {
        let mut app = nara_app::App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderComponentsPlugin,
            nara_render::RenderPlugin::default(),
            crate::WgpuRenderPlugin,
        ))
        .unwrap();

        assert!(app.has_plugin(crate::WGPU_RENDER_PLUGIN_ID));
        assert!(!app.has_plugin(WGPU_SPRITE_SUBMITTER_PLUGIN_ID));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn compiling_ui_support_does_not_install_the_contribution() {
        let mut app = nara_app::App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_transform::TransformPlugin,
            nara_render::RenderComponentsPlugin,
            nara_render::RenderPlugin::default(),
            crate::WgpuRenderPlugin,
        ))
        .unwrap();

        assert!(app.has_plugin(crate::WGPU_RENDER_PLUGIN_ID));
        assert!(!app.has_plugin(WGPU_UI_SUBMITTER_PLUGIN_ID));
    }

    #[cfg(feature = "sprite-submitter")]
    #[test]
    fn sprite_contribution_requires_the_base_and_sprite_owner() {
        let declaration = WgpuSpriteSubmitterPlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[
                crate::WGPU_RENDER_PLUGIN_ID,
                nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
            ]
        );
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn ui_contribution_requires_the_base_and_ui_owner() {
        let declaration = WgpuUiSubmitterPlugin::declaration();
        assert_eq!(
            declaration.requires_plugins,
            &[
                crate::WGPU_RENDER_PLUGIN_ID,
                nara_ui_render::UI_RENDER_PLUGIN_ID,
            ]
        );
    }
}
