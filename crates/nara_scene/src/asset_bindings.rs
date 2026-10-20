use std::fmt;

use nara_asset::{
    AssetRef, AssetRefError, AssetServer, AssetSourceKind, Handle, ProjectAssetDatabase,
};
use nara_ecs::World;
use nara_reflect::{ComponentApplyBatch, ComponentCodecError};

/// Asset identities prepared for one Scene transaction in one exact World.
///
/// The input owns an otherwise empty component batch. It stages no entities, components or live
/// resources. Its private constructor path prevents unrelated component writes from being smuggled
/// into a Scene replacement. Dropping it abandons its private catalogue; allocated process-local
/// identities remain consumed and are never reused.
///
/// ```compile_fail,E0451
/// use nara_reflect::ComponentApplyBatch;
/// use nara_scene::advanced::SceneAssetBindings;
/// fn inject_component_writes(batch: ComponentApplyBatch) -> SceneAssetBindings {
///     SceneAssetBindings { batch }
/// }
/// ```
#[must_use]
pub struct SceneAssetBindings {
    batch: ComponentApplyBatch,
}

impl SceneAssetBindings {
    /// Captures the originating World and its exact current asset catalogue.
    #[must_use]
    pub fn from_world(world: &World) -> Self {
        Self {
            batch: ComponentApplyBatch::from_world(world),
        }
    }

    /// Reserves a typed identity through the same admitted decode context used by Scene codecs.
    ///
    /// The source must resolve in `database` with `expected_kind`. The resulting handle does not
    /// establish residency: a concrete content owner must prepare and commit its resources separately.
    /// No live AssetServer is changed, including when resolution fails.
    pub fn reserve<T: 'static>(
        &mut self,
        reference: &AssetRef,
        database: &ProjectAssetDatabase,
        expected_kind: &AssetSourceKind,
    ) -> Result<Handle<T>, AssetRefError> {
        self.batch.with_decode_context(Some(database), |context| {
            context
                .resolve_asset_ref_with_kind(reference, expected_kind)
                .expect("Scene asset bindings always install a scratch AssetServer")
        })
    }

    /// Observes the catalogue that component decoding will extend and commit on success.
    #[must_use]
    pub fn staged_server(&self) -> &AssetServer {
        self.batch.staged_asset_server()
    }

    /// Rejects use in another World or after the originating live AssetServer changes.
    pub fn validate(&self, world: &World) -> Result<(), ComponentCodecError> {
        self.batch.validate_asset_server(world)
    }

    pub(crate) fn into_batch(
        self,
        world: &World,
    ) -> Result<ComponentApplyBatch, ComponentCodecError> {
        self.validate(world)?;
        Ok(self.batch)
    }
}

impl fmt::Debug for SceneAssetBindings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneAssetBindings")
            .finish_non_exhaustive()
    }
}
