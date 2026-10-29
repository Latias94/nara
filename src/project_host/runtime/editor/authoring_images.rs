use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::{
    AssetId, AssetPath, AssetServer, AssetSlotRevision, AssetStates, AssetVersion, Assets, Handle,
    ImportArtifactDigest, LoadState, SourceHash, StableAssetId,
};
#[cfg(feature = "presentation-ui")]
use nara_image::PreparedImageSnapshot;
use nara_image::{ImageAsset, ImageExtent, ImageFormat, ImageImportSettings};
use nara_ui::__private::{
    UiImageChoice, UiImageChoiceGeneration, UiImageChoiceSet, UiStructureRejection,
};
#[cfg(feature = "presentation-ui")]
use nara_ui::UiImageId;
#[cfg(feature = "presentation-ui")]
use nara_ui_render::UiPreparedImages;
use thiserror::Error;

use super::authoring_runtime::EditorAuthoringRuntime;

const INITIAL_IMAGE_CATALOG_GENERATION: u64 = 1;

/// One immutable authoring image product captured from the Editor's real asset world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EditorAuthoringImageProduct {
    handle: Handle<ImageAsset>,
    stable_id: StableAssetId,
    path: AssetPath,
    version: AssetVersion,
    slot_revision: AssetSlotRevision,
    source_hash: SourceHash,
    artifact_hash: ImportArtifactDigest,
    settings: ImageImportSettings,
    extent: ImageExtent,
    format: ImageFormat,
}

impl EditorAuthoringImageProduct {
    #[must_use]
    pub(super) const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub(super) const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn settings(&self) -> ImageImportSettings {
        self.settings
    }
}

/// One indivisible image catalog publication used by Inspector choices and the UI Canvas.
#[derive(Debug)]
pub(super) struct EditorAuthoringImageCatalogReceipt {
    generation: UiImageChoiceGeneration,
    choices: Arc<UiImageChoiceSet>,
    products: Box<[EditorAuthoringImageProduct]>,
    #[cfg(feature = "presentation-ui")]
    prepared_images: UiPreparedImages,
}

impl EditorAuthoringImageCatalogReceipt {
    #[must_use]
    pub(super) const fn generation(&self) -> UiImageChoiceGeneration {
        self.generation
    }

    #[must_use]
    pub(super) fn share_choices(&self) -> Arc<UiImageChoiceSet> {
        Arc::clone(&self.choices)
    }

    #[must_use]
    pub(super) fn products(&self) -> &[EditorAuthoringImageProduct] {
        &self.products
    }

    #[cfg(feature = "presentation-ui")]
    #[must_use]
    pub(super) fn prepared_images(&self) -> &UiPreparedImages {
        &self.prepared_images
    }
}

/// Prepared successor that remains inert until its exact predecessor publishes it.
#[derive(Debug)]
pub(super) struct EditorAuthoringImageCatalogSuccessor {
    predecessor: UiImageChoiceGeneration,
    receipt: Arc<EditorAuthoringImageCatalogReceipt>,
}

/// Session-local owner of the current authoring image catalog publication.
#[derive(Debug)]
pub(super) struct EditorAuthoringImageHost {
    current: Arc<EditorAuthoringImageCatalogReceipt>,
}

impl EditorAuthoringImageHost {
    pub(super) fn capture(
        runtime: &EditorAuthoringRuntime,
    ) -> Result<Self, EditorAuthoringImageCatalogError> {
        let candidate = runtime.capture_authoring_image_catalog_candidate()?;
        let current = candidate.into_receipt(UiImageChoiceGeneration::new(
            INITIAL_IMAGE_CATALOG_GENERATION,
        ))?;
        Ok(Self {
            current: Arc::new(current),
        })
    }

    pub(super) fn capture_selected(
        runtime: &EditorAuthoringRuntime,
        selected: &BTreeSet<AssetId>,
    ) -> Result<Self, EditorAuthoringImageCatalogError> {
        let candidate = runtime.capture_selected_authoring_image_catalog_candidate(selected)?;
        let current = candidate.into_receipt(UiImageChoiceGeneration::new(
            INITIAL_IMAGE_CATALOG_GENERATION,
        ))?;
        Ok(Self {
            current: Arc::new(current),
        })
    }

    #[must_use]
    pub(super) fn receipt(&self) -> Arc<EditorAuthoringImageCatalogReceipt> {
        Arc::clone(&self.current)
    }

