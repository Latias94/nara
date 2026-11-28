//! Native player saves for game-defined Schema snapshots.

use std::{
    env,
    fs::{self, File, Metadata},
    io,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use nara_core::vfs::{AssetPath, write_atomic};
use nara_reflect::{ComponentCodecError, Schema};

mod wire;

use wire::{SaveFile, SaveSchema};

/// Stores player snapshots in named `.nsav` files.
///
/// Each file is replaced atomically. Writes through one store are serialized so the backup
/// records the previous usable snapshot. Coordinate writers that use separate stores for the
/// same directory. The caller controls the directory during access, including its ancestors.
#[derive(Debug)]
pub struct SaveStore {
    directory: PathBuf,
    app_directory: Option<PathBuf>,
    keep_backup: bool,
    writes: Mutex<()>,
}

/// The value loaded from a save slot and whether it came from the backup file.
#[derive(Debug)]
pub struct LoadedSave<T> {
    pub value: T,
    pub recovered: bool,
}

impl SaveStore {
    /// Uses the platform's per-user data directory for the given application ID.
    ///
    /// IDs may contain ASCII letters, digits, underscores, hyphens, and interior dots.
    /// Windows device names are rejected on every platform.
    pub fn for_app(app_id: &str) -> Result<Self, SaveError> {
        if !is_portable_id(app_id, b"_-.") {
            return Err(SaveError::InvalidAppId(app_id.to_owned()));
        }
        let app_directory = user_data_directory()
            .ok_or(SaveError::UserDataDirectoryUnavailable)?
            .join(app_id);
        let mut store = Self::in_dir(app_directory.join("saves"));
        store.app_directory = Some(app_directory);
        Ok(store)
    }

    /// Uses a caller-supplied directory, creating it when the first save is written.
    #[must_use]
    pub fn in_dir(path: impl Into<PathBuf>) -> Self {
        Self {
            directory: path.into().components().collect(),
            app_directory: None,
            keep_backup: true,
            writes: Mutex::new(()),
        }
    }

    /// Enables or disables saving a copy of the previous snapshot before each replacement.
    /// Existing backups remain available to [`Self::load_with_backup`].
    #[must_use]
    pub fn with_backup(mut self, enabled: bool) -> Self {
        self.keep_backup = enabled;
        self
    }

    /// Writes a snapshot. Slot names may contain ASCII letters, digits, underscores, and hyphens.
    /// Windows device names are rejected on every platform.
    pub fn save<T: Schema>(&self, slot: &str, value: &T) -> Result<(), SaveError> {
        let path = self.slot_path(slot)?;
        let schema = SaveSchema::of::<T>()?;
        let bytes = wire::encode(schema, &value.encode_postcard()?)?;

        let _write = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        self.prepare_directory(true)?;
        if self.keep_backup {
            let backup = path.with_added_extension("bak");
            check_file_entry(&backup)?;
            match read_save(&path) {
                Ok(previous)
                    if previous.schema != schema
                        || T::decode_postcard(previous.payload()).is_ok() =>
                {
                    publish(&backup, &previous.bytes)?;
                }
                Ok(_) | Err(SaveError::NotFound(_) | SaveError::Corrupt { .. }) => {}
                Err(error) => return Err(error),
            }
        } else {
            check_file_entry(&path)?;
        }
        publish(&path, &bytes)
    }

    /// Reads the primary file and requires an exact Postcard schema match.
    pub fn load<T: Schema>(&self, slot: &str) -> Result<T, SaveError> {
        let path = self.slot_path(slot)?;
        self.prepare_directory(false)?;
        read_save(&path)?.decode(&path, SaveSchema::of::<T>()?)
    }

    /// Tries the backup when the primary file is missing or corrupt.
    ///
    /// Recovery returns the backup value in memory. Both files stay available for inspection
    /// or for a subsequent explicit save. A missing backup returns the primary error.
    pub fn load_with_backup<T: Schema>(&self, slot: &str) -> Result<LoadedSave<T>, SaveError> {
        let path = self.slot_path(slot)?;
        self.prepare_directory(false)?;
        let schema = SaveSchema::of::<T>()?;
        let primary_error = match read_save(&path).and_then(|file| file.decode(&path, schema)) {
            Ok(value) => {
                return Ok(LoadedSave {
                    value,
                    recovered: false,
                });
            }
            Err(
                error @ (SaveError::NotFound(_)
                | SaveError::Corrupt { .. }
                | SaveError::Decode { .. }),
            ) => error,
            Err(error) => return Err(error),
        };

        let backup = path.with_added_extension("bak");
        let file = match read_save(&backup) {
            Err(SaveError::NotFound(_)) => return Err(primary_error),
            result => result?,
        };
        let value = file.decode(&backup, schema)?;
        Ok(LoadedSave {
            value,
            recovered: true,
        })
    }

    /// Reads the primary file and invokes `migrate` for a different schema version after
    /// validating the envelope. A same-version file must match the current schema hash.
    /// Save the returned value explicitly to publish the migrated snapshot.
    pub fn load_with_migration<T: Schema>(
        &self,
        slot: &str,
        migrate: impl FnOnce(u16, &[u8]) -> Result<T, SaveError>,
    ) -> Result<T, SaveError> {
        let path = self.slot_path(slot)?;
        self.prepare_directory(false)?;
        let file = read_save(&path)?;
        let schema = SaveSchema::of::<T>()?;
        if file.schema.version != schema.version {
            migrate(file.schema.version, file.payload())
        } else {
            file.decode(&path, schema)
        }
    }

    fn slot_path(&self, slot: &str) -> Result<PathBuf, SaveError> {
        if !is_portable_id(slot, b"_-") {
            return Err(SaveError::InvalidSlot(slot.to_owned()));
        }
        Ok(self.directory.join(format!("{slot}.nsav")))
    }

    fn prepare_directory(&self, create: bool) -> Result<(), SaveError> {
        let first = self.app_directory.as_deref().unwrap_or(&self.directory);
        if create && let Some(parent) = first.parent().filter(|path| !path.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|error| path_error(parent, error))?;
        }
        if let Some(app_directory) = &self.app_directory {
            check_directory(app_directory, create)?;
        }
        check_directory(&self.directory, create)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SaveError {
    #[error("invalid save slot {0:?}")]
    InvalidSlot(String),
    #[error("invalid save application ID {0:?}")]
    InvalidAppId(String),
    #[error("the platform user data directory could not be determined")]
    UserDataDirectoryUnavailable,
    #[error("save path {} does not exist", .0.display())]
    NotFound(PathBuf),
    #[error("save path {} has an unexpected filesystem entry", .0.display())]
    UnsafePath(PathBuf),
    #[error("could not access save path {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("corrupt save at {}: {reason}", path.display())]
    Corrupt { path: PathBuf, reason: &'static str },
    #[error("unsupported NSAV format version {0}")]
    UnsupportedFormatVersion(u16),
    #[error("save payload has {0} bytes and exceeds 16 MiB")]
    PayloadTooLarge(usize),
    #[error("schema version {0} is outside the NSAV range of 1..=65535")]
    InvalidSchemaVersion(u32),
    #[error("save schema version {found} does not match expected version {expected}")]
    SchemaVersionMismatch { expected: u16, found: u16 },
    #[error("save schema hash does not match the requested Postcard schema")]
    SchemaHashMismatch,
    #[error("save schema or Postcard encoder failed: {0}")]
    Codec(#[from] ComponentCodecError),
    #[error("invalid Postcard payload at {}: {source}", path.display())]
    Decode {
        path: PathBuf,
        #[source]
        source: ComponentCodecError,
    },
    #[error("save migration failed: {0}")]
    Migration(String),
}

fn is_portable_id(value: &str, punctuation: &[u8]) -> bool {
    !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || punctuation.contains(&byte))
        && AssetPath::new(value).is_ok()
}

fn user_data_directory() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        absolute_env_path("LOCALAPPDATA")
    }
    #[cfg(target_os = "macos")]
    {
        absolute_env_path("HOME").map(|home| home.join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        absolute_env_path("XDG_DATA_HOME")
            .or_else(|| absolute_env_path("HOME").map(|home| home.join(".local/share")))
    }
}

fn absolute_env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn check_directory(path: &Path, create: bool) -> Result<(), SaveError> {
    let metadata = match fs::symlink_metadata(path) {
        Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(path_error(path, error)),
            }
            fs::symlink_metadata(path)
        }
        result => result,
    }
    .map_err(|error| path_error(path, error))?;
    if !metadata.is_dir() || is_link(&metadata) {
        return Err(SaveError::UnsafePath(path.to_owned()));
    }
    Ok(())
}

fn check_file_entry(path: &Path) -> Result<(), SaveError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => check_regular_file(path, &metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(path_error(path, error)),
    }
}

fn check_regular_file(path: &Path, metadata: &Metadata) -> Result<(), SaveError> {
    if !metadata.is_file() || is_link(metadata) {
        return Err(SaveError::UnsafePath(path.to_owned()));
    }
    Ok(())
}

fn is_link(metadata: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

fn read_save(path: &Path) -> Result<SaveFile, SaveError> {
    check_file_entry(path)?;
    let file = File::open(path).map_err(|error| path_error(path, error))?;
    let metadata = file.metadata().map_err(|error| path_error(path, error))?;
    check_regular_file(path, &metadata)?;
    SaveFile::read(path, file, metadata.len())
}

fn publish(path: &Path, bytes: &[u8]) -> Result<(), SaveError> {
    write_atomic(path, bytes).map_err(|error| path_error(path, error))
}

fn path_error(path: &Path, source: io::Error) -> SaveError {
    if source.kind() == io::ErrorKind::NotFound {
        SaveError::NotFound(path.to_owned())
    } else {
        SaveError::Io {
            path: path.to_owned(),
            source,
        }
    }
}

#[cfg(test)]
mod tests;
