use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_ecs::Resource;

use crate::{AssetId, AssetPath, AssetPathError, AssetRecord, Handle, StableAssetId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    IdSpaceExhausted,
    InvalidPath(AssetPathError),
    ConflictingAssetIdentity {
        path: AssetPath,
        stable_id: StableAssetId,
    },
    PathAlreadyBound {
        path: AssetPath,
        existing_id: AssetId,
        requested_id: AssetId,
    },
    StableIdAlreadyBound {
        stable_id: StableAssetId,
        existing_id: AssetId,
        requested_id: AssetId,
    },
}

impl Display for AssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdSpaceExhausted => formatter.write_str("asset id space exhausted"),
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
            Self::ConflictingAssetIdentity { path, stable_id } => write!(
                formatter,
                "asset path '{path}' and stable id '{stable_id}' are bound to different runtime handles"
            ),
            Self::PathAlreadyBound {
                path,
                existing_id,
                requested_id,
            } => write!(
                formatter,
                "asset path '{path}' is already bound to runtime handle {:?}, not {:?}",
                existing_id, requested_id
            ),
            Self::StableIdAlreadyBound {
                stable_id,
                existing_id,
                requested_id,
            } => write!(
                formatter,
                "stable asset id '{stable_id}' is already bound to runtime handle {:?}, not {:?}",
                existing_id, requested_id
            ),
        }
    }
}

impl Error for AssetError {}

#[derive(Debug, Resource)]
pub struct AssetServer {
    next_id: u64,
    paths: HashMap<AssetPath, AssetId>,
    reverse_paths: HashMap<AssetId, AssetPath>,
    stable_ids: HashMap<StableAssetId, AssetId>,
    reverse_stable_ids: HashMap<AssetId, StableAssetId>,
}

impl Default for AssetServer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetServer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: 1,
            paths: HashMap::new(),
            reverse_paths: HashMap::new(),
            stable_ids: HashMap::new(),
            reverse_stable_ids: HashMap::new(),
        }
    }

    pub fn reserve<T>(&mut self, path: impl Into<String>) -> Result<Handle<T>, AssetError> {
        let path = AssetPath::new(path.into()).map_err(AssetError::InvalidPath)?;
        if let Some(id) = self.paths.get(&path).copied() {
            return Ok(Handle::new(id));
        }

        let id = self.allocate_id()?;
        self.bind_path(id, path)?;
        Ok(Handle::new(id))
    }

    pub fn reserve_record<T>(&mut self, record: &AssetRecord) -> Result<Handle<T>, AssetError> {
        let path_id = self.paths.get(record.path()).copied();
        let stable_id = self.stable_ids.get(&record.stable_id()).copied();

        let id = match (path_id, stable_id) {
            (Some(path_id), Some(stable_id)) if path_id != stable_id => {
                return Err(AssetError::ConflictingAssetIdentity {
                    path: record.path().clone(),
                    stable_id: record.stable_id(),
                });
            }
            (Some(id), _) | (_, Some(id)) => id,
            (None, None) => self.allocate_id()?,
        };

        self.bind_path(id, record.path().clone())?;
        self.bind_stable_id(id, record.stable_id())?;
        Ok(Handle::new(id))
    }

    pub fn reserve_anonymous<T>(&mut self) -> Result<Handle<T>, AssetError> {
        self.allocate_id().map(Handle::new)
    }

    #[must_use]
    pub fn path(&self, id: AssetId) -> Option<&str> {
        self.reverse_paths.get(&id).map(AssetPath::as_str)
    }

    #[must_use]
    pub fn stable_id(&self, id: AssetId) -> Option<StableAssetId> {
        self.reverse_stable_ids.get(&id).copied()
    }

    fn allocate_id(&mut self) -> Result<AssetId, AssetError> {
        let id = AssetId::from_raw(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(AssetError::IdSpaceExhausted)?;
        Ok(id)
    }

    fn bind_path(&mut self, id: AssetId, path: AssetPath) -> Result<(), AssetError> {
        if let Some(existing_id) = self.paths.get(&path).copied()
            && existing_id != id
        {
            return Err(AssetError::PathAlreadyBound {
                path,
                existing_id,
                requested_id: id,
            });
        }

        self.paths.insert(path.clone(), id);
        self.reverse_paths.insert(id, path);
        Ok(())
    }

    fn bind_stable_id(&mut self, id: AssetId, stable_id: StableAssetId) -> Result<(), AssetError> {
        if let Some(existing_id) = self.stable_ids.get(&stable_id).copied()
            && existing_id != id
        {
            return Err(AssetError::StableIdAlreadyBound {
                stable_id,
                existing_id,
                requested_id: id,
            });
        }

        self.stable_ids.insert(stable_id, id);
        self.reverse_stable_ids.insert(id, stable_id);
        Ok(())
    }
}
