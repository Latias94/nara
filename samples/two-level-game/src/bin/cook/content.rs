use super::input::RuntimeKind;
use nara::{
    asset_build::{ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest},
    fs::{DirectoryCapability, RelativePath},
    project_host::{ProjectSettingsCandidate, ingest_project_manifest},
    project_tooling::{
        host_fs,
        project_cook::{
            PreparedProjectCook, ProjectCookError, ProjectCookRequest,
            prepare_project_cooked_content,
        },
    },
    project_workflow::cooked_content::CookedContentLimits,
    runtime_content::RuntimeContentTarget,
};
use nara_sprite_animation::SpriteAnimationDomain;
use two_level_game::sound_bank::SoundBankDomain;

const COOKER_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(test)]
pub(super) fn prepare(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
    runtime: RuntimeKind,
) -> Result<PreparedProjectCook, String> {
    prepare_for_player(root, cache, triple, runtime).map_err(|error| match error {
        ProjectCookError::Source(source) => format!("source delivery agreement: {source:?}"),
        error => error.to_string(),
    })
}

pub(super) fn prepare_for_player(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
    runtime: RuntimeKind,
) -> Result<PreparedProjectCook, ProjectCookError> {
    if runtime == RuntimeKind::AudioDesktop && !triple.contains("-windows-") {
        return Err(ProjectCookError::Invalid(
            "audio-desktop Player requires an explicit Windows target".into(),
        ));
    }
    let candidate = project_settings(root, runtime).map_err(ProjectCookError::Invalid)?;
    let recipe = runtime.recipe().map_err(ProjectCookError::Invalid)?;
    let target = RuntimeContentTarget::new(triple, runtime.content_profile())
        .map_err(|error| ProjectCookError::Invalid(format!("cooked content target: {error}")))?;
    let implementation = implementation_digest().map_err(ProjectCookError::Invalid)?;
    let store = ArtifactBuildStore::open(cache.share(), ArtifactBuildLimits::default())?;
    prepare_project_cooked_content(
        ProjectCookRequest {
            root,
            candidate: &candidate,
            recipe,
            target,
            store: &store,
            implementation,
            limits: CookedContentLimits::default(),
        },
        |cx| {
            cx.add_source_assets::<SoundBankDomain>()?;
            cx.add_source_assets::<SpriteAnimationDomain>()?;
            Ok(())
        },
    )
}

fn project_settings(
    root: &DirectoryCapability,
    runtime: RuntimeKind,
) -> Result<ProjectSettingsCandidate, String> {
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").map_err(|error| error.to_string())?)
        .map_err(|error| format!("nara.toml: {error}"))?;
    let candidate = ingest_project_manifest(
        &manifest,
        match runtime {
            RuntimeKind::Headless => None,
            RuntimeKind::Desktop | RuntimeKind::AudioDesktop => Some("desktop"),
        },
    )
    .map_err(|error| format!("nara.toml: {error:?}"))?;
    if candidate.settings().window.enabled != (runtime != RuntimeKind::Headless) {
        return Err(format!(
            "nara.toml: the {} Player requires window.enabled={}",
            runtime.name(),
            runtime != RuntimeKind::Headless
        ));
    }
    Ok(candidate)
}

fn implementation_digest() -> Result<ArtifactContentDigest, String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("locating the cooker executable: {error}"))?;
    let file = host_fs::readable_file(&executable)
        .map_err(|error| format!("opening the cooker executable: {error}"))?;
    let digest = file
        .digest(COOKER_BYTES)
        .map_err(|error| format!("hashing the cooker executable: {error}"))?;
    Ok(ArtifactContentDigest::from_content(digest))
}
