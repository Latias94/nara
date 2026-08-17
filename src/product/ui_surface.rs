use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginId, PluginProductCapability,
};
use nara_ecs::Resource;
use nara_input::{InputPointerId, InputSeatId, InputWindowId};
use nara_project::EffectivePrimaryUiSurface;
use nara_ui::{
    UiSafeAreaInsets, UiSize, UiSurfaceContentTransform, UiSurfaceEndpoint, UiSurfaceId,
    UiSurfaceSlot, install_ui_surface,
};

pub(crate) const PROJECT_PRIMARY_UI_SURFACE_ID: u64 = 1;
pub(crate) const PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID: PluginId =
    PluginId::new("nara.project-primary-ui-surface");
const PROJECT_PRIMARY_UI_SURFACE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.project-primary-ui-surface.configured", 1);
const PROJECT_PRIMARY_UI_SURFACE_REQUIREMENTS: &[PluginId] = &[nara_ui::UI_PLUGIN_ID];
const PROJECT_PRIMARY_UI_SURFACE_PRODUCT_REQUIREMENTS: &[PluginProductCapability] =
    &[PluginProductCapability::new("runtime-ui")];
const PROJECT_PRIMARY_UI_SURFACE_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
    PluginCategory::Runtime,
)
.requires_plugins(PROJECT_PRIMARY_UI_SURFACE_REQUIREMENTS)
.requires_product_capabilities(PROJECT_PRIMARY_UI_SURFACE_PRODUCT_REQUIREMENTS);

/// Immutable project-owned authored geometry for the primary UI surface.
///
/// The profile contains no target identity. Runtime and Editor consumers derive their own exact
/// target transform while preserving these authored facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
pub(crate) struct AcceptedPrimaryUiSurfaceProfile(EffectivePrimaryUiSurface);

impl AcceptedPrimaryUiSurfaceProfile {
    pub(crate) const fn from_effective(profile: EffectivePrimaryUiSurface) -> Self {
        Self(profile)
    }

    #[cfg(feature = "tooling")]
    pub(crate) const fn logical_extent(self) -> (u32, u32) {
        (self.0.logical_width, self.0.logical_height)
    }

    pub(crate) fn logical_size(self) -> Option<UiSize> {
        UiSize::new(self.0.logical_width as f32, self.0.logical_height as f32)
    }

    pub(crate) fn direct_transform(self) -> Option<UiSurfaceContentTransform> {
        let logical_size = self.logical_size()?;
        let safe_area = self.authored_safe_area(1.0, UiSafeAreaInsets::default())?;
        UiSurfaceContentTransform::new(logical_size, logical_size, safe_area, 1.0).ok()
    }

    #[cfg(feature = "presentation-ui")]
    pub(crate) fn target_transform(
        self,
        physical_size: UiSize,
        host_safe_area: UiSafeAreaInsets,
        platform_scale_factor: f32,
    ) -> Option<UiSurfaceContentTransform> {
        let logical_size = self.logical_size()?;
        let authored_scale = (physical_size.width() / logical_size.width())
            .min(physical_size.height() / logical_size.height());
        if !authored_scale.is_finite() || authored_scale <= 0.0 {
            return None;
        }
        let safe_area = self.authored_safe_area(authored_scale, host_safe_area)?;
        UiSurfaceContentTransform::new(
            logical_size,
            physical_size,
            safe_area,
            platform_scale_factor,
        )
        .ok()
    }

    fn authored_safe_area(
        self,
        authored_scale: f32,
        host_safe_area: UiSafeAreaInsets,
    ) -> Option<UiSafeAreaInsets> {
        UiSafeAreaInsets::new(
            host_safe_area
                .left()
                .max(self.0.safe_area.left as f32 * authored_scale),
            host_safe_area
                .top()
                .max(self.0.safe_area.top as f32 * authored_scale),
            host_safe_area
                .right()
                .max(self.0.safe_area.right as f32 * authored_scale),
            host_safe_area
                .bottom()
                .max(self.0.safe_area.bottom as f32 * authored_scale),
        )
    }

    fn canonical_configuration(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(64);
        bytes.extend_from_slice(b"nara.project-primary-ui-surface.v1\0");
        for value in [
            self.0.logical_width,
            self.0.logical_height,
            self.0.safe_area.left,
            self.0.safe_area.top,
            self.0.safe_area.right,
            self.0.safe_area.bottom,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }
}

#[derive(Debug, Clone, Copy)]
struct ProjectPrimaryUiSurfacePlugin {
    profile: AcceptedPrimaryUiSurfaceProfile,
}

impl Plugin for ProjectPrimaryUiSurfacePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PROJECT_PRIMARY_UI_SURFACE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let transform = self.profile.direct_transform().ok_or_else(setup_error)?;
        let surface_id = UiSurfaceId::new(PROJECT_PRIMARY_UI_SURFACE_ID)
            .expect("the project primary UI surface identity is non-zero");
        let endpoint = UiSurfaceEndpoint::new(
            InputWindowId::new(nara_window::WindowId::PRIMARY.get()),
            InputSeatId::PRIMARY,
            InputPointerId::PRIMARY,
        );
        let slot =
            UiSurfaceSlot::new(surface_id, endpoint, transform).map_err(|_| setup_error())?;
        app.insert_resource(self.profile)?;
        install_ui_surface(app.world_mut()?, slot).map_err(|_| setup_error())
    }
}

pub(crate) fn project_primary_ui_surface_definition(
    profile: AcceptedPrimaryUiSurfaceProfile,
) -> PluginDefinition {
    PluginDefinition::infallible::<ProjectPrimaryUiSurfacePlugin, _>(
        PROJECT_PRIMARY_UI_SURFACE_DEFINITION_ID,
        profile.canonical_configuration(),
        move || ProjectPrimaryUiSurfacePlugin { profile },
    )
}

fn setup_error() -> PluginError {
    PluginError::SetupFailed {
        plugin: PROJECT_PRIMARY_UI_SURFACE_PLUGIN_ID,
        message: "the accepted primary UI surface profile could not be lowered".to_owned(),
    }
}