    pub(super) fn prepare_selected_successor(
        &self,
        runtime: &EditorAuthoringRuntime,
        selected: &BTreeSet<AssetId>,
    ) -> Result<Option<EditorAuthoringImageCatalogSuccessor>, EditorAuthoringImageCatalogError>
    {
        let candidate = runtime.capture_selected_authoring_image_catalog_candidate(selected)?;
        if candidate.products.as_slice() == self.current.products() {
            return Ok(None);
        }
        self.prepare_candidate(candidate).map(Some)
    }

    pub(super) fn prepare_replacement_successor(
        &self,
        runtime: &EditorAuthoringRuntime,
    ) -> Result<EditorAuthoringImageCatalogSuccessor, EditorAuthoringImageCatalogError> {
        let candidate = runtime.capture_authoring_image_catalog_candidate()?;
        self.prepare_candidate(candidate)
    }

    fn prepare_candidate(
        &self,
        candidate: EditorAuthoringImageCatalogCandidate,
    ) -> Result<EditorAuthoringImageCatalogSuccessor, EditorAuthoringImageCatalogError> {
        let generation = self
            .current
            .generation()
            .get()
            .checked_add(1)
            .map(UiImageChoiceGeneration::new)
            .ok_or(EditorAuthoringImageCatalogError::GenerationExhausted)?;
        let receipt = Arc::new(candidate.into_receipt(generation)?);
        Ok(EditorAuthoringImageCatalogSuccessor {
            predecessor: self.current.generation(),
            receipt,
        })
    }

    pub(super) fn publish_successor(
        &mut self,
        successor: EditorAuthoringImageCatalogSuccessor,
    ) -> Result<Arc<EditorAuthoringImageCatalogReceipt>, EditorAuthoringImageCatalogError> {
        self.validate_successor(&successor)?;
        self.current = successor.receipt;
        Ok(Arc::clone(&self.current))
    }

