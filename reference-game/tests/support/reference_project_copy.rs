#![allow(dead_code)]

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct ReferenceProjectCopy {
    path: PathBuf,
}

impl ReferenceProjectCopy {
    pub fn new(label: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "nara-reference-project-{label}-{}-{timestamp}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("reference project copy root must be creatable");

        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        copy_file(source, &path, "nara.toml");
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            copy_tree(&source.join(directory), &path.join(directory))
                .expect("reference project directory must be copyable");
        }

        Self { path }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn file(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for ReferenceProjectCopy {
    fn drop(&mut self) {
        let temp_root = env::temp_dir()
            .canonicalize()
            .expect("system temporary directory must resolve");
        let candidate = self
            .path
            .canonicalize()
            .expect("reference project copy must resolve");
        let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
            .canonicalize()
            .expect("reference project checkout must resolve");
        assert!(
            candidate.starts_with(&temp_root)
                && candidate != temp_root
                && !candidate.starts_with(&checkout),
            "reference project cleanup escaped its owned temporary root: {candidate:?}"
        );
        fs::remove_dir_all(candidate).expect("reference project copy must be removable");
    }
}

fn copy_file(source_root: &Path, destination_root: &Path, relative: &str) {
    let destination = destination_root.join(relative);
    fs::copy(source_root.join(relative), destination)
        .expect("reference project file must be copyable");
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "reference project copy contains a non-file entry",
            ));
        }
    }
    Ok(())
}
