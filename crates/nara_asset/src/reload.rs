use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::{Resource, SystemSet, schedule::IntoScheduleConfigs};

use crate::{AssetDependencyGraph, AssetEvents, AssetServer, AssetStates, ProjectAssetDatabase};

mod diagnostics;
mod requests;
mod resolution;
mod source_changes;

pub use requests::{
    AssetLoadGeneration, AssetLoadGenerations, AssetReloadRequest,
    AssetReloadRequestAdmissionError, AssetReloadRequestId, AssetReloadRequestKind,
    AssetReloadRequestLimitKind, AssetReloadRequestLimits, AssetReloadRequests,
    ImageReloadConsumer, ImageReloadDrainError, ImageReloadRegistrationError,
    register_image_reload_consumer,
};
use resolution::{reject_unclaimed_asset_reload_requests, resolve_asset_source_changes};
pub use source_changes::{
    AssetSourceChange, AssetSourceChangeAdmissionError, AssetSourceChangeKind,
    AssetSourceChangeLimitKind, AssetSourceChangeLimits, AssetSourceChanges, AssetSourceRoot,
};

#[cfg(test)]
use requests::ImageReloadRegistration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, SystemSet)]
pub enum AssetTaskUpdateSet {
    Poll,
    ResolveSourceChanges,
    SpawnJobs,
    ApplyResults,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Resource)]
pub struct AssetReloadDiagnostics {
    report: DiagnosticReport,
}

impl AssetReloadDiagnostics {
    pub fn clear(&mut self) {
        self.report = DiagnosticReport::default();
    }

    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.report.push(diagnostic);
    }

    #[must_use]
    pub const fn report(&self) -> &DiagnosticReport {
        &self.report
    }

    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Diagnostic> {
        self.report.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.report.is_empty()
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.report.has_errors()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
enum AssetReloadInternalSet {
    FinalizeRequests,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct AssetPlugin;

pub const ASSET_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.asset");
pub const ASSET_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(ASSET_PLUGIN_ID, nara_app::PluginCategory::Asset)
        .requires_plugins(&[nara_tasks::TASK_PLUGIN_ID]);

impl Plugin for AssetPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &ASSET_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<AssetServer>()?;
        app.init_resource::<AssetStates>()?;
        app.init_resource::<AssetEvents>()?;
        app.init_resource::<AssetDependencyGraph>()?;
        app.init_resource::<ProjectAssetDatabase>()?;
        app.init_resource::<AssetSourceChanges>()?;
        app.init_resource::<AssetReloadRequests>()?;
        app.init_resource::<AssetReloadDiagnostics>()?;
        app.init_resource::<AssetLoadGenerations>()?;
        app.configure_sets(
            CoreStage::TaskUpdate,
            (
                AssetTaskUpdateSet::Poll,
                AssetTaskUpdateSet::ResolveSourceChanges,
                AssetTaskUpdateSet::SpawnJobs,
                AssetTaskUpdateSet::ApplyResults,
            )
                .chain(),
        )?;
        app.configure_sets(
            CoreStage::TaskUpdate,
            AssetReloadInternalSet::FinalizeRequests.after(AssetTaskUpdateSet::ApplyResults),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            resolve_asset_source_changes.in_set(AssetTaskUpdateSet::ResolveSourceChanges),
        )?;
        app.add_systems(
            CoreStage::TaskUpdate,
            reject_unclaimed_asset_reload_requests.in_set(AssetReloadInternalSet::FinalizeRequests),
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "reload/tests.rs"]
mod tests;
