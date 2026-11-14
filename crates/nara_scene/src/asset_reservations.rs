use std::fmt;

use nara_asset::{
    AssetError, AssetProductRef, AssetRef, AssetRefError, AssetReferenceResolver, AssetServer,
    AssetSourceKind, Handle,
};
use nara_ecs::World;
use nara_reflect::{ComponentApplyBatch, ComponentCodecError};

/// Asset identities reserved for one Scene transaction in one exact World.
///
/// This input owns an otherwise empty component batch and a private asset catalogue. It allocates
/// no target entities and stages no component or live resource writes. Its private constructor
/// prevents unrelated component writes from entering a Scene replacement. Dropping it abandons
/// the private catalogue without publishing it or cleaning up the World; allocated process-local
/// identities remain consumed and are never reused. Reservations alone do not establish residency.
///
/// ```compile_fail,E0451
/// use nara_reflect::ComponentApplyBatch;
/// use nara_scene::advanced::SceneAssetReservations;
/// fn inject_component_writes(batch: ComponentApplyBatch) -> SceneAssetReservations {
///     SceneAssetReservations { batch }
/// }
/// ```
#[must_use]
pub struct SceneAssetReservations {
    batch: ComponentApplyBatch,
}

impl SceneAssetReservations {
    /// Captures the originating World and its exact current asset catalogue.
    #[must_use]
    pub fn from_world(world: &World) -> Self {
        Self {
            batch: ComponentApplyBatch::from_world(world),
        }
    }

    /// Reserves a typed identity through the same admitted decode context used by Scene codecs.
    ///
    /// The source must resolve in `asset_resolution` with `expected_kind`. The resulting handle does not
    /// establish residency: a concrete content owner must prepare and commit its resources separately.
    /// No live AssetServer is changed, including when resolution fails.
    pub fn reserve<T: 'static>(
        &mut self,
        reference: &AssetRef,
        asset_resolution: AssetReferenceResolver<'_>,
        expected_kind: &AssetSourceKind,
    ) -> Result<Handle<T>, AssetRefError> {
        self.batch
            .with_decode_context(Some(asset_resolution), |context| {
                context
                    .resolve_asset_ref_with_kind(reference, expected_kind)
                    .expect("Scene asset reservations always install a scratch AssetServer")
            })
    }

    /// Reserves an admitted Runtime Product without source metadata or live World mutation.
    ///
    /// The content owner must prepare actual values through its resource transaction. Native
    /// codecs consume this reservation through their ordinary decode context; unknown references
    /// remain errors rather than implicit reservations. Dropping these reservations abandons the
    /// private catalogue without publishing it; allocated identities remain consumed.
    pub fn reserve_product<T: 'static>(
        &mut self,
        reference: AssetProductRef,
    ) -> Result<Handle<T>, AssetError> {
        self.batch.reserve_product(reference)
    }

    /// Observes the catalogue that component decoding will extend and commit on success.
    #[must_use]
    pub fn staged_server(&self) -> &AssetServer {
        self.batch.staged_asset_server()
    }

    /// Rejects use in another World or after the originating live AssetServer changes.
    pub fn validate_baseline(&self, world: &World) -> Result<(), ComponentCodecError> {
        self.batch.validate_asset_server(world)
    }

    pub(crate) fn into_batch(
        self,
        world: &World,
    ) -> Result<ComponentApplyBatch, ComponentCodecError> {
        self.validate_baseline(world)?;
        Ok(self.batch)
    }
}

impl fmt::Debug for SceneAssetReservations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SceneAssetReservations")
            .finish_non_exhaustive()
    }
}
