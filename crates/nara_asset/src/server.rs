use std::{
    any::TypeId,
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_ecs::Resource;

use crate::{
    AssetId, AssetPath, AssetPathError, AssetProductIdentityError, AssetProductRef, AssetRecord,
    Handle, ImportedProductId, StableAssetId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    IdSpaceExhausted,
    InvalidPath(AssetPathError),
    InvalidProductIdentity(AssetProductIdentityError),
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
    AssetIdAlreadyBoundToPath {
        id: AssetId,
        existing_path: AssetPath,
        requested_path: AssetPath,
    },
    AssetIdAlreadyBoundToStableId {
        id: AssetId,
        existing_stable_id: StableAssetId,
        requested_stable_id: StableAssetId,
    },
    UnknownAssetId(AssetId),
    UnknownProduct(AssetProductRef),
    AssetTypeMismatch {
        id: AssetId,
        existing: &'static str,
        requested: &'static str,
    },
}

impl Display for AssetError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdSpaceExhausted => formatter.write_str("asset id space exhausted"),
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
            Self::InvalidProductIdentity(error) => {
                write!(formatter, "invalid asset product identity: {error}")
            }
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
            Self::AssetIdAlreadyBoundToPath {
                id,
                existing_path,
                requested_path,
            } => write!(
                formatter,
                "runtime handle {:?} is already bound to asset path '{existing_path}', not '{requested_path}'",
                id
            ),
            Self::AssetIdAlreadyBoundToStableId {
                id,
                existing_stable_id,
                requested_stable_id,
            } => write!(
                formatter,
                "runtime handle {:?} is already bound to stable asset id '{existing_stable_id}', not '{requested_stable_id}'",
                id
            ),
            Self::UnknownAssetId(id) => write!(formatter, "runtime asset id {:?} is unknown", id),
            Self::UnknownProduct(reference) => {
                write!(
                    formatter,
                    "asset product '{reference}' has no runtime identity"
                )
            }
            Self::AssetTypeMismatch {
                id,
                existing,
                requested,
            } => write!(
                formatter,
                "runtime asset id {:?} is claimed as {existing}, not {requested}",
                id
            ),
        }
    }
}

impl Error for AssetError {}

