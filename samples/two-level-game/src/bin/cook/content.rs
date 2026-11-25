use super::input::{PlayerSelection, RuntimeKind};
use nara::{
    asset_build::{ArtifactBuildLimits, ArtifactBuildStore, ArtifactContentDigest},
    fs::{DirectoryCapability, FileCapability, RelativePath},
    project_host::{ProjectSettingsCandidate, ingest_project_manifest},
    project_tooling::{
        host_fs,
        project_cook::{
            PreparedProjectCook, ProjectCookError, ProjectCookRequest, ProjectServerCookTarget,
            prepare_project_cooked_content, prepare_project_server_content,
        },
    },
    project_workflow::cooked_content::CookedContentLimits,
    reflect::ComponentTypeId,
    runtime_content::RuntimeContentTarget,
};
use nara_render::animation::SpriteAnimationDomain;
use two_level_game::{delivery::ControllerBackend, sound_bank::SoundBankDomain};

const COOKER_BYTES: u64 = 512 * 1024 * 1024;

#[cfg(test)]
pub(super) fn prepare(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
    selection: PlayerSelection,
) -> Result<PreparedProjectCook, String> {
    prepare_for_player(root, cache, triple, selection).map_err(|error| match error {
        ProjectCookError::Source(source) => format!("source delivery agreement: {source:?}"),
        error => error.to_string(),
    })
}

pub(super) fn prepare_for_player(
    root: &DirectoryCapability,
    cache: &DirectoryCapability,
    triple: &str,
    selection: PlayerSelection,
) -> Result<PreparedProjectCook, ProjectCookError> {
    selection
        .validate_target(triple)
        .map_err(ProjectCookError::Invalid)?;
    let manifest_path = RelativePath::new("nara.toml").expect("static manifest path");
    let manifest = root
        .open_file(&manifest_path)
        .map_err(|error| ProjectCookError::Invalid(format!("nara.toml: {error}")))?;
    let server = selection.runtime() == RuntimeKind::Server;
    let source = if server {
        PlayerSelection::new(RuntimeKind::Headless, ControllerBackend::Disabled)
            .map_err(ProjectCookError::Invalid)?
    } else {
        selection
    };
    let candidate = project_settings(&manifest, source).map_err(ProjectCookError::Invalid)?;
    let recipe = source.recipe().map_err(ProjectCookError::Invalid)?;
    let target = RuntimeContentTarget::new(triple, selection.content_profile())
        .map_err(|error| ProjectCookError::Invalid(format!("cooked content target: {error}")))?;
    let implementation = implementation_digest().map_err(ProjectCookError::Invalid)?;
    let store = ArtifactBuildStore::open(cache.share(), ArtifactBuildLimits::default())?;
    let request = ProjectCookRequest {
        root,
        candidate: &candidate,
        recipe,
        target,
        store: &store,
        implementation,
        limits: CookedContentLimits::default(),
    };
    if server {
        let candidate =
            project_settings(&manifest, selection).map_err(ProjectCookError::Invalid)?;
        let recipe = selection.recipe().map_err(ProjectCookError::Invalid)?;
        let omit_components = [
            "nara.render.Camera2d",
            nara::sprite::SPRITE_COMPONENT_ID,
            "nara.tilemap.Tilemap",
            "nara.material2d",
            "two-level-game.ExplorerAnimation",
            "two-level-game.SoundPalette",
        ]
        .map(ComponentTypeId::new);
        return prepare_project_server_content(
            request,
            ProjectServerCookTarget {
                candidate: &candidate,
                recipe,
                omit_components: &omit_components,
            },
            |_| Ok(()),
        );
    }

    #[cfg(feature = "play-flow")]
    let mut round_input =
        (selection.flow() == two_level_game::delivery::GameFlow::Round).then_some(selection);
    prepare_project_cooked_content(request, |cx| {
        #[cfg(feature = "play-flow")]
        if let Some(selection) = round_input.take() {
            let input = cx
                .snapshot()
                .input_action_map()
                .map(|input| input.definition());
            selection
                .validate_round_input(input)
                .map_err(ProjectCookError::Invalid)?;
        }
        cx.add_source_assets::<SoundBankDomain>()?;
        cx.add_source_assets::<SpriteAnimationDomain>()?;
        Ok(())
    })
}

fn project_settings(
    manifest: &FileCapability,
    selection: PlayerSelection,
) -> Result<ProjectSettingsCandidate, String> {
    let candidate = ingest_project_manifest(manifest, selection.manifest_profile())
        .map_err(|error| format!("nara.toml: {error:?}"))?;
    let runtime = selection.runtime();
    if candidate.settings().window.enabled != runtime.uses_window() {
        return Err(format!(
            "nara.toml: the {} Player requires window.enabled={}",
            runtime.name(),
            runtime.uses_window()
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
