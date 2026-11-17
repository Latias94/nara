use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use nara::{
    fs::{
        ContentDigest, DirectoryCapability, DirectoryEnumerationLimits, FileCapability,
        FileIdentity, FileKind, RelativePath,
    },
    project_tooling::{cargo_build::CapturedCargoExecutable, host_fs},
};

const ROOT_FILES: [&str; 4] = ["Cargo.toml", "Cargo.lock", "nara.toml", "build.rs"];
const SOURCE_DIRECTORIES: [&str; 6] = ["src", "assets", "scenes", "prefabs", "schema", "input"];
const MAX_FILES: usize = 1024;
const MAX_DEPTH: usize = 16;
const MAX_ENTRIES: usize = MAX_FILES * MAX_DEPTH;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileObservation {
    identity: FileIdentity,
    digest: ContentDigest,
}

#[derive(PartialEq, Eq)]
enum SourceObservation {
    Directory(FileIdentity),
    File(FileObservation),
}

#[derive(Default)]
struct Inventory {
    entries: BTreeMap<String, SourceObservation>,
    files: usize,
    bytes: u64,
}

/// Admission observations for this game's finite source/content envelope.
///
/// Only the root object remains open between checks. Leaf identities are historical rejection
/// facts, not retained object continuity. Checks permit trusted local editing between safe points
/// and do not establish an atomic snapshot or constrain an already adopted Runtime generation.
pub(super) struct SourceFence {
    project: PathBuf,
    root: DirectoryCapability,
    inventory: Inventory,
    declared_inputs: BTreeMap<String, FileObservation>,
}

impl SourceFence {
    pub(super) fn capture(project: &Path) -> Result<Self, String> {
        if !project.is_absolute() {
            return Err("the source project must be an explicit absolute directory".to_owned());
        }
        require_directory(project)?;
        let root = host_fs::readable_directory(project)
            .map_err(|error| format!("cannot open the source project: {error}"))?;
        let project = project
            .canonicalize()
            .map_err(|error| format!("cannot canonicalize the source project: {error}"))?;
        let mut fence = Self {
            project,
            root,
            inventory: Inventory::default(),
            declared_inputs: BTreeMap::new(),
        };
        fence.verify_root()?;
        fence.inventory = Inventory::capture(&fence.root)?;
        fence.verify()?;
        Ok(fence)
    }

    pub(super) fn verify(&self) -> Result<(), String> {
        self.verify_observations(&self.declared_inputs)
    }

    pub(super) fn bind_build(&mut self, captured: &CapturedCargoExecutable) -> Result<(), String> {
        self.bind_inputs(
            captured
                .provenance()
                .declared_inputs()
                .iter()
                .map(|input| (input.path(), input.digest())),
        )
    }

    fn verify_observations(
        &self,
        inputs: &BTreeMap<String, FileObservation>,
    ) -> Result<(), String> {
        self.verify_root()?;
        let current = Inventory::capture(&self.root)?;
        if let Some(path) = self
            .inventory
            .entries
            .keys()
            .chain(current.entries.keys())
            .find(|path| self.inventory.entries.get(*path) != current.entries.get(*path))
        {
            return Err(format!("the game source inventory changed at {path}"));
        }
        for (path, expected) in inputs {
            let relative = relative_input(path)?;
            let file = self
                .root
                .open_file_exact(&relative)
                .map_err(|error| format!("cannot reopen declared source input {path}: {error}"))?;
            if file.identity() != expected.identity {
                return Err(format!(
                    "the declared source input object changed at {path}"
                ));
            }
            file.verify_digest(expected.digest, expected.digest.length())
                .map_err(|error| format!("the declared source input changed at {path}: {error}"))?;
        }
        self.verify_root()
    }

    fn bind_inputs<'a>(
        &mut self,
        inputs: impl IntoIterator<Item = (&'a str, ContentDigest)>,
    ) -> Result<(), String> {
        self.verify()?;
        let observed = self.observe_inputs(inputs)?;
        self.verify_observations(&observed)?;
        self.declared_inputs = observed;
        Ok(())
    }

    fn observe_inputs<'a>(
        &self,
        inputs: impl IntoIterator<Item = (&'a str, ContentDigest)>,
    ) -> Result<BTreeMap<String, FileObservation>, String> {
        let mut observed = BTreeMap::new();
        let mut files = self.inventory.files;
        let mut bytes = self.inventory.bytes;
        for (path, expected) in inputs {
            if observed.len() >= MAX_FILES {
                return Err("the declared source input count exceeds 1024 files".to_owned());
            }
            let relative = relative_input(path)?;
            if observed.contains_key(path) {
                return Err(format!("the declared source input is repeated at {path}"));
            }
            if expected.length() > MAX_FILE_BYTES {
                return Err(format!(
                    "the declared source input exceeds 64 MiB at {path}"
                ));
            }
            if !self.inventory.entries.contains_key(path) {
                files += 1;
                bytes = bytes
                    .checked_add(expected.length())
                    .ok_or_else(|| "the source input byte allowance overflowed".to_owned())?;
                if files > MAX_FILES || bytes > MAX_TOTAL_BYTES {
                    return Err(
                        "the combined source envelope exceeds 1024 files or 64 MiB".to_owned()
                    );
                }
            }
            let file = self
                .root
                .open_file_exact(&relative)
                .map_err(|error| format!("cannot open declared source input {path}: {error}"))?;
            file.verify_digest(expected, expected.length())
                .map_err(|error| format!("the build source input differs at {path}: {error}"))?;
            observed.insert(
                path.to_owned(),
                FileObservation {
                    identity: file.identity(),
                    digest: expected,
                },
            );
        }
        Ok(observed)
    }