/// Weak runtime identities and type claims for complete products, source paths and anonymous assets.
///
/// Source paths alias only their source's primary product. Concrete content owners may reserve
/// primary or auxiliary products without authoring paths. A binding proves neither actual content
/// existence, codec compatibility nor residency; those remain the content owner's responsibility.
#[derive(Debug, Clone, PartialEq, Eq, Resource)]
pub struct AssetServer {
    paths: HashMap<AssetPath, AssetId>,
    reverse_paths: HashMap<AssetId, AssetPath>,
    products: HashMap<AssetProductRef, AssetId>,
    reverse_products: HashMap<AssetId, AssetProductRef>,
    type_claims: HashMap<AssetId, (TypeId, &'static str)>,
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
            paths: HashMap::new(),
            reverse_paths: HashMap::new(),
            products: HashMap::new(),
            reverse_products: HashMap::new(),
            type_claims: HashMap::new(),
        }
    }

    pub fn reserve<T: 'static>(
        &mut self,
        path: impl Into<String>,
    ) -> Result<Handle<T>, AssetError> {
        let path = AssetPath::new(path.into()).map_err(AssetError::InvalidPath)?;
        let id = self.reserve_path_id(path)?;
        self.claim(id)
    }

    pub fn reserve_path_id(&mut self, path: AssetPath) -> Result<AssetId, AssetError> {
        if let Some(id) = self.paths.get(&path).copied() {
            return Ok(id);
        }

        let id = AssetId::allocate()?;
        self.bind_path(id, path)?;
        Ok(id)
    }

    /// Reserves a complete product identity and claims its Rust type.
    ///
    /// The concrete content owner must separately validate product membership, codec compatibility
    /// and actual resources. Repeating a reservation with another Rust type rejects without
    /// changing the catalogue. This does not create a source-path alias or retain an asset value.
    pub fn reserve_product<T: 'static>(
        &mut self,
        reference: AssetProductRef,
    ) -> Result<Handle<T>, AssetError> {
        if let Some(id) = self.products.get(&reference).copied() {
            return self.claim(id);
        }

        let id = AssetId::allocate()?;
        self.products.insert(reference, id);
        self.reverse_products.insert(id, reference);
        self.claim(id)
    }

    /// Resolves only an existing complete product identity and checks or establishes its Rust type.
    ///
    /// Unknown products and incompatible type claims reject without changing the catalogue or
    /// allocating an identity. Success proves no content existence, compatibility or residency.
    pub fn resolve_product<T: 'static>(
        &mut self,
        reference: AssetProductRef,
    ) -> Result<Handle<T>, AssetError> {
        let id = self
            .products
            .get(&reference)
            .copied()
            .ok_or(AssetError::UnknownProduct(reference))?;
        self.claim(id)
    }

    /// Reserves the source's primary product; the record proves no auxiliary product membership.
    ///
    /// Nil source identities reject before any path, product or type binding.
    pub fn reserve_record<T: 'static>(
        &mut self,
        record: &AssetRecord,
    ) -> Result<Handle<T>, AssetError> {
        let primary = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
            .map_err(AssetError::InvalidProductIdentity)?;
        if let Some(id) = self
            .paths
            .get(record.path())
            .copied()
            .or_else(|| self.products.get(&primary).copied())
        {
            self.validate_type_claim::<T>(id)?;
        }
        let id = self.reserve_record_id(record)?;
        self.claim(id)
    }

    /// Binds the actual source path and primary product transactionally, rejecting nil sources.
    pub fn reserve_record_id(&mut self, record: &AssetRecord) -> Result<AssetId, AssetError> {
        let primary = AssetProductRef::new(record.stable_id(), ImportedProductId::PRIMARY)
            .map_err(AssetError::InvalidProductIdentity)?;
        let path_id = self.paths.get(record.path()).copied();
        let product_id = self.products.get(&primary).copied();

        let id = match (path_id, product_id) {
            (Some(path_id), Some(product_id)) if path_id != product_id => {
                return Err(AssetError::ConflictingAssetIdentity {
                    path: record.path().clone(),
                    stable_id: record.stable_id(),
                });
            }
            (Some(id), _) | (_, Some(id)) => id,
            (None, None) => AssetId::allocate()?,
        };

        self.validate_path_binding(id, record.path())?;
        self.validate_primary_binding(id, primary)?;
        self.bind_path(id, record.path().clone())?;
        self.products.insert(primary, id);
        self.reverse_products.insert(id, primary);
        Ok(id)
    }

    pub fn reserve_anonymous<T: 'static>(&mut self) -> Result<Handle<T>, AssetError> {
        let id = AssetId::allocate()?;
        self.type_claims
            .insert(id, (TypeId::of::<T>(), std::any::type_name::<T>()));
        Ok(Handle::new(id))
    }

    pub fn claim<T: 'static>(&mut self, id: AssetId) -> Result<Handle<T>, AssetError> {
        self.validate_type_claim::<T>(id)?;
        self.type_claims
            .entry(id)
            .or_insert((TypeId::of::<T>(), std::any::type_name::<T>()));
        Ok(Handle::new(id))
    }

    #[must_use]
    pub fn get_handle<T: 'static>(&self, id: AssetId) -> Option<Handle<T>> {
        self.type_claims
            .get(&id)
            .filter(|(asset_type, _)| *asset_type == TypeId::of::<T>())
            .map(|_| Handle::new(id))
    }

    #[must_use]
    pub(crate) fn id_for_path(&self, path: &AssetPath) -> Option<AssetId> {
        self.paths.get(path).copied()
    }

    #[must_use]
    pub(crate) fn id_for_stable_id(&self, stable_id: StableAssetId) -> Option<AssetId> {
        let primary = AssetProductRef::new(stable_id, ImportedProductId::PRIMARY).ok()?;
        self.products.get(&primary).copied()
    }

    #[must_use]
    pub fn path(&self, id: AssetId) -> Option<&str> {
        self.reverse_paths.get(&id).map(AssetPath::as_str)
    }

    /// Returns the complete product identity, including any auxiliary selection.
    #[must_use]
    pub fn product(&self, id: AssetId) -> Option<AssetProductRef> {
        self.reverse_products.get(&id).copied()
    }

    /// Returns a stable source identity only for its primary product.
    ///
    /// Use [`Self::product`] to preserve an auxiliary selection.
    #[must_use]
    pub fn stable_id(&self, id: AssetId) -> Option<StableAssetId> {
        self.product(id)
            .filter(|reference| reference.product() == ImportedProductId::PRIMARY)
            .map(AssetProductRef::source)
    }

    pub(crate) fn validate_type_claim<T: 'static>(&self, id: AssetId) -> Result<(), AssetError> {
        let known = self.reverse_paths.contains_key(&id)
            || self.reverse_products.contains_key(&id)
            || self.type_claims.contains_key(&id);
        if !known {
            return Err(AssetError::UnknownAssetId(id));
        }
        let requested = TypeId::of::<T>();
        if let Some((existing, existing_name)) = self.type_claims.get(&id)
            && *existing != requested
        {
            return Err(AssetError::AssetTypeMismatch {
                id,
                existing: existing_name,
                requested: std::any::type_name::<T>(),
            });
        }
        Ok(())
    }

    fn bind_path(&mut self, id: AssetId, path: AssetPath) -> Result<(), AssetError> {
        self.validate_path_binding(id, &path)?;
        self.paths.insert(path.clone(), id);
        self.reverse_paths.insert(id, path);
        Ok(())
    }

    fn validate_path_binding(&self, id: AssetId, path: &AssetPath) -> Result<(), AssetError> {
        if let Some(existing_id) = self.paths.get(path).copied()
            && existing_id != id
        {
            return Err(AssetError::PathAlreadyBound {
                path: path.clone(),
                existing_id,
                requested_id: id,
            });
        }
        if let Some(existing_path) = self.reverse_paths.get(&id).cloned()
            && &existing_path != path
        {
            return Err(AssetError::AssetIdAlreadyBoundToPath {
                id,
                existing_path,
                requested_path: path.clone(),
            });
        }

        Ok(())
    }

    fn validate_primary_binding(
        &self,
        id: AssetId,
        primary: AssetProductRef,
    ) -> Result<(), AssetError> {
        if let Some(existing_id) = self.products.get(&primary).copied()
            && existing_id != id
        {
            return Err(AssetError::StableIdAlreadyBound {
                stable_id: primary.source(),
                existing_id,
                requested_id: id,
            });
        }
        // Source reservation selects only a path alias or a primary product, never an auxiliary.
        if let Some(existing_product) = self.product(id)
            && existing_product != primary
        {
            return Err(AssetError::AssetIdAlreadyBoundToStableId {
                id,
                existing_stable_id: existing_product.source(),
                requested_stable_id: primary.source(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