    pub(super) fn validate_successor(
        &self,
        successor: &EditorAuthoringImageCatalogSuccessor,
    ) -> Result<(), EditorAuthoringImageCatalogError> {
        if successor.predecessor != self.current.generation() {
            Err(EditorAuthoringImageCatalogError::StaleSuccessor)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub(super) struct EditorAuthoringImageCatalogCandidate {
    products: Vec<EditorAuthoringImageProduct>,
    #[cfg(feature = "presentation-ui")]
    prepared_images: UiPreparedImages,
}

impl EditorAuthoringImageCatalogCandidate {
    pub(super) fn capture(
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
    ) -> Result<Self, EditorAuthoringImageCatalogError> {
        Self::capture_with_selection(server, images, states, None)
    }

    pub(super) fn capture_selected(
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
        selected: &BTreeSet<AssetId>,
    ) -> Result<Self, EditorAuthoringImageCatalogError> {
        Self::capture_with_selection(server, images, states, Some(selected))
    }

    fn capture_with_selection(
        server: &AssetServer,
        images: &Assets<ImageAsset>,
        states: &AssetStates,
        selected: Option<&BTreeSet<AssetId>>,
    ) -> Result<Self, EditorAuthoringImageCatalogError> {
        let mut paths_by_stable_id = BTreeMap::new();
        let mut stable_ids_by_path = BTreeMap::new();
        let mut captured: BTreeMap<(StableAssetId, AssetPath), CapturedAuthoringImage> =
            BTreeMap::new();
        let mut captured_ids = BTreeSet::new();

        for (handle, image) in images.iter() {
            let id = handle.id();
            if selected.is_some_and(|selected| !selected.contains(&id)) {
                continue;
            }
            let Some(path) = server.path(id) else {
                return Err(EditorAuthoringImageCatalogError::IncompleteIdentity { id });
            };
            let Some(stable_id) = server.stable_id(id) else {
                return Err(EditorAuthoringImageCatalogError::IncompleteIdentity { id });
            };
            let source = image
                .source()
                .ok_or(EditorAuthoringImageCatalogError::SourceIdentityMismatch { id })?;
            if source.stable_id() != stable_id || source.path().as_str() != path {
                return Err(EditorAuthoringImageCatalogError::SourceIdentityMismatch { id });
            }
            let Some(state) = states.state(id) else {
                return Err(EditorAuthoringImageCatalogError::IncompleteLoadedState { id });
            };
            if state.load_state() != &LoadState::Loaded
                || state.source_hash() != Some(source.source_hash())
                || state.import_hash() != Some(source.artifact().key().digest())
            {
                return Err(EditorAuthoringImageCatalogError::LoadedStateMismatch { id });
            }

            let path = source.path().clone();
            let product = EditorAuthoringImageProduct {
                handle,
                stable_id,
                path: path.clone(),
                version: state.version(),
                slot_revision: images.slot_revision(handle),
                source_hash: source.source_hash(),
                artifact_hash: source.artifact().key().digest(),
                settings: source.settings(),
                extent: image.extent(),
                format: image.format(),
            };
            if paths_by_stable_id
                .get(&stable_id)
                .is_some_and(|existing| existing != &path)
                || stable_ids_by_path
                    .get(&path)
                    .is_some_and(|existing| existing != &stable_id)
            {
                return Err(EditorAuthoringImageCatalogError::ConflictingIdentity);
            }
            let identity = (stable_id, path.clone());
            if let Some(existing) = captured.get(&identity) {
                if existing.product != product {
                    return Err(EditorAuthoringImageCatalogError::ConflictingIdentity);
                }
                continue;
            }

            paths_by_stable_id.insert(stable_id, path.clone());
            stable_ids_by_path.insert(path.clone(), stable_id);
            captured_ids.insert(id);
            #[cfg(feature = "presentation-ui")]
            let prepared = PreparedImageSnapshot::capture(handle, images, states)
                .ok_or(EditorAuthoringImageCatalogError::LoadedStateMismatch { id })?;
            captured.insert(
                identity,
                CapturedAuthoringImage {
                    product,
                    #[cfg(feature = "presentation-ui")]
                    prepared,
                },
            );
        }

        if let Some(missing) = selected.and_then(|selected| {
            selected
                .iter()
                .find(|id| !captured_ids.contains(id))
                .copied()
        }) {
            return Err(EditorAuthoringImageCatalogError::IncompleteLoadedState { id: missing });
        }

        #[cfg(feature = "presentation-ui")]
        let prepared_images = prepare_image_aliases(captured.values());
        let products = captured.into_values().map(|image| image.product).collect();
        Ok(Self {
            products,
            #[cfg(feature = "presentation-ui")]
            prepared_images,
        })
    }

    fn into_receipt(
        self,
        generation: UiImageChoiceGeneration,
    ) -> Result<EditorAuthoringImageCatalogReceipt, EditorAuthoringImageCatalogError> {
        let choices = self
            .products
            .iter()
            .map(|product| authoring_image_choice(product.stable_id(), product.path()))
            .collect::<Result<Vec<_>, _>>()?;
        let choices = Arc::new(UiImageChoiceSet::new(generation, choices)?);
        Ok(EditorAuthoringImageCatalogReceipt {
            generation,
            choices,
            products: self.products.into_boxed_slice(),
            #[cfg(feature = "presentation-ui")]
            prepared_images: self.prepared_images,
        })
    }
}

pub(super) fn validate_authoring_image_choice(
    stable_id: StableAssetId,
    path: &AssetPath,
) -> Result<(), EditorAuthoringImageCatalogError> {
    authoring_image_choice(stable_id, path).map(drop)
}

fn authoring_image_choice(
    stable_id: StableAssetId,
    path: &AssetPath,
) -> Result<UiImageChoice, EditorAuthoringImageCatalogError> {
    UiImageChoice::new(stable_id.to_string(), path.as_str()).map_err(Into::into)
}

struct CapturedAuthoringImage {
    product: EditorAuthoringImageProduct,
    #[cfg(feature = "presentation-ui")]
    prepared: PreparedImageSnapshot,
}

#[cfg(feature = "presentation-ui")]
fn prepare_image_aliases<'a>(
    images: impl IntoIterator<Item = &'a CapturedAuthoringImage>,
) -> UiPreparedImages {
    let mut prepared_images = UiPreparedImages::new();
    for image in images {
        let path = image.product.path().as_str();
        let stable_id = image.product.stable_id().to_string();
        for alias in [
            path.to_owned(),
            format!("path:{path}"),
            format!("project://{path}"),
            stable_id.clone(),
            format!("stable:{stable_id}"),
        ] {
            if let Ok(id) = UiImageId::new(alias) {
                prepared_images.insert(id, image.prepared.clone());
            }
        }
    }
    prepared_images
}

#[derive(Debug, Error)]
pub(super) enum EditorAuthoringImageCatalogError {
    #[error("the Editor authoring image runtime resources are unavailable")]
    RuntimeUnavailable,
    #[error("image asset {id:?} does not have a complete runtime identity")]
    IncompleteIdentity { id: AssetId },
    #[error("image asset {id:?} disagrees with its runtime identity")]
    SourceIdentityMismatch { id: AssetId },
    #[error("image asset {id:?} does not have an authoritative loaded state")]
    IncompleteLoadedState { id: AssetId },
    #[error("image asset {id:?} disagrees with its authoritative loaded state")]
    LoadedStateMismatch { id: AssetId },
    #[error("the Editor authoring image catalog contains conflicting stable-id or path facts")]
    ConflictingIdentity,
    #[error(transparent)]
    Choice(#[from] UiStructureRejection),
    #[error("the Editor authoring image catalog generation is exhausted")]
    GenerationExhausted,
    #[error("the Editor authoring image catalog successor has an obsolete predecessor")]
    StaleSuccessor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "presentation-ui")]
    #[test]
    fn ui_canvas_source_switch_rejects_invalid_projection_without_reusing_cache() {
        use nara_ecs::World;
        use nara_project::{EffectivePrimaryUiSurface, EffectiveUiSafeArea};
        use nara_reflect::{ComponentTypeId, ComponentValue};
        use nara_scene::{
            PrefabDocument, SceneAuthoringRevision, SceneComponentRecord, SceneEntityId,
            SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
        };
        use nara_tooling::EditorDocumentId;
        use nara_ui::__private::{
            UiAuthoringCanvasProjection, UiAuthoringDiagnostic, project_prefab_authoring,
        };
        use nara_ui::{UiNode, UiRoot};

        use crate::product::AcceptedPrimaryUiSurfaceProfile;
        use crate::project_host::runtime::editor::ui_preview::EditorUiPreviewSession;

        let images = EditorAuthoringImageCatalogCandidate::capture(
            &AssetServer::new(),
            &Assets::<ImageAsset>::default(),
            &AssetStates::default(),
        )
        .unwrap()
        .into_receipt(UiImageChoiceGeneration::new(
            INITIAL_IMAGE_CATALOG_GENERATION,
        ))
        .unwrap();
        let registry = crate::project_host::authoring::first_party_authoring_preview_registry(
            crate::project_host::composition::built_in_schema_providers(),
        )
        .unwrap();
        let mut world = World::new();
        let entity = world.spawn((UiRoot::default(), UiNode::default())).id();
        let encode = |component: &ComponentTypeId| {
            SceneComponentRecord::new(
                registry.schema(component).unwrap().version,
                registry
                    .encode_component(component, &world, entity)
                    .unwrap()
                    .unwrap(),
            )
        };
        let root_id = SceneEntityId::new("shared-local-root").unwrap();
        let root_type = ComponentTypeId::new("nara.ui.UiRoot");
        let node_type = ComponentTypeId::new("nara.ui.UiNode");
        let valid = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(root_id.clone())
            .with_component(root_type.clone(), encode(&root_type))
            .with_component(node_type.clone(), encode(&node_type))]);
        let valid_projection = project_prefab_authoring(&valid).unwrap();
        let mut invalid = valid.clone();
        invalid.entities[0]
            .components
            .get_mut(&root_type)
            .unwrap()
            .value = ComponentValue::Null;
        let invalid_projection = project_prefab_authoring(&invalid)
            .expect("malformed component values remain projectable for authoring");
        let [first_source, second_source] = ["first-canvas", "second-canvas"].map(|name| {
            SourceDocumentKey::named(
                "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
                name,
                SourceDocumentKind::Prefab,
            )
            .unwrap()
        });
        let document = EditorDocumentId::from_raw(1);
        let revision = SceneAuthoringRevision::initial();
        let profile = Some(AcceptedPrimaryUiSurfaceProfile::from_effective(
            EffectivePrimaryUiSurface {
                logical_width: 320,
                logical_height: 180,
                safe_area: EffectiveUiSafeArea::default(),
            },
        ));
        let mut session = EditorUiPreviewSession::new().unwrap();
        let UiAuthoringCanvasProjection::Ready(canvas) = session.prepare(
            document,
            &first_source,
            revision,
            None,
            &valid_projection,
            &images,
            profile,
        ) else {
            panic!("the valid source must populate the Canvas cache");
        };
        assert_eq!(canvas.facts().len(), 1);
        assert_eq!(canvas.facts()[0].entity(), &root_id);
        assert!(session.geometry_generation().is_some());

        let switched = session.prepare(
            document,
            &second_source,
            revision,
            None,
            &invalid_projection,
            &images,
            profile,
        );
        assert_eq!(
            switched,
            UiAuthoringCanvasProjection::Rejected(vec![
                UiAuthoringDiagnostic::canvas_compilation_rejected(),
            ])
        );
        assert_eq!(session.geometry_generation(), None);
        assert!(session.take_candidate().is_none());
    }

    #[test]
    fn authoring_image_choice_preflight_rejects_an_unbounded_project_path() {
        let stable_id = StableAssetId::parse_str("f903fcca-cc66-420d-a962-5724e9e55b58").unwrap();
        let path = AssetPath::new(format!("textures/{}.png", "a".repeat(260))).unwrap();

        assert!(matches!(
            validate_authoring_image_choice(stable_id, &path),
            Err(EditorAuthoringImageCatalogError::Choice(_))
        ));
    }
}
