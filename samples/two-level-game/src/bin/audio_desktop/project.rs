use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nara::{
    asset::{AssetMetaIdentityCandidate, AssetPath, AssetSourceKind, SourceAssetImporter},
    fs::{ContentDigest, DirectoryCapability, RelativePath},
    project_host::{
        ProjectContentDeliveryReceipt, ProjectContentMemberKind, ProjectContentMemberReceipt,
        ingest_project_manifest,
    },
    project_workflow::{ProjectCheckRequest, check_project},
};
use serde::Deserialize;
use two_level_game::{
    RunOptions,
    audio::{self, AudioOptions},
    levels::LevelId,
    open_explicit_project_root,
    sound_bank::{SoundBankDomain, bank_product},
};

use super::oracle::{SourceWavs, wav_chunks};

pub(super) const DESKTOP_PROFILE: &str = "desktop";

pub(super) struct ProjectCopy {
    pub directory: PathBuf,
    pub project: PathBuf,
    original: PathBuf,
    sources: CopiedSources,
}

struct CopiedSources {
    scenes: PathBuf,
    meadow: Vec<u8>,
    cavern: Vec<u8>,
    pickup: Vec<u8>,
    travel: Vec<u8>,
    travel_path: String,
}

#[derive(Deserialize)]
struct CuePaths {
    pickup: AssetPath,
    travel: AssetPath,
}

impl ProjectCopy {
    pub(super) fn new(source: &Path) -> Result<Self, String> {
        let root = open_explicit_project_root(source).map_err(|error| error.to_string())?;
        let original = source.canonicalize().map_err(|error| error.to_string())?;
        let recipe = audio::recipe(
            RunOptions::default(),
            AudioOptions {
                capture_output: true,
            },
        );
        let delivery = check_project(
            ProjectCheckRequest::new(root.share(), recipe).with_profile(DESKTOP_PROFILE),
        )
        .map_err(|error| format!("check source verification project: {error:?}"))?;

        let temporary_parent = std::env::temp_dir()
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = temporary_parent.join(format!(
            "two-level-native-audio-{}-{stamp}",
            std::process::id()
        ));
        if directory.starts_with(&original) || original.starts_with(&directory) {
            return Err("temporary verification directory overlaps the source project".into());
        }
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        if directory
            .canonicalize()
            .map_err(|error| error.to_string())?
            != directory
        {
            return Err("temporary verification directory escaped its original TEMP parent".into());
        }
        let project = directory.join("project");
        let sources = match copy_sources(&root, &delivery, &project) {
            Ok(sources) => sources,
            Err(error) => {
                return Err(match remove_directory(&directory, &original) {
                    Ok(()) => error,
                    Err(cleanup) => format!("{error}; private copy cleanup failed: {cleanup}"),
                });
            }
        };
        Ok(Self {
            directory,
            project,
            original,
            sources,
        })
    }

    pub(super) fn wavs(&self) -> SourceWavs<'_> {
        SourceWavs {
            pickup: &self.sources.pickup,
            travel: &self.sources.travel,
        }
    }

    fn scene_path(&self, level: LevelId) -> PathBuf {
        self.sources.scenes.join(level.scene_path().as_str())
    }

    fn scene_bytes(&self, level: LevelId) -> &[u8] {
        match level {
            LevelId::Meadow => &self.sources.meadow,
            LevelId::Cavern => &self.sources.cavern,
        }
    }

    fn copied_path(&self, relative: &Path) -> Result<PathBuf, String> {
        let path = self
            .project
            .join(relative)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&self.project) {
            return Err("copied path escaped its temporary project".into());
        }
        Ok(path)
    }

    pub(super) fn edit_scene_position(
        &self,
        level: LevelId,
        revision: usize,
    ) -> Result<(), String> {
        let path = self.copied_path(&self.scene_path(level))?;
        let mut scene: serde_json::Value =
            serde_json::from_slice(self.scene_bytes(level)).map_err(|error| error.to_string())?;
        let pickup = scene["payload"]["entities"]
            .as_array_mut()
            .and_then(|entities| entities.iter_mut().find(|entity| entity["id"] == "gem-far"))
            .ok_or("copied level lacks the expected authored pickup")?;
        pickup["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]["value"]
            ["x"]["value"] = serde_json::json!(81 + revision);
        let bytes = serde_json::to_vec_pretty(&scene).map_err(|error| error.to_string())?;
        replace_file(&path, &bytes)
    }

    pub(super) fn restore_scene_positions(&self) -> Result<(), String> {
        for level in LevelId::ALL {
            let path = self.copied_path(&self.scene_path(level))?;
            replace_file(&path, self.scene_bytes(level))?;
        }
        Ok(())
    }

    pub(super) fn change_travel_samples(&self, changed: bool) -> Result<(), String> {
        let path = self.copied_path(Path::new(&self.sources.travel_path))?;
        if changed {
            let (_, data) = wav_chunks(&self.sources.travel)?;
            let mut bytes = self.sources.travel.clone();
            let first_pcm_byte = bytes[data]
                .get_mut(8)
                .ok_or("copied WAV lacks its first PCM16 sample")?;
            *first_pcm_byte ^= 1;
            replace_file(&path, &bytes)
        } else {
            replace_file(&path, &self.sources.travel)
        }
    }

    pub(super) fn remove_cavern(&self) -> Result<(), String> {
        let path = self.copied_path(&self.scene_path(LevelId::Cavern))?;
        fs::remove_file(path).map_err(|error| error.to_string())
    }

    pub(super) fn repair_cavern(&self) -> Result<(), String> {
        let directory = self.copied_path(&self.sources.scenes)?;
        write_new_file(
            &directory.join(LevelId::Cavern.scene_path().as_str()),
            &self.sources.cavern,
        )
    }

    pub(super) fn remove(self) -> Result<(), String> {
        remove_directory(&self.directory, &self.original)
    }
}

