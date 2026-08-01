#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    fs::File,
    num::NonZeroU32,
    path::Path,
};

use engine::{
    ProductConfiguration, ProductRecipe, ProductRecipeError, SchemaContribution,
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId},
    ecs::{ResMut, Resource},
    fs::{
        CapabilityRights, DirectoryCapability, FsError, FsOperation, HostCapabilityOptions,
        TrustMode,
    },
    prelude::{App, CoreStage, Plugin},
    project_host::{HeadlessRun, HeadlessRunOutcome},
    reflect::{
        ComponentRegistry, ComponentRegistryError, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaProviderSourceError,
    },
};

const RUNTIME_PROBE_PLUGIN_ID: PluginId = PluginId::new("renamed-root.runtime-probe");
const RUNTIME_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RUNTIME_PROBE_PLUGIN_ID, PluginCategory::Runtime);

const CONTRIBUTION_PLUGIN_ID: PluginId = PluginId::new("renamed-root.configured-contribution");
const CONTRIBUTION_REQUIREMENTS: &[PluginId] = &[RUNTIME_PROBE_PLUGIN_ID];
const CONTRIBUTION_SCHEMA_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("renamed-root.schema");
const CONTRIBUTION_SCHEMA_OWNER_ID: ComponentSchemaOwnerId =
    ComponentSchemaOwnerId::new("renamed-root.schema");
const CONTRIBUTION_SCHEMA_PROVIDER: ComponentSchemaProviderDefinition =
    ComponentSchemaProviderDefinition::new(
        CONTRIBUTION_SCHEMA_OWNER_ID,
        CONTRIBUTION_SCHEMA_PROVIDER_ID,
        ComponentSchemaProviderBindingId::new("renamed-root.schema.native", 1),
        empty_schema_catalog,
        register_empty_schema,
    );
const CONTRIBUTION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CONTRIBUTION_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(CONTRIBUTION_REQUIREMENTS)
        .provides_schema(&[CONTRIBUTION_SCHEMA_PROVIDER_ID]);

#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct ExternalRunOutcome {
    pub ticks: u64,
    pub configuration: u64,
    pub runtime_plugin_seen: bool,
}

#[derive(Debug)]
pub enum ExternalRunnerError {
    Authority(String),
    Recipe(String),
    Run(String),
    UnexpectedOutcome,
}

impl Display for ExternalRunnerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Authority(error) => write!(formatter, "project authority failed: {error}"),
            Self::Recipe(error) => write!(formatter, "product recipe failed: {error}"),
            Self::Run(error) => write!(formatter, "headless product run failed: {error}"),
            Self::UnexpectedOutcome => formatter.write_str("headless product returned an unexpected outcome"),
        }
    }
}

impl Error for ExternalRunnerError {}

#[derive(Debug, Default)]
struct RuntimeProbePlugin;

impl Plugin for RuntimeProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RUNTIME_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ExternalRunOutcome {
            ticks: 0,
            configuration: 0,
            runtime_plugin_seen: true,
        })?
        .add_systems(CoreStage::FixedUpdate, observe_fixed_tick)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct ContributionMarker(u64);

fn observe_fixed_tick(
    marker: engine::ecs::Res<ContributionMarker>,
    mut outcome: ResMut<ExternalRunOutcome>,
) {
    outcome.ticks = outcome.ticks.saturating_add(1);
    outcome.configuration = marker.0;
}

#[derive(Debug, Clone, Copy)]
struct ContributionConfiguration {
    marker: u64,
}

impl ProductConfiguration for ContributionConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.marker.to_le_bytes());
    }
}

#[derive(Debug)]
struct ConfiguredContributionPlugin {
    marker: u64,
}

impl Plugin for ConfiguredContributionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CONTRIBUTION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ContributionMarker(self.marker))?;
        Ok(())
    }
}

fn empty_schema_catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog::default())
}

fn register_empty_schema(_registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    Ok(())
}

fn recipe() -> Result<ProductRecipe, ProductRecipeError> {
    let contribution = SchemaContribution::<ConfiguredContributionPlugin>::configured(
        ContributionConfiguration { marker: 42 },
        |configuration: &ContributionConfiguration| ConfiguredContributionPlugin {
            marker: configuration.marker,
        },
        [CONTRIBUTION_SCHEMA_PROVIDER],
    )?;

    ProductRecipe::new()
        .add_plugin::<RuntimeProbePlugin>()?
        .add_contribution(contribution)
}

pub fn run() -> Result<ExternalRunOutcome, ExternalRunnerError> {
    let mut product: HeadlessRun<ExternalRunOutcome> = HeadlessRun::from_recipe(
        project_root().map_err(|error| ExternalRunnerError::Authority(error.to_string()))?,
        recipe().map_err(|error| ExternalRunnerError::Recipe(error.to_string()))?,
        NonZeroU32::new(1).expect("the fixture run has a non-zero tick bound"),
        Vec::new(),
    );

    for _ in 0..8 {
        let report = product.execute_bounded();
        match report.into_outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                if outcome.ticks == 1
                    && outcome.configuration == 42
                    && outcome.runtime_plugin_seen
                {
                    return Ok(outcome);
                }
                return Err(ExternalRunnerError::UnexpectedOutcome);
            }
            HeadlessRunOutcome::CleanupIncomplete => std::thread::yield_now(),
            HeadlessRunOutcome::Failed => {
                return Err(ExternalRunnerError::Run("product action failed".to_owned()));
            }
        }
    }

    Err(ExternalRunnerError::Run(
        "bounded cleanup did not complete".to_owned(),
    ))
}

fn project_root() -> Result<DirectoryCapability, FsError> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("project");
    let file = open_directory(&path).map_err(|source| FsError::Io {
        operation: FsOperation::OpenDirectory,
        source,
    })?;
    DirectoryCapability::from_host_handle(
        file,
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
    )
}

#[cfg(windows)]
fn open_directory(path: &Path) -> Result<File, std::io::Error> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ_WRITE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_directory(path: &Path) -> Result<File, std::io::Error> {
    File::open(path)
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn ordinary_recipe_runs_from_the_renamed_root() {
        let outcome = run().expect("the external ordinary product path must complete");
        assert_eq!(outcome.ticks, 1);
        assert_eq!(outcome.configuration, 42);
        assert!(outcome.runtime_plugin_seen);
    }
}
