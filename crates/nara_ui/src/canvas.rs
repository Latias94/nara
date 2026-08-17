//! Renderer-neutral compilation of one persistent UI Prefab for Editor authoring.

use std::{collections::BTreeMap, sync::Arc};

use nara_reflect::{ComponentCodecError, ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_scene::{SceneComponentRecord, SceneEntityId};
use nara_text::bundled_font_face;
use nara_ui_kernel::{
    UiAuthoringError, UiAuthoringSnapshotBuilder, UiCompileError, UiCompiler, UiEnvironment,
    UiEnvironmentError, UiFrame, UiKey, UiKeyError, UiLocalKey, UiOpaqueId, UiPersistentIdentity,
    UiRootKey, UiRootStatus, UiSurfaceContentTransform, UiSurfaceGenerations, UiSurfaceId,
    UiTheme as KernelTheme,
};
use nara_ui_widgets::{UiButton as KernelButton, UiButtonState};
use thiserror::Error;

use crate::authoring_model::{
    UiAuthoringCanvasFact, UiAuthoringCanvasReady, UiAuthoringComponentKind,
    UiAuthoringHierarchyNode, UiAuthoringProjection,
};
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
        source_revision: u64,
        request: UiAuthoringCanvasCompileRequest,
    ) -> Result<UiAuthoringCanvasCandidate, UiAuthoringCanvasCompileError> {
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
            builder = builder.with_root(build_root(root, projection, &mut source_by_key)?);
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
    projection: &UiAuthoringProjection,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
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
    let root_key = UiRootKey::persistent(persistent_identity(entity)?)?;
    let source_key = UiKey::root(root_key.clone())?;
    insert_source_key(source_by_key, source_key, entity)?;

    let mut authored =
        nara_ui_kernel::UiRoot::new(root_key.clone(), root.layer(), authored_node.style());
    if let Some(visual) = decode_optional(
        projection,
        entity,
        UiAuthoringComponentKind::Visual,
        codec::decode_visual,
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
    let children = build_children(entity, projection, &root_key, source_by_key)?;
    Ok(authored.with_children(children))
}

fn build_children(
    parent: &SceneEntityId,
    projection: &UiAuthoringProjection,
    root_key: &UiRootKey,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
) -> Result<Vec<nara_ui_kernel::UiNode>, UiAuthoringCanvasCompileError> {
    let mut children = Vec::new();
    for child in projection
        .hierarchy()
        .iter()
        .filter(|child| child.parent() == Some(parent))
    {
        children.push(build_node(child, projection, root_key, source_by_key)?);
    }
    Ok(children)
}

fn build_node(
    hierarchy_node: &UiAuthoringHierarchyNode,
    projection: &UiAuthoringProjection,
    root_key: &UiRootKey,
    source_by_key: &mut BTreeMap<UiKey, SceneEntityId>,
) -> Result<nara_ui_kernel::UiNode, UiAuthoringCanvasCompileError> {
    let entity = hierarchy_node.entity();
    let node = decode_required(
        projection,
        entity,
        UiAuthoringComponentKind::Node,
        codec::decode_node,
    )?;
    let local_key = UiLocalKey::persistent(persistent_identity(entity)?)?;
    let source_key = UiKey::scoped(root_key.clone(), local_key.clone())?;
    insert_source_key(source_by_key, source_key.clone(), entity)?;
    let children = build_children(entity, projection, root_key, source_by_key)?;

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
            codec::decode_visual,
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
        codec::decode_visual,
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

fn persistent_identity(
    entity: &SceneEntityId,
) -> Result<UiPersistentIdentity, UiAuthoringCanvasCompileError> {
    Ok(UiPersistentIdentity::local(UiOpaqueId::new(
        entity.as_str(),
    )?))
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
    decode: fn(&ComponentValue) -> Result<T, ComponentCodecError>,
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
    decode: fn(&ComponentValue) -> Result<T, ComponentCodecError>,
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
    decode: fn(&ComponentValue) -> Result<T, ComponentCodecError>,
) -> Result<T, UiAuthoringCanvasCompileError> {
    if record.version != ComponentSchemaVersion::ONE {
        return Err(UiAuthoringCanvasCompileError::ComponentVersionMismatch {
            entity: entity.clone(),
            component,
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
    #[error(
        "UI entity {entity} component {component} has version {actual:?}; version 1 is required"
    )]
    ComponentVersionMismatch {
        entity: SceneEntityId,
        component: ComponentTypeId,
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
    use nara_reflect::ComponentSchemaVersion;
    use nara_scene::{PrefabDocument, SceneComponentRecord, SceneEntityId, SceneEntityRecord};
    use nara_ui_kernel::{
        UiColor, UiPoint, UiRect, UiSafeAreaInsets, UiSize, UiStyle, UiSurfaceContentTransform,
    };

    use super::*;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).unwrap()
    }

    fn record(value: ComponentValue) -> SceneComponentRecord {
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value)
    }

    fn insert(
        entity: SceneEntityRecord,
        kind: UiAuthoringComponentKind,
        value: Result<ComponentValue, ComponentCodecError>,
    ) -> SceneEntityRecord {
        entity.with_component(kind.component_type(), record(value.unwrap()))
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

        let candidate = UiAuthoringCanvasCompiler::new()
            .unwrap()
            .compile(&projection, 9, request)
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