fn copy_sources(
    source: &DirectoryCapability,
    delivery: &ProjectContentDeliveryReceipt,
    project: &Path,
) -> Result<CopiedSources, String> {
    fs::create_dir(project).map_err(|error| error.to_string())?;
    for member in delivery.members() {
        let bytes = read_member(source, member)?;
        let destination = project.join(member.path().as_str());
        fs::create_dir_all(destination.parent().ok_or("copied file has no parent")?)
            .map_err(|error| error.to_string())?;
        write_new_file(&destination, &bytes)?;
    }

    let copy = open_explicit_project_root(project).map_err(|error| error.to_string())?;
    let manifest = copy
        .open_file(&RelativePath::new("nara.toml").expect("fixed manifest name"))
        .map_err(|error| format!("open copied nara.toml: {error}"))?;
    let settings = ingest_project_manifest(&manifest, Some(DESKTOP_PROFILE))
        .map_err(|error| format!("admit copied nara.toml: {error:?}"))?;
    if settings.manifest_digest() != delivery.manifest_digest() {
        return Err("copied manifest differs from the source delivery".into());
    }
    let paths = &settings.settings().paths;
    for path in [&paths.assets, &paths.scenes, &paths.prefabs] {
        fs::create_dir_all(project.join(path.as_str())).map_err(|error| error.to_string())?;
    }
    let read_scene = |level: LevelId| {
        let path = format!("{}/{}", paths.scenes.as_str(), level.scene_path().as_str());
        read_selected(&copy, delivery, &path, ProjectContentMemberKind::Scene)
    };
    let assets = paths.assets.as_str();
    let asset_prefix = format!("{assets}/");
    let bank = bank_product().source();
    let mut descriptor = None;
    for member in delivery.members() {
        if member.kind() != ProjectContentMemberKind::AssetMetadata
            || !member.path().as_str().starts_with(&asset_prefix)
        {
            continue;
        }
        let metadata = AssetMetaIdentityCandidate::decode_json_bytes(&read_member(&copy, member)?)
            .map_err(|error| format!("read copied metadata {}: {error}", member.path().as_str()))?;
        let identity = metadata.identity();
        if identity.stable_id() == bank
            && matches!(identity.source_kind(), AssetSourceKind::Other(kind) if kind == SoundBankDomain::SOURCE_KIND)
        {
            descriptor = member.path().as_str().strip_suffix(".meta");
            break;
        }
    }
    let read_asset =
        |path: &str| read_selected(&copy, delivery, path, ProjectContentMemberKind::AssetSource);
    let descriptor = descriptor.ok_or("source delivery lacks the game's Sound Bank metadata")?;
    let cues: CuePaths = serde_json::from_slice(&read_asset(descriptor)?)
        .map_err(|error| format!("read copied Sound Bank descriptor {descriptor}: {error}"))?;
    let pickup_path = format!("{assets}/{}", cues.pickup.as_str());
    let travel_path = format!("{assets}/{}", cues.travel.as_str());
    Ok(CopiedSources {
        scenes: PathBuf::from(paths.scenes.as_str()),
        meadow: read_scene(LevelId::Meadow)?,
        cavern: read_scene(LevelId::Cavern)?,
        pickup: read_asset(&pickup_path)?,
        travel: read_asset(&travel_path)?,
        travel_path,
    })
}

fn read_selected(
    root: &DirectoryCapability,
    delivery: &ProjectContentDeliveryReceipt,
    path: &str,
    kind: ProjectContentMemberKind,
) -> Result<Vec<u8>, String> {
    let member = delivery
        .members()
        .iter()
        .find(|member| member.kind() == kind && member.path().as_str() == path)
        .ok_or_else(|| format!("source delivery lacks {path}"))?;
    read_member(root, member)
}

fn read_member(
    root: &DirectoryCapability,
    member: &ProjectContentMemberReceipt,
) -> Result<Vec<u8>, String> {
    let name = member.path().as_str();
    let path =
        RelativePath::new(name).map_err(|error| format!("invalid member {name}: {error}"))?;
    let bytes = root
        .open_file(&path)
        .and_then(|file| file.read_to_end_bounded(member.byte_len()))
        .map_err(|error| format!("read admitted member {name}: {error}"))?;
    if ContentDigest::of_bytes(&bytes) != member.digest() {
        return Err(format!(
            "admitted member {name} changed during source capture"
        ));
    }
    Ok(bytes)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| error.to_string())?;
    verify_file(path, bytes)
}

fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| error.to_string())?;
    verify_file(path, bytes)
}

fn verify_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if fs::read(path).map_err(|error| error.to_string())? != bytes {
        return Err("temporary project file differs from the requested bytes".into());
    }
    Ok(())
}

fn remove_directory(directory: &Path, original: &Path) -> Result<(), String> {
    let actual = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if actual != directory || original.starts_with(&actual) || actual.starts_with(original) {
        return Err("refusing to remove an unverified temporary project directory".into());
    }
    fs::remove_dir_all(actual).map_err(|error| error.to_string())
}
