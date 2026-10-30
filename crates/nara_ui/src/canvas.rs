//! Renderer-neutral compilation of one persistent UI Prefab for Editor authoring.

use std::{collections::BTreeMap, sync::Arc};

use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_scene::{SceneComponentRecord, SceneEntityId, SceneOccurrenceKey, SourceDocumentKey};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiEnvironment,
    UiEnvironmentError, UiFrame, UiKey, UiKeyError, UiLocalKey, UiOpaqueId, UiRootKey,
    UiRootStatus, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
    UiTheme as KernelTheme,
};
use nara_ui_widgets::{UiButton as KernelButton, UiButtonState};
use thiserror::Error;

use crate::authoring_model::{
    UiAuthoringCanvasFact, UiAuthoringCanvasReady, UiAuthoringComponentKind,
    UiAuthoringHierarchyNode, UiAuthoringProjection,
};
use crate::persistent_key::{local_identity, source_context};
use crate::{authoring::resolve_controlled_button_visual, codec};

const AUTHORING_CANVAS_SURFACE_ID: u64 = 1;
const AUTHORING_CANVAS_FONT_REVISION: u64 = 1;

/// Exact renderer-neutral environment requested by one private authoring Canvas owner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiAuthoringCanvasCompileRequest {
    content_transform: UiSurfaceContentTransform,
    generation: u64,
}

impl UiAuthoringCanvasCompileRequest {
    #[must_use]
    pub const fn new(content_transform: UiSurfaceContentTransform, generation: u64) -> Self {
        Self {
            content_transform,
            generation,
        }
    }
}

/// One immutable authoring frame and its exact source-local Canvas facts.
#[derive(Debug)]
pub struct UiAuthoringCanvasCandidate {
    frame: Arc<UiFrame>,
    canvas: UiAuthoringCanvasReady,
}

impl UiAuthoringCanvasCandidate {
    #[must_use]
    pub fn frame(&self) -> &Arc<UiFrame> {
        &self.frame
    }

    #[must_use]
    pub const fn canvas(&self) -> &UiAuthoringCanvasReady {
        &self.canvas
    }
}

/// Stateful owner of bounded UI compilation and last-good kernel caches for one Editor Canvas.
pub struct UiAuthoringCanvasCompiler {
    compiler: UiCompiler,
}

impl UiAuthoringCanvasCompiler {
    pub fn new() -> Result<Self, UiAuthoringCanvasCompileError> {
        let font = bundled_font_face()
            .map_err(|error| UiAuthoringCanvasCompileError::BundledFont(error.to_string()))?;
        Ok(Self {
            compiler: UiCompiler::new(font, Default::default())?,
        })
    }

    pub fn compile(
        &mut self,
        projection: &UiAuthoringProjection,
        source: &SourceDocumentKey,
        occurrence: &SceneOccurrenceKey,
        source_revision: u64,
        request: UiAuthoringCanvasCompileRequest,
        assets: &mut ComponentDecodeContext<'_>,
    ) -> Result<UiAuthoringCanvasCandidate, UiAuthoringCanvasCompileError> {
        let context = source_context(source, occurrence)?;
        let roots = projection
            .hierarchy()
            .iter()
            .filter(|node| node.parent().is_none())
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Err(UiAuthoringCanvasCompileError::MissingUiRoot);
        }

        let theme = select_theme(projection)?;
        let surface_id = UiSurfaceId::new(AUTHORING_CANVAS_SURFACE_ID)
            .expect("the private authoring Canvas surface identity is non-zero");
        let generations = UiSurfaceGenerations::new(
            request.generation,
            request.generation,
            request.generation,
            request.generation,
        )?;
        let environment = UiEnvironment::for_surface(
            surface_id,
            request.content_transform,
            generations,
            theme.revision().get(),
            AUTHORING_CANVAS_FONT_REVISION,
        )?;
        let mut source_by_key = BTreeMap::new();
        let mut builder = UiAuthoringSnapshotBuilder::new(source_revision);
        for root in roots {
            builder = builder.with_root(build_root(
                root,
                &context,
                projection,
                &mut source_by_key,
                assets,
            )?);
        }
        let snapshot = builder.build()?;
        let frame = self.compiler.compile(&snapshot, environment, theme)?;
        if frame
            .root_outcomes()
            .iter()
            .any(|outcome| outcome.status() != UiRootStatus::Published)
        {
            return Err(UiAuthoringCanvasCompileError::RootCompilationRejected);
        }