    fn verify_root(&self) -> Result<(), String> {
        require_directory(&self.project)?;
        let current = host_fs::readable_directory(&self.project)
            .map_err(|error| format!("cannot reopen the source project: {error}"))?;
        if !self
            .root
            .same_object(&current)
            .map_err(|error| format!("cannot verify the source project object: {error}"))?
        {
            return Err("the source project root object changed".to_owned());
        }
        Ok(())
    }
}

impl Inventory {
    fn capture(root: &DirectoryCapability) -> Result<Self, String> {
        let mut inventory = Self::default();
        for path in ROOT_FILES {
            let relative = relative_input(path)?;
            let file = root
                .open_file_exact(&relative)
                .map_err(|error| format!("cannot open required source file {path}: {error}"))?;
            inventory.observe_file(path.to_owned(), file)?;
        }
        for path in SOURCE_DIRECTORIES {
            let relative = relative_input(path)?;
            let directory = root.open_directory_exact(&relative).map_err(|error| {
                format!("cannot open required source directory {path}: {error}")
            })?;
            inventory.observe_directory(path.to_owned(), directory, 1)?;
        }
        Ok(inventory)
    }

    fn observe_directory(
        &mut self,
        path: String,
        directory: DirectoryCapability,
        depth: usize,
    ) -> Result<(), String> {
        self.insert(
            path.clone(),
            SourceObservation::Directory(directory.identity()),
        )?;
        let mut cursor = directory
            .begin_child_enumeration(DirectoryEnumerationLimits::new(MAX_ENTRIES, 1024, 1))
            .map_err(|error| format!("cannot enumerate source directory {path}: {error}"))?;
        loop {
            let batch = cursor
                .next_batch(64)
                .map_err(|error| format!("cannot enumerate source directory {path}: {error}"))?;
            if !batch.rejected_entry_facts().is_empty() {
                return Err(format!(
                    "source directory {path} contains an unsupported or changed entry"
                ));
            }
            for entry in batch.entries() {
                if depth >= MAX_DEPTH {
                    return Err(format!(
                        "the source inventory exceeds depth 16 beneath {path}"
                    ));
                }
                let name = entry.name().to_str().ok_or_else(|| {
                    format!("source directory {path} contains a nonportable name")
                })?;
                let child_path = format!("{path}/{name}");
                match entry.kind() {
                    FileKind::Directory => {
                        let child =
                            directory
                                .open_child_directory(entry.name())
                                .map_err(|error| {
                                    format!("cannot open source directory {child_path}: {error}")
                                })?;
                        if entry.identity() != Some(child.identity()) {
                            return Err(format!("the source directory changed at {child_path}"));
                        }
                        self.observe_directory(child_path, child, depth + 1)?;
                    }
                    FileKind::Regular => {
                        let file = directory.open_child_file(entry.name()).map_err(|error| {
                            format!("cannot open source file {child_path}: {error}")
                        })?;
                        if entry.identity() != Some(file.identity()) {
                            return Err(format!("the source file changed at {child_path}"));
                        }
                        self.observe_file(child_path, file)?;
                    }
                    FileKind::Other => {
                        return Err(format!(
                            "the source entry is not a regular file or directory at {child_path}"
                        ));
                    }
                }
            }
            if batch.is_complete() {
                return Ok(());
            }
        }
    }

    fn observe_file(&mut self, path: String, file: FileCapability) -> Result<(), String> {
        if self.files >= MAX_FILES {
            return Err("the game source inventory exceeds 1024 files".to_owned());
        }
        let digest = file
            .digest(MAX_FILE_BYTES.min(MAX_TOTAL_BYTES - self.bytes))
            .map_err(|error| format!("cannot digest source file {path}: {error}"))?;
        self.insert(
            path,
            SourceObservation::File(FileObservation {
                identity: file.identity(),
                digest,
            }),
        )?;
        self.files += 1;
        self.bytes += digest.length();
        Ok(())
    }

    fn insert(&mut self, path: String, observation: SourceObservation) -> Result<(), String> {
        if self.entries.len() >= MAX_ENTRIES {
            return Err(
                "the game source inventory exceeds its directory entry allowance".to_owned(),
            );
        }
        if self.entries.insert(path.clone(), observation).is_some() {
            return Err(format!("the source inventory repeated an entry at {path}"));
        }
        Ok(())
    }
}

fn require_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect the source project directory: {error}"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("the source project must be a physical directory".to_owned());
    }
    Ok(())
}

fn relative_input(path: &str) -> Result<RelativePath, String> {
    let relative = RelativePath::new(path)
        .map_err(|error| format!("invalid project-relative source input {path}: {error}"))?;
    if relative.len() > MAX_DEPTH {
        return Err(format!("the source input exceeds depth 16 at {path}"));
    }
    Ok(relative)
}

#[cfg(test)]
mod tests;
