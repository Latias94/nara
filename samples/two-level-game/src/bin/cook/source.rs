use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use nara::{
    asset_build::ArtifactBuildObservation,
    fs::{ContentDigest, DirectoryCapability, FileIdentity, RelativePath},
    project_tooling::host_fs,
};

const CONTENT: [&str; 10] = [
    "nara.toml",
    "schema/authoring.bundle.json",
    "scenes/meadow.scene.json",
    "scenes/cavern.scene.json",
    "assets/textures/meadow.png",
    "assets/textures/meadow.png.meta",
    "assets/textures/cavern.png",
    "assets/textures/cavern.png.meta",
    "assets/audio/game.soundbank.json",
    "assets/audio/game.soundbank.json.meta",
];
const MAX_BYTES: u64 = 64 * 1024 * 1024;

pub(super) struct ContentFence {
    path: PathBuf,
    root: DirectoryCapability,
    files: BTreeMap<String, (FileIdentity, ContentDigest)>,
    code: Option<Vec<RelativePath>>,
}

impl ContentFence {
    pub fn capture(path: &Path) -> Result<Self, String> {
        let root = host_fs::readable_directory(path).map_err(|error| error.to_string())?;
        let mut files = BTreeMap::new();
        let mut total = 0_u64;
        for name in CONTENT {
            let file = root
                .open_file(&relative(name)?)
                .map_err(|error| error.to_string())?;
            let digest = file.digest(MAX_BYTES).map_err(|error| error.to_string())?;
            total = total
                .checked_add(digest.length())
                .ok_or("content inventory overflow")?;
            if total > MAX_BYTES {
                return Err("content inventory exceeds 64 MiB".into());
            }
            files.insert(name.to_owned(), (file.identity(), digest));
        }
        Ok(Self {
            path: path.to_path_buf(),
            root,
            files,
            code: None,
        })
    }

    pub fn root(&self) -> &DirectoryCapability {
        &self.root
    }

    pub fn bind_raw_inputs(
        &mut self,
        observations: &[ArtifactBuildObservation],
    ) -> Result<(), String> {
        let mut total = self
            .files
            .values()
            .map(|(_, digest)| digest.length())
            .sum::<u64>();
        for observation in observations {
            let ArtifactBuildObservation::Raw {
                path,
                digest: Some(digest),
            } = observation
            else {
                return Err("cooked source imports must report present raw inputs".into());
            };
            let name = path.as_str();
            let digest = digest.content();
            let file = self
                .root
                .open_file(&relative(name)?)
                .map_err(|error| error.to_string())?;
            if file.digest(MAX_BYTES).map_err(|error| error.to_string())? != digest {
                return Err(format!("build input changed: {name}"));
            }
            if let Some((identity, captured)) = self.files.get(name) {
                if file.identity() != *identity || digest != *captured {
                    return Err(format!("content selection changed: {name}"));
                }
            } else {
                total = total
                    .checked_add(digest.length())
                    .ok_or("content inventory overflow")?;
                if total > MAX_BYTES {
                    return Err("content inventory exceeds 64 MiB".into());
                }
                self.files
                    .insert(name.to_owned(), (file.identity(), digest));
            }
        }
        self.verify()
    }

    pub fn bind_code(&mut self, inputs: &[RelativePath]) -> Result<(), String> {
        if self.code.is_some() {
            return Err("code selection is already bound".into());
        }
        let mut total = self
            .files
            .values()
            .map(|(_, digest)| digest.length())
            .sum::<u64>();
        for path in inputs {
            let file = self
                .root
                .open_file(path)
                .map_err(|error| error.to_string())?;
            let digest = file.digest(MAX_BYTES).map_err(|error| error.to_string())?;
            total = total
                .checked_add(digest.length())
                .ok_or("source inventory overflow")?;
            if total > MAX_BYTES {
                return Err("complete source inventory exceeds 64 MiB".into());
            }
            self.files
                .insert(portable(path)?, (file.identity(), digest));
        }
        self.code = Some(inputs.to_vec());
        self.verify()
    }

    pub fn verify(&self) -> Result<(), String> {
        let current = host_fs::readable_directory(&self.path).map_err(|error| error.to_string())?;
        if !self
            .root
            .same_object(&current)
            .map_err(|error| error.to_string())?
        {
            return Err("original project directory changed".into());
        }
        if let Some(code) = &self.code {
            let current = code_inventory(&self.path)?;
            if &current != code {
                return Err("the original code input set changed".into());
            }
        }
        for (name, (identity, digest)) in &self.files {
            let file = self
                .root
                .open_file(&relative(name)?)
                .map_err(|error| error.to_string())?;
            if file.identity() != *identity
                || file.digest(MAX_BYTES).map_err(|error| error.to_string())? != *digest
            {
                return Err(format!("content selection changed: {name}"));
            }
        }
        let current = host_fs::readable_directory(&self.path).map_err(|error| error.to_string())?;
        if !self
            .root
            .same_object(&current)
            .map_err(|error| error.to_string())?
        {
            return Err("original project directory changed".into());
        }
        Ok(())
    }
}

pub(super) fn code_inventory(root: &Path) -> Result<Vec<RelativePath>, String> {
    let mut inputs = Vec::new();
    for name in ["Cargo.toml", "Cargo.lock", "build.rs"] {
        let metadata = fs::symlink_metadata(root.join(name)).map_err(|error| error.to_string())?;
        if linked(&metadata) || !metadata.is_file() {
            return Err(format!("code input is not a regular file: {name}"));
        }
        inputs.push(relative(name)?);
    }
    let mut entries = 0;
    visit(root, "src", 0, &mut entries, &mut inputs)?;
    Ok(inputs)
}

fn portable(path: &RelativePath) -> Result<String, String> {
    path.components()
        .iter()
        .map(|part| {
            part.to_str()
                .ok_or_else(|| "code input requires UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

fn visit(
    root: &Path,
    path: &str,
    depth: usize,
    count: &mut usize,
    inputs: &mut Vec<RelativePath>,
) -> Result<(), String> {
    if depth > 16 {
        return Err("code inventory exceeds 16 directory levels".into());
    }
    let directory = fs::symlink_metadata(root.join(path)).map_err(|error| error.to_string())?;
    if linked(&directory) || !directory.is_dir() {
        return Err(format!(
            "code directory must be an unlinked directory: {path}"
        ));
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(root.join(path)).map_err(|error| error.to_string())? {
        if *count >= 16384 {
            return Err("code inventory exceeds 16384 directory entries".into());
        }
        *count += 1;
        entries.push(entry.map_err(|error| error.to_string())?);
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if inputs.len() >= 1024 {
            return Err("code inventory exceeds 1024 files".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "code filenames require UTF-8")?;
        let path = format!("{path}/{name}");
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if linked(&metadata) {
            return Err(format!("linked code input is unsupported: {path}"));
        }
        if metadata.is_dir() {
            visit(root, &path, depth + 1, count, inputs)?;
        } else if metadata.is_file() {
            inputs.push(relative(&path)?);
        } else {
            return Err(format!("non-regular code input: {path}"));
        }
    }
    Ok(())
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn relative(value: &str) -> Result<RelativePath, String> {
    RelativePath::new(value).map_err(|error| error.to_string())
}
