use std::sync::Arc;

use nara::{
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs},
};
use nara_render_wgpu::{
    WGPU_STOCK_CAPTURE_PLUGIN_ID, WgpuPolicyCaptureSet, WgpuPolicyDefinition,
    WgpuPolicyFrameReader, WgpuPolicyFrameWriter, WgpuPolicyInstallation, WgpuRenderInput,
    WgpuStockCaptureSet, WgpuStockFrameReader, install_wgpu_policy,
};

use crate::{CompositionCapture, CompositionConfiguration, CompositionPolicy};

/// The four most recent original CPU inputs, without transport or UI completion custody.
#[derive(Default, Resource)]
pub struct RetainedInputs(pub Vec<WgpuRenderInput>);

#[derive(Resource)]
struct Configuration(CompositionConfiguration);

#[derive(Clone, Copy, Debug, Default)]
pub struct CompositionPlugin(pub CompositionConfiguration);

impl Plugin for CompositionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.stock-composition"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[WGPU_STOCK_CAPTURE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let installation = WgpuPolicyDefinition::new(CompositionPolicy::new())
            .install()
            .map_err(|error| PluginError::SetupFailed {
                plugin: Self::declaration().id,
                message: error.to_string(),
            })?;
        install_wgpu_policy(app, installation)?;
        app.insert_resource(Configuration(self.0))?;
        app.init_resource::<RetainedInputs>()?;
        app.add_systems(
            CoreStage::Render,
            compose.in_set(WgpuStockCaptureSet::Compose),
        )?;
        app.add_systems(
            CoreStage::Render,
            retain_original
                .after(WgpuPolicyCaptureSet::Seal)
                .before(WgpuPolicyCaptureSet::Publish),
        )?;
        Ok(())
    }
}

fn compose(
    installation: Res<WgpuPolicyInstallation<CompositionPolicy>>,
    configuration: Res<Configuration>,
    mut reader: WgpuStockFrameReader,
    mut writer: WgpuPolicyFrameWriter,
) -> nara::ecs::bevy_ecs::error::Result {
    let Some(frame) = reader.take(&installation, &writer) else {
        return Ok(());
    };
    let (stock, ui) = frame.into_parts();
    let capture = Arc::new(CompositionCapture::new(stock, configuration.0));
    if let Some(ui) = ui {
        if let Err(failure) = writer.capture_with_ui(&installation, capture, ui) {
            let (error, ui) = failure.into_parts();
            // Drop returns the exact original permits' pre-admitted Aborted outcome. The
            // scheduler propagates the fault; it does not invent successful presentation.
            drop(ui);
            return Err(error.into());
        }
    } else if !writer.capture(&installation, capture)? {
        return Err(nara_render_wgpu::WgpuRenderProgramError::feature(
            "stock frame admission changed during composition",
        )
        .into());
    }
    Ok(())
}

fn retain_original(
    reader: WgpuPolicyFrameReader,
    mut retained: ResMut<RetainedInputs>,
) -> nara::ecs::bevy_ecs::error::Result {
    if let Some(input) = reader
        .read()
        .map_err(|error| nara_render_wgpu::WgpuRenderProgramError::feature(error.to_string()))?
    {
        if retained.0.len() == 4 {
            retained.0.remove(0);
        }
        retained.0.push(input.clone());
    }
    Ok(())
}
