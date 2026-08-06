//! ECS and Scene authoring adapter for Nara's renderer-neutral UI kernel.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
extern crate nara_ecs as bevy_ecs;

mod authoring;
mod codec;
mod interaction;
mod runtime;
mod style;

use nara_app::{App, CoreStage, Plugin, PluginError, PluginPreflightContext};
use nara_ecs::schedule::IntoScheduleConfigs;

pub use authoring::{
    UiButton, UiButtonVisuals, UiNode, UiRoot, UiRuntimeKey, UiSemantics, UiText, UiTheme, UiVisual,
};
pub use codec::register_ui_components;
pub use interaction::{
    MAX_UI_ACTION_CONSUMERS, MAX_UI_ACTION_JOURNAL_ENTRIES, UiActionAdmissionError,
    UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionJournal,
    UiActionSequence, UiInputProjectionIssue, UiRouteDisposition, UiRouteRejection,
    UiRouteRejectionLog, UiRouteRejectionRecord,
};
pub use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiPresentationReceipt, UiRouteStamp, UiSeatId, UiSurfaceId,
};
pub use runtime::{
    UiAdapterDiagnostic, UiAdapterDiagnostics, UiAdapterError, UiSurfaceDescriptor,
    UiSurfaceMappingError, active_ui_surface_descriptors, commit_ui_presentation,
    install_ui_surface, latest_ui_frame, latest_ui_frames, retire_ui_surface, ui_semantic_snapshot,
};
pub use style::*;

#[derive(Debug, Default, Clone, Copy)]
pub struct UiPlugin;

pub const UI_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui");
pub const UI_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.ui.components");
pub const UI_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.ui.components");
pub const UI_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        UI_SCHEMA_OWNER_ID,
        UI_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.ui.components.native", 1),
        ui_schema_catalog,
        crate::codec::validate_ui_components,
        register_ui_components,
    );
const UI_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara_input::INPUT_PLUGIN_ID,
    nara_hierarchy::HIERARCHY_PLUGIN_ID,
];
const UI_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-ui")];
pub const UI_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_PLUGIN_ID, nara_app::PluginCategory::Runtime)
        .requires_plugins(UI_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_PRODUCT_REQUIREMENTS)
        .provides_schema(&[UI_SCHEMA_PROVIDER_ID]);

fn ui_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(crate::codec::ui_schema_catalog())
}

impl Plugin for UiPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        let registry = nara_reflect::registry_for_plugin_preflight(
            context,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
        )?;
        UI_SCHEMA_PROVIDER.preflight(registry).map_err(|error| {
            PluginError::component_registration(UI_PLUGIN_ID, UI_SCHEMA_PROVIDER_ID.as_str(), error)
        })
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara_reflect::register_schema_provider_for_plugin(
            app,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
            &UI_SCHEMA_PROVIDER,
        )?;
        if !app.world().contains_resource::<runtime::UiDefaultTheme>() {
            app.insert_resource(runtime::UiDefaultTheme(UiTheme::default()))?;
        }
        app.insert_resource(runtime::UiRuntimeState::new().map_err(|message| {
            PluginError::SetupFailed {
                plugin: UI_PLUGIN_ID,
                message,
            }
        })?)?;
        app.init_resource::<UiAdapterDiagnostics>()?;
        app.init_resource::<UiActionJournal>()?;
        app.init_resource::<UiRouteRejectionLog>()?;
        app.add_systems(
            CoreStage::First,
            interaction::route_timeline_ui_input.in_set(nara_input::__private::InputSet::RouteUi),
        )?;
        app.add_systems(
            CoreStage::Extract,
            runtime::compile_ui_frames
                .after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
        )?;
        Ok(())
    }
}

/// Ordinary runtime authoring surface. Compiler, journal, and session types stay out of this prelude.
pub mod prelude {
    pub use crate::{
        UiActionConsumerError, UiActionConsumerId, UiActionCursor, UiActionEvent, UiActionId,
        UiActionJournal, UiActionSequence, UiAlign, UiBorder, UiBrush, UiButton, UiButtonVisuals,
        UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges, UiFlexDirection, UiImageId,
        UiJustify, UiLayer, UiNode, UiOverflow, UiPlugin, UiPositionMode, UiRoot, UiRuntimeKey,
        UiSemanticRole, UiSemantics, UiStyle, UiText, UiTextAlignment, UiTextOverflow, UiTheme,
        UiThemePalette, UiVisual,
    };
}

/// Exact presentation and presented-input boundary for first-party product integrations.
pub mod advanced {
    pub use crate::{
        UiAdapterDiagnostic, UiAdapterDiagnostics, UiAdapterError, UiInputProjectionIssue,
        UiPresentationReceipt, UiRouteDisposition, UiRouteRejection, UiRouteRejectionLog,
        UiRouteRejectionRecord, UiRouteStamp, UiSeatId, UiSurfaceDescriptor, UiSurfaceId,
        UiSurfaceMappingError, active_ui_surface_descriptors, commit_ui_presentation,
        install_ui_surface, latest_ui_frame, latest_ui_frames, retire_ui_surface,
        ui_semantic_snapshot,
    };
    pub use nara_ui_kernel::{UiFrame, UiInputEvent, UiSemanticSnapshot};
}

#[cfg(test)]
mod adapter_tests;