        let mut facts = Vec::with_capacity(source_by_key.len());
        for (paint_order, layout) in frame.layouts().iter().enumerate() {
            let Some(entity) = layout.key().and_then(|key| source_by_key.get(key)).cloned() else {
                continue;
            };
            facts.push(UiAuthoringCanvasFact::new(
                entity,
                layout.rect(),
                layout.clip_rect(),
                u32::try_from(paint_order)
                    .map_err(|_| UiAuthoringCanvasCompileError::FactLimitExceeded)?,
                layout.visible(),
            ));
        }
        if facts.len() != source_by_key.len() {
            return Err(UiAuthoringCanvasCompileError::SourceFactsIncomplete {
                expected: source_by_key.len(),
                actual: facts.len(),
            });
        }
        let canvas = UiAuthoringCanvasReady::new(request.content_transform, facts);
        Ok(UiAuthoringCanvasCandidate { frame, canvas })
    }
}

fn select_theme(
    projection: &UiAuthoringProjection,
) -> Result<KernelTheme, UiAuthoringCanvasCompileError> {
    let mut selected = None;
    for node in projection.hierarchy() {
        let Some(theme) = decode_optional(
            projection,
            node.entity(),
            UiAuthoringComponentKind::Theme,
            codec::decode_theme,
        )?
        else {
            continue;
        };
        match selected {
            None => selected = Some(theme),
            Some(current) if current == theme => {}
            Some(_) => return Err(UiAuthoringCanvasCompileError::ThemeConflict),
        }
    }
    Ok(selected.unwrap_or_default().kernel())
}

fn build_root(
    node: &UiAuthoringHierarchyNode,
    context: &UiOpaqueId,
    projection: &UiAuthoringProjection,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
    assets: &mut ComponentDecodeContext<'_>,
) -> Result<nara_ui_kernel::UiRoot, UiAuthoringCanvasCompileError> {
    let entity = node.entity();
    if component(projection, entity, UiAuthoringComponentKind::Button).is_some() {
        return Err(UiAuthoringCanvasCompileError::RootButtonUnsupported {
            entity: entity.clone(),
        });
    }
    let root = decode_required(
        projection,
        entity,
        UiAuthoringComponentKind::Root,
        codec::decode_root,
    )?;
    let authored_node = decode_required(
        projection,
        entity,
        UiAuthoringComponentKind::Node,
        codec::decode_node,
    )?;
    let root_key = UiRootKey::persistent(local_identity(context, entity)?)?;
    let source_key = UiKey::root(root_key.clone())?;
    insert_source_key(source_by_key, source_key, entity)?;

    let mut authored =
        nara_ui_kernel::UiRoot::new(root_key.clone(), root.layer(), authored_node.style());
    if let Some(visual) = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::Visual,
        |value| codec::decode_visual_with_context(value, assets),
    )? {
        authored = authored.with_visual(visual.kernel());
    }
    if let Some(text) = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::Text,
        codec::decode_text,
    )? {
        authored = authored.with_text(text.kernel());
    }
    if let Some(semantics) = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::Semantics,
        codec::decode_semantics,
    )? {
        authored = authored.with_semantics(semantics.kernel());
    }
    let children = build_children(
        entity,
        context,
        projection,
        &root_key,
        source_by_key,
        assets,
    )?;
    Ok(authored.with_children(children))
}

fn build_children(
    parent: &SceneEntityId,
    context: &UiOpaqueId,
    projection: &UiAuthoringProjection,
    root_key: &UiRootKey,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
    assets: &mut ComponentDecodeContext<'_>,
) -> Result<Vec<nara_ui_kernel::UiNode>, UiAuthoringCanvasCompileError> {
    let mut children = Vec::new();
    for child in projection
        .hierarchy()
        .iter()
        .filter(|child| child.parent() == Some(parent))
    {
        children.push(build_node(
            child,
            context,
            projection,
            root_key,
            source_by_key,
            assets,
        )?);
    }
    Ok(children)
}

