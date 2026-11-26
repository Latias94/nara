//! Expand authored prefab instances without source discovery or filesystem access.

mod address;
mod expansion;

pub use address::{
    PrefabOverrideError, PrefabOverridePath, decode_override_value, encode_override_value,
};

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
};

use nara_core::{AssetId, DocumentId, EntityUuid, vfs::AssetPath};
use nara_reflect::{ComponentSchemaCatalog, ComponentTypeId, ComponentValue};
use thiserror::Error;
use uuid::Uuid;

use super::{SceneComponent, SceneDocument, SceneDocumentError, SceneEntity};

pub const MAX_PREFAB_DEPTH: usize = 32;
pub const MAX_EXPANDED_ENTITIES: usize = 65_536;

type EntityIds = BTreeMap<EntityUuid, EntityUuid>;

#[derive(Debug, Clone, Default, PartialEq)]
struct InstanceEntityIds {
    by_prefab: BTreeMap<(EntityUuid, AssetId), EntityIds>,
    outermost: BTreeMap<EntityUuid, AssetId>,
}

impl InstanceEntityIds {
    fn record_completed(&mut self, instance: EntityUuid, prefab: AssetId, ids: EntityIds) {
        self.by_prefab.insert((instance, prefab), ids);
        self.outermost.insert(instance, prefab);
    }

    fn outermost(&self, instance: EntityUuid) -> Option<&EntityIds> {
        self.by_prefab
            .get(&(instance, *self.outermost.get(&instance)?))
    }
}

/// A flat scene, with source coordinates for the roots and descendants of prefab instances.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandedScene {
    /// The input scene's identity and aliases, with all instances replaced by ordinary entities.
    /// Save the original document for authoring. In particular, opaque components in this output
    /// may still contain raw data scoped to the prefab that authored them.
    pub document: SceneDocument,
    pub provenance: BTreeMap<EntityUuid, PrefabProvenance>,
    /// Hints from the scene and every loaded prefab, indexed by durable ID rather than alias.
    /// When hints disagree, the original scene and then the first visited prefab take precedence.
    pub asset_hints: BTreeMap<AssetId, AssetPath>,
    /// Overrides retained in the input but not applied because the component was opaque.
    pub deferred_overrides: Vec<DeferredPrefabOverride>,
    instance_ids: InstanceEntityIds,
    authored_ids: BTreeSet<EntityUuid>,
}

impl ExpandedScene {
    /// Resolve an address relative to an instance in the original scene, including its root at
    /// the empty path.
    #[must_use]
    pub fn resolve_target(
        &self,
        instance: EntityUuid,
        child_path: &[EntityUuid],
    ) -> Option<&SceneEntity> {
        let target = derive_path(instance, child_path)?;
        let origin = self.provenance.get(&target)?;
        if origin.instance != instance || origin.child_path != child_path {
            return None;
        }
        let entities = &self.document.entities;
        let index = entities
            .binary_search_by_key(&target, |entity| entity.id)
            .ok()?;
        entities.get(index)
    }

    /// Parse an authored override against the components on its target. An explicit
    /// `component::field` boundary selects that component. Opaque component paths remain
    /// available for inspection or Revert when their project schema is unavailable.
    pub fn parse_override(
        &self,
        instance: EntityUuid,
        address: &str,
        schemas: &ComponentSchemaCatalog,
    ) -> Result<PrefabOverridePath, PrefabOverrideError> {
        let (child_path, tail) = address::split_target(address)?;
        let target = self
            .resolve_target(instance, &child_path)
            .ok_or(PrefabOverrideError::MissingTarget)?;
        PrefabOverridePath::parse_target(child_path, tail, schemas, target)
    }

    /// Read the current expanded value of the field an override addresses.
    pub fn resolve_override(
        &self,
        instance: EntityUuid,
        path: &PrefabOverridePath,
    ) -> Result<ComponentValue, PrefabOverrideError> {
        let target = self
            .resolve_target(instance, path.child_path())
            .ok_or(PrefabOverrideError::MissingTarget)?;
        match target.components.get(path.component()) {
            Some(SceneComponent::Known { value, .. }) => Ok(value.read_path(path.field())?),
            Some(SceneComponent::Unknown(_)) => Err(PrefabOverrideError::OpaqueComponent(
                path.component().clone(),
            )),
            None => Err(PrefabOverrideError::MissingComponent(
                path.component().clone(),
            )),
        }
    }

    /// Find the authored UUID of an expanded entity in the outermost prefab mounted at
    /// `instance`. This includes the source root and direct document members, including nested
    /// instance roots.
    #[must_use]
    pub fn local_entity_reference(
        &self,
        instance: EntityUuid,
        expanded: EntityUuid,
    ) -> Option<EntityUuid> {
        local_entity_id(self.instance_ids.outermost(instance)?, expanded)
    }

    /// Find the authored UUID of `target` in the specified prefab mounted at `instance`.
    /// A delegated root can share its expanded UUID with several source roots. Each prefab
    /// identifies its own root and direct document members.
    #[must_use]
    pub fn local_entity_reference_in(
        &self,
        instance: EntityUuid,
        prefab: AssetId,
        target: EntityUuid,
    ) -> Option<EntityUuid> {
        local_entity_id(
            self.instance_ids.by_prefab.get(&(instance, prefab))?,
            target,
        )
    }

    /// Resolve a local UUID in the outermost prefab mounted at `instance`.
    #[must_use]
    pub fn expanded_entity_reference(
        &self,
        instance: EntityUuid,
        local: EntityUuid,
    ) -> Option<EntityUuid> {
        self.instance_ids.outermost(instance)?.get(&local).copied()
    }

