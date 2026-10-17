use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const TEMP_ROOT_PREFIX: &str = ".nara-supervised-play-canary-";
const DESKTOP_SOURCE_PATH: &str = "src/bin/desktop.rs";
const INJECTED_BUILD_FAILURE: &[u8] =
    b"compile_error!(\"injected supervised Play canary build failure\");\n";
const MAX_SOURCE_FILES: usize = 4_096;
const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

static NEXT_TEMP_ROOT: AtomicU64 = AtomicU64::new(1);

pub(in crate::platform::imgui) struct GuardedSupervisedPlayProject {
    root: PathBuf,
    desktop_source_backup: Option<Vec<u8>>,
}

impl GuardedSupervisedPlayProject {
    pub(in crate::platform::imgui) fn prepare(source_root: &Path) -> io::Result<Self> {
        let source_root = fs::canonicalize(source_root)?;
        if !fs::metadata(&source_root)?.is_dir() {
            return Err(io::Error::other(
                "the supervised Play canary source root is not a directory",
            ));
        }
        let parent = source_root.parent().ok_or_else(|| {
            io::Error::other("the supervised Play canary source root has no parent")
        })?;
        let root = allocate_temp_root(parent)?;
        let copy_result = copy_project_tree(&source_root, &root);
        if let Err(error) = copy_result {
            let _ = fs::remove_dir_all(&root);
            return Err(error);
        }
        Ok(Self {
            root,
            desktop_source_backup: None,
        })
    }

    pub(in crate::platform::imgui) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn inject_desktop_build_failure(&mut self) -> io::Result<()> {
        if self.desktop_source_backup.is_some() {
            return Err(io::Error::other(
                "the supervised Play canary build failure is already injected",
            ));
        }
        let path = self.root.join(DESKTOP_SOURCE_PATH);
        let source = fs::read(&path)?;
        let mut failing = Vec::new();
        failing
            .try_reserve_exact(INJECTED_BUILD_FAILURE.len() + source.len())
            .map_err(|_| {
                io::Error::other("the supervised Play canary build-failure source is too large")
            })?;
        failing.extend_from_slice(INJECTED_BUILD_FAILURE);
        failing.extend_from_slice(&source);
        fs::write(path, failing)?;
        self.desktop_source_backup = Some(source);
        Ok(())
    }

    pub(super) fn restore_desktop_source(&mut self) -> io::Result<()> {
        let source = self.desktop_source_backup.as_ref().ok_or_else(|| {
            io::Error::other("the supervised Play canary build failure was not injected")
        })?;
        fs::write(self.root.join(DESKTOP_SOURCE_PATH), source)?;
        self.desktop_source_backup = None;
        Ok(())
    }
}

impl Drop for GuardedSupervisedPlayProject {
    fn drop(&mut self) {
        if self
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(TEMP_ROOT_PREFIX))
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn allocate_temp_root(parent: &Path) -> io::Result<PathBuf> {
    for _ in 0..32 {
        let nonce = NEXT_TEMP_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = parent.join(format!("{TEMP_ROOT_PREFIX}{}-{nonce}", std::process::id()));
        match fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "the supervised Play canary temporary-root retry budget was exhausted",
    ))
}

fn copy_project_tree(source_root: &Path, destination_root: &Path) -> io::Result<()> {
    let mut pending = vec![(
        source_root.to_path_buf(),
        destination_root.to_path_buf(),
        true,
    )];
    let mut copied_files = 0_usize;
    let mut copied_bytes = 0_u64;

    while let Some((source, destination, top_level)) = pending.pop() {
        for entry in fs::read_dir(&source)? {
            let entry = entry?;
            let name = entry.file_name();
            if top_level
                && name
                    .to_str()
                    .is_some_and(|name| matches!(name, "target" | ".git" | ".nara"))
            {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(io::Error::other(
                    "the supervised Play canary source tree contains a symbolic link",
                ));
            }
            let source_path = entry.path();
            let destination_path = destination.join(&name);
            if file_type.is_dir() {
                fs::create_dir(&destination_path)?;
                pending.push((source_path, destination_path, false));
                continue;
            }
            if !file_type.is_file() {
                return Err(io::Error::other(
                    "the supervised Play canary source tree contains a special file",
                ));
            }
            copied_files = copied_files
                .checked_add(1)
                .filter(|count| *count <= MAX_SOURCE_FILES)
                .ok_or_else(|| {
                    io::Error::other("the supervised Play canary source tree has too many files")
                })?;
            let length = entry.metadata()?.len();
            copied_bytes = copied_bytes
                .checked_add(length)
                .filter(|bytes| *bytes <= MAX_SOURCE_BYTES)
                .ok_or_else(|| {
                    io::Error::other("the supervised Play canary source tree is too large")
                })?;
            fs::copy(source_path, destination_path)?;
        }
    }
    Ok(())
}