fn build_node(
    hierarchy_node: &UiAuthoringHierarchyNode,
    context: &UiOpaqueId,
    projection: &UiAuthoringProjection,
    root_key: &UiRootKey,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
    assets: &mut ComponentDecodeContext<'_>,
) -> Result<nara_ui_kernel::UiNode, UiAuthoringCanvasCompileError> {
    let entity = hierarchy_node.entity();
    let node = decode_required(
        projection,
        entity,
        UiAuthoringComponentKind::Node,
        codec::decode_node,
    )?;
    let local_key = UiLocalKey::persistent(local_identity(context, entity)?)?;
    let source_key = UiKey::scoped(root_key.clone(), local_key.clone())?;
    insert_source_key(source_by_key, source_key.clone(), entity)?;
    let children = build_children(entity, context, projection, root_key, source_by_key, assets)?;

    let Some(button) = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::Button,
        codec::decode_button,
    )?
    else {
        let mut authored = nara_ui_kernel::UiNode::new(node.style()).with_key(local_key);
        if let Some(visual) = decode_optional(
            projection,
            entity,
            UiAuthoringComponentKind::Visual,
            |value| codec::decode_visual_with_context(value, assets),
        )? {
            authored = authored.with_visual(visual.kernel());
        }
        if let Some(text) = decode_optional(
            projection,
            entity,
            UiAuthoringComponentKind::Text,
            codec::decode_text,
        )? {
            authored = authored.with_text(text.kernel());
        }
        if let Some(semantics) = decode_optional(
            projection,
            entity,
            UiAuthoringComponentKind::Semantics,
            codec::decode_semantics,
        )? {
            authored = authored.with_semantics(semantics.kernel());
        }
        return Ok(authored.with_children(children));
    };

    if component(projection, entity, UiAuthoringComponentKind::Text).is_some()
        || component(projection, entity, UiAuthoringComponentKind::Semantics).is_some()
    {
        return Err(UiAuthoringCanvasCompileError::ButtonAuthoringConflict {
            entity: entity.clone(),
        });
    }
    let visual = decode_required(
        projection,
        entity,
        UiAuthoringComponentKind::Visual,
        |value| codec::decode_visual_with_context(value, assets),
    )?;
    let visuals = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::ButtonVisuals,
        codec::decode_button_visuals,
    )?
    .unwrap_or_default();
    let description = KernelButton::new(source_key, button.label(), button.action().clone())?
        .with_enabled(button.enabled());
    let (visual, text_brush) =
        resolve_controlled_button_visual(&visual, visuals, UiButtonState::default(), &description)?;
    Ok(description
        .authored_node(
            root_key,
            node.style(),
            visual,
            text_brush,
            button.logical_text_size(),
        )?
        .with_children(children))
}

fn insert_source_key(
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
    key: UiKey,
    entity: &SceneEntityId,
) -> Result<(), UiAuthoringCanvasCompileError> {
    if source_by_key.insert(key, entity.clone()).is_some() {
        return Err(UiAuthoringCanvasCompileError::DuplicateCanvasKey {
            entity: entity.clone(),
        });
    }
    Ok(())
}

fn component<'a>(
    projection: &'a UiAuthoringProjection,
    entity: &SceneEntityId,
    kind: UiAuthoringComponentKind,
) -> Option<&'a SceneComponentRecord> {
    projection.component_record(entity, kind)
}

fn decode_required<T>(
    projection: &UiAuthoringProjection,
    entity: &SceneEntityId,
    kind: UiAuthoringComponentKind,
    decode: impl FnOnce(&ComponentValue) -> Result<T, ComponentCodecError>,
) -> Result<T, UiAuthoringCanvasCompileError> {
    let component = kind.component_type();
    let record = projection.component_record(entity, kind).ok_or_else(|| {
        UiAuthoringCanvasCompileError::MissingRequiredComponent {
            entity: entity.clone(),
            component: component.clone(),
        }
    })?;
    decode_record(entity, component, record, decode)
}

fn decode_optional<T>(
    projection: &UiAuthoringProjection,
    entity: &SceneEntityId,
    kind: UiAuthoringComponentKind,
    decode: impl FnOnce(&ComponentValue) -> Result<T, ComponentCodecError>,
) -> Result<Option<T>, UiAuthoringCanvasCompileError> {
    let component = kind.component_type();
    projection
        .component_record(entity, kind)
        .map(|record| decode_record(entity, component, record, decode))
        .transpose()
}