    /// Encode a reference for an override declared in this scene on a top-level instance.
    /// Prefer a stable prefab-local UUID when unambiguous, and preserve references into the
    /// scene. `None` means no bare UUID can express the reference in this scope.
    #[must_use]
    pub fn override_entity_reference(
        &self,
        instance: EntityUuid,
        expanded: EntityUuid,
    ) -> Option<EntityUuid> {
        if !self.authored_ids.contains(&instance) {
            return None;
        }
        let ids = self.instance_ids.outermost(instance)?;
        if self.authored_ids.contains(&expanded) {
            return Some(expanded);
        }
        if let Some(local) = local_entity_id(ids, expanded)
            && !self.authored_ids.contains(&local)
        {
            return Some(local);
        }
        if ids
            .get(&expanded)
            .is_none_or(|resolved| *resolved == expanded)
        {
            Some(expanded)
        } else {
            None
        }
    }
}

fn local_entity_id(ids: &EntityIds, target: EntityUuid) -> Option<EntityUuid> {
    ids.iter()
        .find_map(|(local, expanded)| (*expanded == target).then_some(*local))
}

/// Stable editor coordinates relative to an authored instance in the original scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefabProvenance {
    pub instance: EntityUuid,
    /// Each segment selects a local entity inside the current prefab document. A nested instance
    /// opens a new namespace for the next segment. Ordinary parents do not add segments.
    pub child_path: Vec<EntityUuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeferredPrefabOverride {
    /// Final ID of the declaring instance, including when the instance is nested.
    pub instance: EntityUuid,
    /// Final ID of the entity on which the opaque component was found.
    pub target: EntityUuid,
    pub address: String,
    pub component: ComponentTypeId,
}

/// Expand one scene using a caller-supplied loader that addresses prefabs by stable asset ID.
///
/// Each asset is loaded at most once. Output entities are sorted by their final UUID. References
/// are resolved from the document that authored the value out through its enclosing instances.
/// In an instance's added components or overrides, direct prefab-local IDs are also available
/// when no enclosing authored entity has that ID.
pub fn expand_prefabs<E: Display>(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
    load: impl FnMut(AssetId) -> Result<SceneDocument, E>,
) -> Result<ExpandedScene, PrefabError> {
    expand(source, schemas, load, expansion::Mode::Strict)
}

/// Like [`expand_prefabs`], but defer overrides on rows that were loaded as opaque components.
/// Deferred addresses are reported in [`ExpandedScene::deferred_overrides`]. The original raw
/// override stays in `source`. Errors on known components are reported in both modes.
pub fn expand_prefabs_for_editor<E: Display>(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
    load: impl FnMut(AssetId) -> Result<SceneDocument, E>,
) -> Result<ExpandedScene, PrefabError> {
    expand(source, schemas, load, expansion::Mode::Editor)
}

fn expand<E: Display>(
    source: &SceneDocument,
    schemas: &ComponentSchemaCatalog,
    mut load: impl FnMut(AssetId) -> Result<SceneDocument, E>,
    mode: expansion::Mode,
) -> Result<ExpandedScene, PrefabError> {
    expansion::expand(source, schemas, mode, |asset| {
        load(asset).map_err(|error| PrefabError::Load {
            asset,
            message: error.to_string(),
        })
    })
}

#[derive(Debug, Error)]
pub enum PrefabError {
    #[error("could not load prefab asset {asset}: {message}")]
    Load { asset: AssetId, message: String },
    #[error("invalid source document {document}: {source}")]
    InvalidDocument {
        document: DocumentId,
        #[source]
        source: SceneDocumentError,
    },
    #[error("prefab asset {asset} must have exactly one parent-free root, found {roots}")]
    InvalidRootCount { asset: AssetId, roots: usize },
    #[error("prefab asset cycle: {}", .chain.iter().map(ToString::to_string).collect::<Vec<_>>().join(" -> "))]
    Cycle { chain: Vec<AssetId> },
    #[error("prefab asset {asset} exceeds the expansion depth limit of {limit}")]
    DepthLimit { asset: AssetId, limit: usize },
    #[error("prefab expansion exceeds the entity limit of {limit}")]
    EntityLimit { limit: usize },
    #[error("prefab expansion produced duplicate entity UUID {entity}")]
    EntityCollision { entity: EntityUuid },
    #[error("could not derive an entity UUID for prefab instance {instance}, child {child}")]
    InvalidDerivedId {
        instance: EntityUuid,
        child: EntityUuid,
    },
    #[error("prefab instance {instance} both adds and removes component '{component}'")]
    AddRemoveConflict {
        instance: EntityUuid,
        component: ComponentTypeId,
    },
    #[error(
        "prefab instance {instance} adds component '{component}' more than once or already inherits it"
    )]
    DuplicateComponent {
        instance: EntityUuid,
        component: ComponentTypeId,
    },
    #[error("prefab instance {instance}, override '{address}': {source}")]
    Override {
        instance: EntityUuid,
        address: String,
        #[source]
        source: PrefabOverrideError,
    },
}

fn child_id(instance: EntityUuid, child: EntityUuid) -> Option<EntityUuid> {
    EntityUuid::try_from_uuid(Uuid::new_v5(
        &instance.as_uuid(),
        child.as_uuid().as_bytes(),
    ))
    .ok()
}

fn derive_path(instance: EntityUuid, child_path: &[EntityUuid]) -> Option<EntityUuid> {
    if child_path.len() > MAX_PREFAB_DEPTH {
        return None;
    }
    child_path.iter().copied().try_fold(instance, child_id)
}