fn decode_record<T>(
    entity: &SceneEntityId,
    component: ComponentTypeId,
    record: &SceneComponentRecord,
    decode: impl FnOnce(&ComponentValue) -> Result<T, ComponentCodecError>,
) -> Result<T, UiAuthoringCanvasCompileError> {
    let expected = codec::ui_component_schema_version(&component).ok_or_else(|| {
        UiAuthoringCanvasCompileError::MissingComponentSchema {
            component: component.clone(),
        }
    })?;
    if record.version != expected {
        return Err(UiAuthoringCanvasCompileError::ComponentVersionMismatch {
            entity: entity.clone(),
            component,
            expected,
            actual: record.version,
        });
    }
    decode(&record.value).map_err(|source| UiAuthoringCanvasCompileError::ComponentDecode {
        entity: entity.clone(),
        component,
        source,
    })
}

#[derive(Debug, Error)]
pub enum UiAuthoringCanvasCompileError {
    #[error("the bundled UI font could not be loaded: {0}")]
    BundledFont(String),
    #[error("the Prefab does not contain a UI root")]
    MissingUiRoot,
    #[error("the Prefab hierarchy contains a missing entity")]
    MissingEntity,
    #[error("the Prefab hierarchy is cyclic, duplicated, or incomplete")]
    InvalidHierarchy,
    #[error("UI root {entity} cannot own a controlled button")]
    RootButtonUnsupported { entity: SceneEntityId },
    #[error("UI node {entity} has conflicting controlled-button text or semantics")]
    ButtonAuthoringConflict { entity: SceneEntityId },
    #[error("UI entity {entity} is missing required component {component}")]
    MissingRequiredComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    #[error("the UI Canvas has no current schema for component {component}")]
    MissingComponentSchema { component: ComponentTypeId },
    #[error(
        "UI entity {entity} component {component} has version {actual:?}; version {expected:?} is required"
    )]
    ComponentVersionMismatch {
        entity: SceneEntityId,
        component: ComponentTypeId,
        expected: ComponentSchemaVersion,
        actual: ComponentSchemaVersion,
    },
    #[error("UI entity {entity} component {component} could not be decoded")]
    ComponentDecode {
        entity: SceneEntityId,
        component: ComponentTypeId,
        #[source]
        source: ComponentCodecError,
    },
    #[error("the UI Prefab contains conflicting theme components")]
    ThemeConflict,
    #[error("UI source node {entity} produced a duplicate Canvas key")]
    DuplicateCanvasKey { entity: SceneEntityId },
    #[error("the UI compiler rejected one root-local Canvas body")]
    RootCompilationRejected,
    #[error("the UI Canvas fact count exceeds its u32 order identity")]
    FactLimitExceeded,
    #[error("the UI Canvas published {actual} source facts but expected {expected}")]
    SourceFactsIncomplete { expected: usize, actual: usize },
    #[error(transparent)]
    Key(#[from] UiKeyError),
    #[error(transparent)]
    Authoring(#[from] UiAuthoringError),
    #[error(transparent)]
    Environment(#[from] UiEnvironmentError),
    #[error(transparent)]
    Style(#[from] nara_ui_kernel::UiStyleError),
    #[error(transparent)]
    Widget(#[from] nara_ui_widgets::UiWidgetError),
    #[error(transparent)]
    Compile(#[from] UiCompileError),
}

#[cfg(test)]
mod tests {
    use nara_scene::{PrefabDocument, SceneComponentRecord, SceneEntityId, SceneEntityRecord};
    use nara_ui_kernel::{
        UiColor, UiPoint, UiRect, UiSafeAreaInsets, UiSize, UiStyle, UiSurfaceContentTransform,
    };

    use super::*;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn insert(
        entity: SceneEntityRecord,
        kind: UiAuthoringComponentKind,
        value: Result<ComponentValue, ComponentCodecError>,
    ) -> SceneEntityRecord {
        let component = kind.component_type();
        let version = codec::ui_component_schema_version(&component)
            .expect("first-party Canvas component must have a current schema");
        entity.with_component(
            component,
            SceneComponentRecord::new(version, value.unwrap()),
        )
    }

    #[test]
    fn persistent_prefab_compiles_exact_source_local_canvas_facts() {
        let root_id = id("root");
        let child_id = id("child");
        let root = insert(
            insert(
                SceneEntityRecord::new(root_id.clone()).with_children([child_id.clone()]),
                UiAuthoringComponentKind::Root,
                codec::encode_root(&crate::UiRoot::default()),
            ),
            UiAuthoringComponentKind::Node,
            codec::encode_node(&crate::UiNode::default()),
        );
        let child = insert(
            insert(
                SceneEntityRecord::new(child_id.clone()),
                UiAuthoringComponentKind::Node,
                codec::encode_node(&crate::UiNode::new(UiStyle::absolute(
                    12.0, 8.0, 80.0, 24.0,
                ))),
            ),
            UiAuthoringComponentKind::Visual,
            codec::encode_visual(&crate::UiVisual::solid(UiColor::rgba(0.2, 0.4, 0.8, 1.0))),
        );
        let prefab = PrefabDocument::new([root_id.clone()], [root, child]);
        let projection = crate::authoring_model::project_prefab_authoring(&prefab).unwrap();
        let logical_size = UiSize::new(320.0, 180.0).unwrap();
        let request = UiAuthoringCanvasCompileRequest::new(
            UiSurfaceContentTransform::direct(logical_size, 1.0).unwrap(),
            1,
        );

        let source = SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "canvas",
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap();
        let occurrence = SceneOccurrenceKey::new("tests", "canvas").unwrap();
        let mut compiler = UiAuthoringCanvasCompiler::new().unwrap();
        let mut assets =
            ComponentDecodeContext::new().with_asset_path_scope(&nara_asset::AssetPathScope::ROOT);
        let candidate = compiler
            .compile(&projection, &source, &occurrence, 9, request, &mut assets)
            .unwrap();

        assert_eq!(candidate.frame().source_revision().get(), 9);
        assert_eq!(candidate.canvas().logical_size(), logical_size);
        assert_eq!(candidate.canvas().facts().len(), 2);
        assert!(candidate.canvas().facts().iter().any(|fact| {
            fact.entity() == &child_id
                && fact.rect().origin().x() == 12.0
                && fact.rect().origin().y() == 8.0
                && fact.rect().size().width() == 80.0
                && fact.rect().size().height() == 24.0
        }));

        let first_root = candidate.frame().root_outcomes()[0].key();
        let rebuilt = compiler
            .compile(&projection, &source, &occurrence, 10, request, &mut assets)
            .unwrap();
        assert_eq!(first_root, rebuilt.frame().root_outcomes()[0].key());
        let other_slot = SceneOccurrenceKey::new("tests", "other").unwrap();
        let other = compiler
            .compile(&projection, &source, &other_slot, 10, request, &mut assets)
            .unwrap();
        assert_ne!(first_root, other.frame().root_outcomes()[0].key());
        let other_source = SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "other-canvas",
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap();
        let switched = compiler
            .compile(
                &projection,
                &other_source,
                &occurrence,
                10,
                request,
                &mut assets,
            )
            .unwrap();
        assert_ne!(first_root, switched.frame().root_outcomes()[0].key());

        let mut app = nara_app::App::new();
        app.add_plugins((
            nara_reflect::ComponentRegistryPlugin,
            nara_hierarchy::HierarchyPlugin,
            nara_input::InputPlugin,
            crate::UiPlugin::default(),
        ))
        .unwrap();
        let surface = UiSurfaceId::new(7).unwrap();
        crate::install_ui_surface(
            app.world_mut().unwrap(),
            crate::UiSurfaceSlot::new(
                surface,
                crate::UiSurfaceEndpoint::new(
                    nara_input::InputWindowId::new(11),
                    nara_input::InputSeatId::PRIMARY,
                    nara_input::InputPointerId::PRIMARY,
                ),
                UiSurfaceContentTransform::direct(logical_size, 1.0).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        app.add_systems(
            nara_app::StartupStage::Scene,
            move |world: &mut nara_ecs::World| {
                let registry = nara_reflect::ComponentRegistry::from_snapshot(
                    nara_reflect::component_registry(world)
                        .unwrap()
                        .snapshot()
                        .unwrap(),
                );
                let bound = nara_scene::ExpandedSceneCandidate::plain(
                    source.clone(),
                    Arc::new(prefab.instantiate()),
                );
                let report = nara_scene::spawn_scene(world, &registry, &bound, &occurrence, None);
                assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            },
        )
        .unwrap();
        let mut runtime = nara_app::RuntimeInstance::start(app).unwrap();
        runtime.drive(std::time::Duration::ZERO).unwrap();
        let frame = crate::latest_ui_frame(runtime.world(), surface).unwrap();
        runtime.finish().unwrap();
        let frame = frame.expect("the equal-coordinate Runtime projection must compile");
        let keys = |frame: &UiFrame| {
            frame
                .layouts()
                .iter()
                .filter_map(|layout| layout.key().cloned())
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(keys(candidate.frame()), keys(&frame));
    }

    #[test]
    fn canvas_binds_root_node_and_button_images_through_actual_context() {
        use nara_asset::{
            AssetEvents, AssetPathScope, AssetProductRef, AssetRecord, AssetRef, AssetServer,
            AssetSourceKind, AssetSourcePath, AssetStates, Assets, ImportedProductId,
            ProjectAssetDatabase, StableAssetId,
        };
        use nara_image::{
            ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, PreparedImageSnapshot,
        };
        use nara_ui_kernel::{UiActionId, UiImageId, UiLayer, UiPaintPrimitive, UiResolvedBrush};
        let scope = AssetPathScope::under(AssetSourcePath::new("custom/assets/images").unwrap());
        let local = AssetRef::path(format!("{}.png", "a".repeat(244))).unwrap();
        let source: StableAssetId = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f".parse().unwrap();
        let primary = AssetProductRef::new(source, ImportedProductId::PRIMARY).unwrap();
        let auxiliary = AssetProductRef::new(
            source,
            ImportedProductId::parse_subproduct("011a8cc0-343a-4b95-8adb-3aa6d81977ed").unwrap(),
        )
        .unwrap();
        let record = AssetRecord::new(
            source,
            scope.resolve(local.as_path().unwrap()),
            AssetSourceKind::Image,
        );
        assert!(record.path().as_str().len() > nara_ui_kernel::MAX_UI_KEY_BYTES);
        let mut database = ProjectAssetDatabase::default();
        database.insert(record.clone()).unwrap();
        let mut server = AssetServer::new();
        let primary_handle = server.reserve_record::<ImageAsset>(&record).unwrap();
        let auxiliary_handle = server.reserve_product::<ImageAsset>(auxiliary).unwrap();
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let mut snapshots = Vec::new();
        for (handle, product, pixels) in [
            (primary_handle, primary, [240, 0, 0, 255]),
            (auxiliary_handle, auxiliary, [0, 240, 0, 255]),
        ] {
            images
                .commit_loaded(
                    handle,
                    ImageAsset::from_product(
                        product,
                        ImageExtent::new(1, 1),
                        ImageFormat::Rgba8,
                        ImageColorSpace::Srgb,
                        pixels.to_vec(),
                    )
                    .unwrap(),
                    &mut states,
                    &mut events,
                    None,
                    None,
                )
                .unwrap();
            snapshots.push(PreparedImageSnapshot::capture(handle, &images, &states).unwrap());
        }
        let mut database = Some(database);
        let source_key = SourceDocumentKey::named(
            "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap(),
            "images",
            nara_scene::SourceDocumentKind::Prefab,
        )
        .unwrap();
        let occurrence = SceneOccurrenceKey::new("tests", "canvas-images").unwrap();
        let request = UiAuthoringCanvasCompileRequest::new(
            UiSurfaceContentTransform::direct(UiSize::new(320.0, 180.0).unwrap(), 1.0).unwrap(),
            1,
        );
        let mut compiler = UiAuthoringCanvasCompiler::new().unwrap();
        for (index, reference) in [local, AssetRef::product(auxiliary)]
            .into_iter()
            .enumerate()
        {
            if index == 1 {
                drop(database.take());
            }
            let visual =
                codec::image_visual_value(&reference, UiColor::rgba(1.0, 1.0, 1.0, 1.0)).unwrap();
            let node = |name: &str, style| {
                insert(
                    insert(
                        SceneEntityRecord::new(id(name)),
                        UiAuthoringComponentKind::Node,
                        codec::encode_node(&crate::UiNode::new(style)),
                    ),
                    UiAuthoringComponentKind::Visual,
                    Ok(visual.clone()),
                )
            };
            let root = insert(
                node("root", UiStyle::fill()).with_children([id("node"), id("button")]),
                UiAuthoringComponentKind::Root,
                codec::encode_root(&crate::UiRoot::new(UiLayer::Base)),
            );
            let child = node("node", UiStyle::absolute(0.0, 0.0, 32.0, 32.0));
            let button = insert(
                node("button", UiStyle::absolute(40.0, 0.0, 120.0, 32.0)),
                UiAuthoringComponentKind::Button,
                codec::encode_button(
                    &crate::UiButton::new(
                        "Button",
                        UiActionId::new("test.activate").unwrap(),
                        16.0,
                    )
                    .unwrap(),
                ),
            );
            let projection = crate::authoring_model::project_prefab_authoring(
                &PrefabDocument::new([id("root")], [root, child, button]),
            )
            .unwrap();
            let mut scratch = server.clone();
            let mut assets = ComponentDecodeContext::with_asset_server(&mut scratch);
            if let Some(database) = &database {
                assets = assets.with_asset_resolution(database.resolver(&scope));
            }
            let candidate = compiler
                .compile(
                    &projection,
                    &source_key,
                    &occurrence,
                    index as u64 + 1,
                    request,
                    &mut assets,
                )
                .unwrap();
            let snapshot = &snapshots[index];
            let expected =
                UiImageId::external::<ImageAsset>(snapshot.identity().key().asset_id().raw());
            let image_ids = candidate
                .frame()
                .paint()
                .iter()
                .filter_map(|paint| match paint {
                    UiPaintPrimitive::Rect {
                        brush: Some(UiResolvedBrush::Image { image, .. }),
                        ..
                    } => Some(image),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(image_ids, vec![&expected, &expected, &expected]);
            let mut prepared = nara_ui_render::UiPreparedImages::new();
            prepared.insert(expected, snapshot.clone());
            let packet = nara_ui_render::UiPresentationPlanner::default()
                .plan(
                    Arc::clone(candidate.frame()),
                    nara_render::RenderTarget::Window(nara_window::WindowId::PRIMARY),
                    &prepared,
                )
                .unwrap();
            assert_eq!(packet.images().len(), 1);
            assert_eq!(packet.images()[0].snapshot(), snapshot);
        }
    }

    #[test]
    fn canvas_pick_uses_safe_area_clip_visibility_and_paint_order() {
        let logical = UiSize::new(100.0, 100.0).unwrap();
        let physical = UiSize::new(200.0, 100.0).unwrap();
        let transform = UiSurfaceContentTransform::new(
            logical,
            physical,
            UiSafeAreaInsets::new(20.0, 10.0, 20.0, 10.0).unwrap(),
            1.0,
        )
        .unwrap();
        let bottom = id("bottom");
        let top = id("top");
        let hidden = id("hidden");
        let canvas = UiAuthoringCanvasReady::new(
            transform,
            [
                UiAuthoringCanvasFact::new(
                    bottom.clone(),
                    UiRect::from_origin_size(10.0, 10.0, 30.0, 30.0).unwrap(),
                    None,
                    0,
                    true,
                ),
                UiAuthoringCanvasFact::new(
                    top.clone(),
                    UiRect::from_origin_size(10.0, 10.0, 30.0, 30.0).unwrap(),
                    Some(UiRect::from_origin_size(25.0, 25.0, 10.0, 10.0).unwrap()),
                    2,
                    true,
                ),
                UiAuthoringCanvasFact::new(
                    hidden,
                    UiRect::from_origin_size(10.0, 10.0, 30.0, 30.0).unwrap(),
                    None,
                    3,
                    false,
                ),
            ],
        );

        let uv = |point: UiPoint| {
            let physical_point = transform.authored_to_physical(point).unwrap();
            [
                physical_point.x() / physical.width(),
                physical_point.y() / physical.height(),
            ]
        };
        assert_eq!(
            canvas.source_at_surface_uv(uv(UiPoint::new(20.0, 20.0))),
            Some(&bottom)
        );
        assert_eq!(
            canvas.source_at_surface_uv(uv(UiPoint::new(30.0, 30.0))),
            Some(&top)
        );
        assert_eq!(canvas.source_at_surface_uv([0.05, 0.5]), None);
        assert_eq!(canvas.source_at_surface_uv([1.0, 0.5]), None);
    }
}
