//! Closed source-local UI composition facts for first-party Editor adapters.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::AssetRef;
use nara_reflect::{ComponentTypeId, ComponentValue};
use nara_scene::{
    SceneComponentRecord, SceneEntityId, SceneEntityRecord, ScenePlacement, ScenePlacementAnchor,
    ScenePlacementOwner,
};
use nara_ui_kernel::{
    UiActionId, UiAlign, UiBrush, UiColor, UiColorRole, UiDimension, UiEdges, UiFlexDirection,
    UiJustify, UiOverflow, UiPositionMode, UiStyle, UiTextAlignment, UiTextOverflow,
};
use thiserror::Error;

use crate::{
    UiButton, UiButtonVisuals, UiNode, UiText, UiVisual,
    authoring_model::{
        UiAuthoringComponentKind, UiAuthoringHierarchyNode, UiAuthoringProjection,
        UiAuthoringSourceTopologyNode,
    },
    codec,
};

const MAX_IMAGE_CHOICES: usize = 4_096;
const MAX_IMAGE_FACT_BYTES: usize = 256;
const MAX_CREATE_TEXT_BYTES: usize = 4_096;

/// Closed first-party creation catalog. Current hierarchy classification is a separate type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiCatalogNodeKind {
    Container,
    Text,
    Image,
    Button,
}

impl UiCatalogNodeKind {
    pub const ALL: [Self; 4] = [Self::Container, Self::Text, Self::Image, Self::Button];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Container => "Container",
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Button => "Button",
        }
    }
}

/// Closed classification for an existing source-local hierarchy row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UiAuthoringHierarchyKind {
    Root,
    Container,
    Text,
    Image,
    Button,
    ExistingOther,
}

impl UiAuthoringHierarchyKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Root => "Root",
            Self::Container => "Container",
            Self::Text => "Text",
            Self::Image => "Image",
            Self::Button => "Button",
            Self::ExistingOther => "UI Node",
        }
    }
}

/// Why a projected row cannot be moved or deleted by the v1 structural workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringStructuralReadOnlyReason {
    ProtectedRoot,
    OmittedSourceDescendant,
    NestedUiRoot,
}

/// Shared move/delete capability derived from the complete source subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiAuthoringStructuralCapability {
    Editable,
    ReadOnly(UiAuthoringStructuralReadOnlyReason),
}

impl UiAuthoringStructuralCapability {
    pub(crate) const fn available() -> Self {
        Self::Editable
    }
}

/// Comparable Host-supplied project-content generation token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiImageChoiceGeneration(u64);

impl UiImageChoiceGeneration {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One exact source identity and canonical project-relative image path supplied by the Host.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiImageChoice {
    source_identity: String,
    project_path: String,
}

impl UiImageChoice {
    pub fn new(
        source_identity: impl Into<String>,
        project_path: impl Into<String>,
    ) -> Result<Self, UiStructureRejection> {
        let source_identity = source_identity.into();
        let project_path = project_path.into();
        validate_image_fact(&source_identity)?;
        validate_image_fact(&project_path)?;
        AssetRef::path(project_path.clone())
            .map_err(|_| UiStructureRejection::InvalidImageChoiceFact)?;
        Ok(Self {
            source_identity,
            project_path,
        })
    }

    #[must_use]
    pub fn source_identity(&self) -> &str {
        &self.source_identity
    }

    #[must_use]
    pub fn project_path(&self) -> &str {
        &self.project_path
    }

    #[must_use]
    pub fn selection(&self, generation: UiImageChoiceGeneration) -> UiImageChoiceSelection {
        UiImageChoiceSelection {
            generation,
            source_identity: self.source_identity.clone(),
            project_path: self.project_path.clone(),
        }
    }
}

/// Captured image choice carried by a revision-scoped Adapter form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiImageChoiceSelection {
    generation: UiImageChoiceGeneration,
    source_identity: String,
    project_path: String,
}

impl UiImageChoiceSelection {
    #[must_use]
    pub const fn generation(&self) -> UiImageChoiceGeneration {
        self.generation
    }

    #[must_use]
    pub fn source_identity(&self) -> &str {
        &self.source_identity
    }

    #[must_use]
    pub fn project_path(&self) -> &str {
        &self.project_path
    }
}

/// Bounded current image choices; project-content and Editor identities stay outside this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiImageChoiceSet {
    generation: UiImageChoiceGeneration,
    choices: Arc<[UiImageChoice]>,
}

impl UiImageChoiceSet {
    pub fn new(
        generation: UiImageChoiceGeneration,
        choices: impl IntoIterator<Item = UiImageChoice>,
    ) -> Result<Self, UiStructureRejection> {
        let choices = choices.into_iter().collect::<Vec<_>>();
        if choices.len() > MAX_IMAGE_CHOICES {
            return Err(UiStructureRejection::ImageChoiceLimitExceeded);
        }
        let mut source_identities = BTreeSet::new();
        let mut project_paths = BTreeSet::new();
        for choice in &choices {
            if !source_identities.insert(choice.source_identity.clone())
                || !project_paths.insert(choice.project_path.clone())
            {
                return Err(UiStructureRejection::DuplicateImageChoice);
            }
        }
        Ok(Self {
            generation,
            choices: choices.into(),
        })
    }

    #[must_use]
    pub const fn generation(&self) -> UiImageChoiceGeneration {
        self.generation
    }

    #[must_use]
    pub fn choices(&self) -> &[UiImageChoice] {
        &self.choices
    }

    fn resolve(
        &self,
        selection: &UiImageChoiceSelection,
    ) -> Result<&UiImageChoice, UiStructureRejection> {
        if selection.generation != self.generation {
            return Err(UiStructureRejection::StaleImageChoice);
        }
        self.choices
            .iter()
            .find(|choice| {
                choice.source_identity == selection.source_identity
                    && choice.project_path == selection.project_path
            })
            .ok_or(UiStructureRejection::ImageChoiceUnavailable)
    }

    fn selection_for_project_path(
        &self,
        project_path: &str,
    ) -> Result<UiImageChoiceSelection, UiStructureRejection> {
        self.choices
            .iter()
            .find(|choice| choice.project_path == project_path)
            .map(|choice| choice.selection(self.generation))
            .ok_or(UiStructureRejection::ImageChoiceUnavailable)
    }
}

fn validate_image_fact(value: &str) -> Result<(), UiStructureRejection> {
    if value.is_empty() || value.len() > MAX_IMAGE_FACT_BYTES || value.chars().any(char::is_control)
    {
        return Err(UiStructureRejection::InvalidImageChoiceFact);
    }
    Ok(())
}

/// Bounded kind-specific values captured before a create transaction allocates an entity ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCreateParameters {
    Container,
    Text {
        content: String,
    },
    Image {
        selection: Option<UiImageChoiceSelection>,
    },
    Button {
        label: String,
        action: Option<UiActionId>,
    },
}

impl UiCreateParameters {
    #[must_use]
    pub const fn kind(&self) -> UiCatalogNodeKind {
        match self {
            Self::Container => UiCatalogNodeKind::Container,
            Self::Text { .. } => UiCatalogNodeKind::Text,
            Self::Image { .. } => UiCatalogNodeKind::Image,
            Self::Button { .. } => UiCatalogNodeKind::Button,
        }
    }
}

/// Toolkit-neutral source-local structural request. It never carries an allocated entity ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiStructureIntent {
    Create {
        parameters: UiCreateParameters,
        placement: ScenePlacement,
    },
    Move {
        target: SceneEntityId,
        placement: ScenePlacement,
    },
    Delete {
        target: SceneEntityId,
    },
}

/// One exact legal placement shown by a current chooser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiPlacementCandidate {
    parent: SceneEntityId,
    placement: ScenePlacement,
    current: bool,
}

impl UiPlacementCandidate {
    #[must_use]
    pub const fn parent(&self) -> &SceneEntityId {
        &self.parent
    }

    #[must_use]
    pub const fn placement(&self) -> &ScenePlacement {
        &self.placement
    }

    #[must_use]
    pub const fn is_current(&self) -> bool {
        self.current
    }
}

/// Complete schema-versioned component bundle without a persistent entity ID.
#[derive(Debug, Clone, PartialEq)]
pub struct UiEntityRecipe {
    kind: UiCatalogNodeKind,
    components: Arc<BTreeMap<ComponentTypeId, SceneComponentRecord>>,
}

impl UiEntityRecipe {
    #[must_use]
    pub const fn kind(&self) -> UiCatalogNodeKind {
        self.kind
    }

    #[must_use]
    pub fn components(&self) -> &BTreeMap<ComponentTypeId, SceneComponentRecord> {
        &self.components
    }
}

/// Canonical preflight result consumed by the root Host and `nara_tooling` transaction owner.
#[derive(Debug, Clone, PartialEq)]
pub enum UiPreparedStructure {
    Create {
        recipe: UiEntityRecipe,
        placement: ScenePlacement,
    },
    Move {
        target: SceneEntityId,
        placement: ScenePlacement,
    },
    Delete {
        target: SceneEntityId,
    },
    NoOp {
        target: SceneEntityId,
    },
}

/// Typed rejection for closed catalog, source topology, placement, and image-choice preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum UiStructureRejection {
    #[error("the target is not in the exact current UI projection")]
    TargetNotProjected,
    #[error("the selected UI root is protected")]
    ProtectedRoot,
    #[error("the affected source subtree contains an omitted source record")]
    OmittedSourceDescendant,
    #[error("the affected source subtree contains a protected nested UI root")]
    NestedUiRoot,
    #[error("the selected parent is not in the exact current UI projection")]
    ParentNotProjected,
    #[error("the selected parent is the moved entity or one of its descendants")]
    DescendantParent,
    #[error("the placement owner is not the selected source-local UI parent")]
    InvalidPlacementOwner,
    #[error("the placement anchor is not an exact current projected sibling")]
    InvalidSiblingAnchor,
    #[error("image creation requires an exact current image choice")]
    ImageChoiceRequired,
    #[error("the captured image choice belongs to an obsolete project-content generation")]
    StaleImageChoice,
    #[error("the captured image choice is absent from the current exact choice set")]
    ImageChoiceUnavailable,
    #[error("the Host supplied an invalid or unbounded image-choice fact")]
    InvalidImageChoiceFact,
    #[error("the Host supplied more image choices than this authoring slice admits")]
    ImageChoiceLimitExceeded,
    #[error("the Host supplied duplicate image source or path identity")]
    DuplicateImageChoice,
    #[error("button creation requires a valid action identity")]
    ButtonActionRequired,
    #[error("button creation requires a non-empty label")]
    ButtonLabelRequired,
    #[error("a bounded create text value exceeds the admitted size")]
    CreateTextLimitExceeded,
    #[error("the complete first-party component recipe could not be encoded or validated")]
    InvalidRecipe,
}

impl UiAuthoringProjection {
    /// Computes only exact current parent/anchor choices. Missing anchors are never repaired.
    pub fn structure_placement_candidates(
        &self,
        moving: Option<&SceneEntityId>,
    ) -> Result<Vec<UiPlacementCandidate>, UiStructureRejection> {
        let excluded = if let Some(target) = moving {
            self.require_structurally_editable(target)?;
            self.source_subtree(target)?
        } else {
            BTreeSet::new()
        };
        let current = moving.and_then(|target| current_resolved_placement(self, target));
        let mut candidates = Vec::new();
        let mut resolved = BTreeSet::new();

        for parent in self.hierarchy() {
            if excluded.contains(parent.entity()) {
                continue;
            }
            let owner = ScenePlacementOwner::Parent {
                parent: parent.entity().clone(),
            };
            let source = self
                .source_node(parent.entity())
                .ok_or(UiStructureRejection::ParentNotProjected)?;
            self.push_placement_candidate(
                &mut candidates,
                &mut resolved,
                moving,
                parent.entity(),
                ScenePlacement::first(owner.clone()),
                current.as_ref(),
            );
            for sibling in source.children.iter().filter(|sibling| {
                moving != Some(*sibling)
                    && self
                        .source_node(sibling)
                        .is_some_and(|source| source.projected)
            }) {
                self.push_placement_candidate(
                    &mut candidates,
                    &mut resolved,
                    moving,
                    parent.entity(),
                    ScenePlacement::before(owner.clone(), sibling.clone()),
                    current.as_ref(),
                );
                self.push_placement_candidate(
                    &mut candidates,
                    &mut resolved,
                    moving,
                    parent.entity(),
                    ScenePlacement::after(owner.clone(), sibling.clone()),
                    current.as_ref(),
                );
            }
            self.push_placement_candidate(
                &mut candidates,
                &mut resolved,
                moving,
                parent.entity(),
                ScenePlacement::last(owner),
                current.as_ref(),
            );
        }
        Ok(candidates)
    }

    /// Revalidates one high-level intent and returns the complete ID-free structural command.
    pub fn prepare_structure(
        &self,
        intent: &UiStructureIntent,
        image_choices: Option<&UiImageChoiceSet>,
    ) -> Result<UiPreparedStructure, UiStructureRejection> {
        match intent {
            UiStructureIntent::Create {
                parameters,
                placement,
            } => {
                self.resolve_placement(None, placement)?;
                let recipe = build_recipe(parameters, image_choices)?;
                Ok(UiPreparedStructure::Create {
                    recipe,
                    placement: placement.clone(),
                })
            }
            UiStructureIntent::Move { target, placement } => {
                self.require_structurally_editable(target)?;
                let resolved = self.resolve_placement(Some(target), placement)?;
                if current_resolved_placement(self, target).as_ref() == Some(&resolved) {
                    return Ok(UiPreparedStructure::NoOp {
                        target: target.clone(),
                    });
                }
                Ok(UiPreparedStructure::Move {
                    target: target.clone(),
                    placement: placement.clone(),
                })
            }
            UiStructureIntent::Delete { target } => {
                self.require_structurally_editable(target)?;
                Ok(UiPreparedStructure::Delete {
                    target: target.clone(),
                })
            }
        }
    }

    /// Revalidates an ID-free entity draft against the current closed catalog and placement facts.
    ///
    /// This is the Host-side admission path for toolkit-neutral commands. It reconstructs the
    /// catalog parameters from the draft, rebuilds the owner-defined recipe, and requires exact
    /// component equality before any Editor identity is allocated.
    pub fn revalidate_create_recipe(
        &self,
        components: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
        placement: &ScenePlacement,
        image_choices: Option<&UiImageChoiceSet>,
    ) -> Result<(), UiStructureRejection> {
        let parameters = create_parameters_from_recipe(components, image_choices)?;
        let prepared = self.prepare_structure(
            &UiStructureIntent::Create {
                parameters,
                placement: placement.clone(),
            },
            image_choices,
        )?;
        let UiPreparedStructure::Create { recipe, .. } = prepared else {
            return Err(UiStructureRejection::InvalidRecipe);
        };
        if recipe.components() != components {
            return Err(UiStructureRejection::InvalidRecipe);
        }
        Ok(())
    }

    fn require_structurally_editable(
        &self,
        target: &SceneEntityId,
    ) -> Result<&UiAuthoringHierarchyNode, UiStructureRejection> {
        let node = self
            .hierarchy()
            .iter()
            .find(|node| node.entity() == target)
            .ok_or(UiStructureRejection::TargetNotProjected)?;
        match node.structural_capability() {
            UiAuthoringStructuralCapability::Editable => Ok(node),
            UiAuthoringStructuralCapability::ReadOnly(reason) => Err(reason.into()),
        }
    }

    fn source_subtree(
        &self,
        target: &SceneEntityId,
    ) -> Result<BTreeSet<SceneEntityId>, UiStructureRejection> {
        let mut subtree = BTreeSet::new();
        let mut pending = vec![target.clone()];
        while let Some(entity) = pending.pop() {
            if !subtree.insert(entity.clone()) {
                return Err(UiStructureRejection::OmittedSourceDescendant);
            }
            let source = self
                .source_node(&entity)
                .ok_or(UiStructureRejection::TargetNotProjected)?;
            pending.extend(source.children.iter().cloned());
        }
        Ok(subtree)
    }

    fn resolve_placement(
        &self,
        moving: Option<&SceneEntityId>,
        placement: &ScenePlacement,
    ) -> Result<ResolvedPlacement, UiStructureRejection> {
        let ScenePlacementOwner::Parent { parent } = &placement.owner else {
            return Err(UiStructureRejection::InvalidPlacementOwner);
        };
        let parent_source = self
            .source_node(parent)
            .filter(|source| source.projected)
            .ok_or(UiStructureRejection::ParentNotProjected)?;
        if moving
            .is_some_and(|target| target == parent || is_source_descendant(self, target, parent))
        {
            return Err(UiStructureRejection::DescendantParent);
        }

        let siblings = parent_source
            .children
            .iter()
            .filter(|candidate| moving != Some(*candidate))
            .cloned()
            .collect::<Vec<_>>();
        let index = match &placement.anchor {
            ScenePlacementAnchor::First => 0,
            ScenePlacementAnchor::Last => siblings.len(),
            ScenePlacementAnchor::Before { sibling } | ScenePlacementAnchor::After { sibling } => {
                if !self
                    .source_node(sibling)
                    .is_some_and(|source| source.projected)
                {
                    return Err(UiStructureRejection::InvalidSiblingAnchor);
                }
                let anchor_index = siblings
                    .iter()
                    .position(|candidate| candidate == sibling)
                    .ok_or(UiStructureRejection::InvalidSiblingAnchor)?;
                if matches!(&placement.anchor, ScenePlacementAnchor::Before { .. }) {
                    anchor_index
                } else {
                    anchor_index.saturating_add(1)
                }
            }
        };
        Ok(ResolvedPlacement {
            parent: parent.clone(),
            insertion_index: index,
        })
    }

    fn push_placement_candidate(
        &self,
        candidates: &mut Vec<UiPlacementCandidate>,
        seen: &mut BTreeSet<ResolvedPlacement>,
        moving: Option<&SceneEntityId>,
        parent: &SceneEntityId,
        placement: ScenePlacement,
        current: Option<&ResolvedPlacement>,
    ) {
        let Ok(resolved) = self.resolve_placement(moving, &placement) else {
            return;
        };
        if !seen.insert(resolved.clone()) {
            return;
        }
        candidates.push(UiPlacementCandidate {
            parent: parent.clone(),
            current: current == Some(&resolved),
            placement,
        });
    }
}

impl From<UiAuthoringStructuralReadOnlyReason> for UiStructureRejection {
    fn from(reason: UiAuthoringStructuralReadOnlyReason) -> Self {
        match reason {
            UiAuthoringStructuralReadOnlyReason::ProtectedRoot => Self::ProtectedRoot,
            UiAuthoringStructuralReadOnlyReason::OmittedSourceDescendant => {
                Self::OmittedSourceDescendant
            }
            UiAuthoringStructuralReadOnlyReason::NestedUiRoot => Self::NestedUiRoot,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ResolvedPlacement {
    parent: SceneEntityId,
    insertion_index: usize,
}

fn current_resolved_placement(
    projection: &UiAuthoringProjection,
    target: &SceneEntityId,
) -> Option<ResolvedPlacement> {
    let source = projection.source_node(target)?;
    let parent = source.parent.as_ref()?;
    let children = &projection.source_node(parent)?.children;
    let index = children.iter().position(|candidate| candidate == target)?;
    Some(ResolvedPlacement {
        parent: parent.clone(),
        insertion_index: index,
    })
}

fn is_source_descendant(
    projection: &UiAuthoringProjection,
    ancestor: &SceneEntityId,
    candidate: &SceneEntityId,
) -> bool {
    let mut cursor = projection
        .source_node(candidate)
        .and_then(|node| node.parent.as_ref());
    while let Some(parent) = cursor {
        if parent == ancestor {
            return true;
        }
        cursor = projection
            .source_node(parent)
            .and_then(|node| node.parent.as_ref());
    }
    false
}

fn build_recipe(
    parameters: &UiCreateParameters,
    image_choices: Option<&UiImageChoiceSet>,
) -> Result<UiEntityRecipe, UiStructureRejection> {
    let mut components = BTreeMap::new();
    let (width, height) = match parameters {
        UiCreateParameters::Container => (240.0, 160.0),
        UiCreateParameters::Text { .. } => (160.0, 32.0),
        UiCreateParameters::Image { .. } => (128.0, 128.0),
        UiCreateParameters::Button { .. } => (160.0, 44.0),
    };
    insert_component(
        &mut components,
        UiAuthoringComponentKind::Node,
        codec::encode_node(&UiNode::new(default_node_style(width, height)))
            .map_err(|_| UiStructureRejection::InvalidRecipe)?,
    )?;

    match parameters {
        UiCreateParameters::Container => {}
        UiCreateParameters::Text { content } => {
            validate_create_text(content)?;
            let text = UiText::new(content.clone(), 16.0)
                .map_err(|_| UiStructureRejection::InvalidRecipe)?
                .with_brush(UiBrush::Role(UiColorRole::PrimaryText))
                .with_alignment(UiTextAlignment::Start)
                .with_overflow(UiTextOverflow::Clip);
            insert_component(
                &mut components,
                UiAuthoringComponentKind::Text,
                codec::encode_text(&text).map_err(|_| UiStructureRejection::InvalidRecipe)?,
            )?;
        }
        UiCreateParameters::Image { selection } => {
            let selection = selection
                .as_ref()
                .ok_or(UiStructureRejection::ImageChoiceRequired)?;
            let choice = image_choices
                .ok_or(UiStructureRejection::ImageChoiceRequired)?
                .resolve(selection)?;
            let reference = AssetRef::path(choice.project_path.clone())
                .map_err(|_| UiStructureRejection::InvalidRecipe)?;
            insert_component(
                &mut components,
                UiAuthoringComponentKind::Visual,
                codec::image_visual_value(&reference, UiColor::rgba(1.0, 1.0, 1.0, 1.0))
                    .map_err(|_| UiStructureRejection::InvalidRecipe)?,
            )?;
        }
        UiCreateParameters::Button { label, action } => {
            validate_button_label(label)?;
            let action = action
                .as_ref()
                .ok_or(UiStructureRejection::ButtonActionRequired)?
                .clone();
            let visual = UiVisual::role(UiColorRole::Panel);
            let button = UiButton::new(label.clone(), action, 16.0)
                .map_err(|_| UiStructureRejection::InvalidRecipe)?
                .with_enabled(true);
            let visuals = UiButtonVisuals::new();
            insert_component(
                &mut components,
                UiAuthoringComponentKind::Visual,
                codec::encode_visual(&visual).map_err(|_| UiStructureRejection::InvalidRecipe)?,
            )?;
            insert_component(
                &mut components,
                UiAuthoringComponentKind::Button,
                codec::encode_button(&button).map_err(|_| UiStructureRejection::InvalidRecipe)?,
            )?;
            insert_component(
                &mut components,
                UiAuthoringComponentKind::ButtonVisuals,
                codec::encode_button_visuals(&visuals)
                    .map_err(|_| UiStructureRejection::InvalidRecipe)?,
            )?;
        }
    }

    validate_recipe_bundle(parameters.kind(), &components)?;
    Ok(UiEntityRecipe {
        kind: parameters.kind(),
        components: Arc::new(components),
    })
}

fn create_parameters_from_recipe(
    components: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
    image_choices: Option<&UiImageChoiceSet>,
) -> Result<UiCreateParameters, UiStructureRejection> {
    let kind = UiCatalogNodeKind::ALL
        .into_iter()
        .find(|kind| validate_recipe_bundle(*kind, components).is_ok())
        .ok_or(UiStructureRejection::InvalidRecipe)?;
    match kind {
        UiCatalogNodeKind::Container => Ok(UiCreateParameters::Container),
        UiCatalogNodeKind::Text => {
            let text = components
                .get(&UiAuthoringComponentKind::Text.component_type())
                .and_then(|record| codec::decode_text(&record.value).ok())
                .ok_or(UiStructureRejection::InvalidRecipe)?;
            Ok(UiCreateParameters::Text {
                content: text.kernel().content().to_string(),
            })
        }
        UiCatalogNodeKind::Image => {
            let reference = components
                .get(&UiAuthoringComponentKind::Visual.component_type())
                .and_then(|record| codec::visual_image_reference(&record.value).ok().flatten())
                .ok_or(UiStructureRejection::InvalidRecipe)?;
            let image_path = match &reference {
                AssetRef::Path(path) => path.as_str(),
                _ => return Err(UiStructureRejection::InvalidRecipe),
            };
            let selection = image_choices
                .ok_or(UiStructureRejection::ImageChoiceRequired)?
                .selection_for_project_path(image_path)?;
            Ok(UiCreateParameters::Image {
                selection: Some(selection),
            })
        }
        UiCatalogNodeKind::Button => {
            let button = components
                .get(&UiAuthoringComponentKind::Button.component_type())
                .and_then(|record| codec::decode_button(&record.value).ok())
                .ok_or(UiStructureRejection::InvalidRecipe)?;
            Ok(UiCreateParameters::Button {
                label: button.label().to_string(),
                action: Some(button.action().clone()),
            })
        }
    }
}

fn default_node_style(width: f32, height: f32) -> UiStyle {
    UiStyle::flex()
        .with_position_mode(UiPositionMode::Flow)
        .with_width(UiDimension::Px(width))
        .with_height(UiDimension::Px(height))
        .with_left(UiDimension::Px(0.0))
        .with_top(UiDimension::Px(0.0))
        .with_flex_direction(UiFlexDirection::Column)
        .with_justify_content(UiJustify::Start)
        .with_align_items(UiAlign::Stretch)
        .with_gap(0.0)
        .with_padding(UiEdges::default())
        .with_flex_grow(0.0)
        .with_flex_shrink(1.0)
        .with_z_index(0)
        .with_visible(true)
        .with_overflow(UiOverflow::Visible)
        .with_stacking_context(false)
}

fn validate_create_text(value: &str) -> Result<(), UiStructureRejection> {
    if value.len() > MAX_CREATE_TEXT_BYTES {
        return Err(UiStructureRejection::CreateTextLimitExceeded);
    }
    Ok(())
}

fn validate_button_label(value: &str) -> Result<(), UiStructureRejection> {
    if value.is_empty() {
        return Err(UiStructureRejection::ButtonLabelRequired);
    }
    validate_create_text(value)
}

fn insert_component(
    components: &mut BTreeMap<ComponentTypeId, SceneComponentRecord>,
    kind: UiAuthoringComponentKind,
    value: ComponentValue,
) -> Result<(), UiStructureRejection> {
    let component = kind.component_type();
    codec::validate_ui_component_value(&component, &value)
        .map_err(|_| UiStructureRejection::InvalidRecipe)?;
    let schema = codec::ui_schema_catalog()
        .components
        .into_iter()
        .find(|schema| schema.id() == &component)
        .ok_or(UiStructureRejection::InvalidRecipe)?;
    if components
        .insert(
            component,
            SceneComponentRecord::new(schema.version(), value),
        )
        .is_some()
    {
        return Err(UiStructureRejection::InvalidRecipe);
    }
    Ok(())
}

fn validate_recipe_bundle(
    kind: UiCatalogNodeKind,
    components: &BTreeMap<ComponentTypeId, SceneComponentRecord>,
) -> Result<(), UiStructureRejection> {
    let actual = components.keys().cloned().collect::<BTreeSet<_>>();
    let expected = match kind {
        UiCatalogNodeKind::Container => [UiAuthoringComponentKind::Node]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        UiCatalogNodeKind::Text => [
            UiAuthoringComponentKind::Node,
            UiAuthoringComponentKind::Text,
        ]
        .into_iter()
        .collect(),
        UiCatalogNodeKind::Image => [
            UiAuthoringComponentKind::Node,
            UiAuthoringComponentKind::Visual,
        ]
        .into_iter()
        .collect(),
        UiCatalogNodeKind::Button => [
            UiAuthoringComponentKind::Node,
            UiAuthoringComponentKind::Visual,
            UiAuthoringComponentKind::Button,
            UiAuthoringComponentKind::ButtonVisuals,
        ]
        .into_iter()
        .collect(),
    }
    .into_iter()
    .map(UiAuthoringComponentKind::component_type)
    .collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(UiStructureRejection::InvalidRecipe);
    }
    Ok(())
}

pub(crate) fn hierarchy_kind(entity: &SceneEntityRecord) -> UiAuthoringHierarchyKind {
    let has =
        |kind: UiAuthoringComponentKind| entity.components.contains_key(&kind.component_type());
    if has(UiAuthoringComponentKind::Root) {
        return UiAuthoringHierarchyKind::Root;
    }
    if has(UiAuthoringComponentKind::Button) {
        return if has(UiAuthoringComponentKind::Node)
            && has(UiAuthoringComponentKind::Visual)
            && has(UiAuthoringComponentKind::ButtonVisuals)
            && !has(UiAuthoringComponentKind::Text)
            && !has(UiAuthoringComponentKind::Semantics)
        {
            UiAuthoringHierarchyKind::Button
        } else {
            UiAuthoringHierarchyKind::ExistingOther
        };
    }
    if has(UiAuthoringComponentKind::Text) {
        return UiAuthoringHierarchyKind::Text;
    }
    if entity
        .components
        .get(&UiAuthoringComponentKind::Visual.component_type())
        .and_then(|record| codec::visual_image_reference(&record.value).ok().flatten())
        .is_some()
    {
        return UiAuthoringHierarchyKind::Image;
    }
    if has(UiAuthoringComponentKind::Node) {
        UiAuthoringHierarchyKind::Container
    } else {
        UiAuthoringHierarchyKind::ExistingOther
    }
}

pub(crate) fn decorate_authoring_hierarchy(
    hierarchy: &mut [UiAuthoringHierarchyNode],
    source_topology: &[UiAuthoringSourceTopologyNode],
) {
    let mut collision_groups =
        BTreeMap::<(Option<SceneEntityId>, UiAuthoringHierarchyKind), Vec<SceneEntityId>>::new();
    for node in hierarchy.iter() {
        collision_groups
            .entry((node.parent().cloned(), node.kind()))
            .or_default()
            .push(node.entity().clone());
    }

    let mut paths = BTreeMap::<SceneEntityId, String>::new();
    for node in hierarchy.iter_mut() {
        let siblings = collision_groups
            .get(&(node.parent().cloned(), node.kind()))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let label = if siblings.len() > 1 {
            format!(
                "{} [{}]",
                node.kind().label(),
                shortest_unique_suffix(node.entity(), siblings)
            )
        } else {
            node.kind().label().to_owned()
        };
        let path = node
            .parent()
            .and_then(|parent| paths.get(parent))
            .map_or_else(
                || label.clone(),
                |parent_path| format!("{parent_path}/{label}"),
            );
        let capability = structural_capability(node.entity(), source_topology);
        paths.insert(node.entity().clone(), path.clone());
        node.set_composition_facts(label, path, capability);
    }
}

fn structural_capability(
    target: &SceneEntityId,
    source_topology: &[UiAuthoringSourceTopologyNode],
) -> UiAuthoringStructuralCapability {
    let Some(root) = source_topology.iter().find(|node| node.entity == *target) else {
        return UiAuthoringStructuralCapability::ReadOnly(
            UiAuthoringStructuralReadOnlyReason::OmittedSourceDescendant,
        );
    };
    if root.contains_ui_root {
        return UiAuthoringStructuralCapability::ReadOnly(
            UiAuthoringStructuralReadOnlyReason::ProtectedRoot,
        );
    }

    let mut omitted = false;
    let mut nested_root = false;
    let mut visited = BTreeSet::new();
    let mut pending = root.children.clone();
    while let Some(entity) = pending.pop() {
        if !visited.insert(entity.clone()) {
            omitted = true;
            continue;
        }
        let Some(source) = source_topology.iter().find(|node| node.entity == entity) else {
            omitted = true;
            continue;
        };
        nested_root |= source.contains_ui_root;
        omitted |= !source.projected;
        pending.extend(source.children.iter().cloned());
    }
    if nested_root {
        UiAuthoringStructuralCapability::ReadOnly(UiAuthoringStructuralReadOnlyReason::NestedUiRoot)
    } else if omitted {
        UiAuthoringStructuralCapability::ReadOnly(
            UiAuthoringStructuralReadOnlyReason::OmittedSourceDescendant,
        )
    } else {
        UiAuthoringStructuralCapability::Editable
    }
}

fn shortest_unique_suffix<'a>(entity: &'a SceneEntityId, siblings: &[SceneEntityId]) -> &'a str {
    let id = entity.as_str();
    let maximum = siblings
        .iter()
        .map(|candidate| candidate.as_str().len())
        .max()
        .unwrap_or(id.len());
    for length in 4..=maximum {
        let suffix = ascii_suffix(id, length);
        if siblings
            .iter()
            .filter(|candidate| *candidate != entity)
            .all(|candidate| ascii_suffix(candidate.as_str(), length) != suffix)
        {
            return suffix;
        }
    }
    id
}

fn ascii_suffix(value: &str, maximum: usize) -> &str {
    &value[value.len().saturating_sub(maximum)..]
}

#[cfg(test)]
mod tests {
    use nara_reflect::ComponentSchemaVersion;
    use nara_scene::{PrefabDocument, SceneEntityRecord};
    use nara_ui_kernel::UiLayer;

    use super::*;
    use crate::UiRoot;

    fn id(value: &str) -> SceneEntityId {
        SceneEntityId::new(value).expect("test entity ID must be valid")
    }

    fn image_choices(generation: u64) -> UiImageChoiceSet {
        UiImageChoiceSet::new(
            UiImageChoiceGeneration::new(generation),
            [UiImageChoice::new("image-source-1", "assets/ui/panel.png").unwrap()],
        )
        .unwrap()
    }

    fn entity_from_recipe(
        entity: SceneEntityId,
        children: impl IntoIterator<Item = SceneEntityId>,
    ) -> SceneEntityRecord {
        let recipe = build_recipe(&UiCreateParameters::Container, None).unwrap();
        let mut record = SceneEntityRecord::new(entity).with_children(children);
        for (component, value) in recipe.components().iter() {
            record = record.with_component(component.clone(), value.clone());
        }
        record
    }

    fn root_entity(
        entity: SceneEntityId,
        children: impl IntoIterator<Item = SceneEntityId>,
    ) -> SceneEntityRecord {
        let record = entity_from_recipe(entity, children);
        let root = UiRoot::new(UiLayer::Base);
        record.with_component(
            UiAuthoringComponentKind::Root.component_type(),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                codec::encode_root(&root).unwrap(),
            ),
        )
    }

    #[test]
    fn closed_catalog_builds_each_complete_exact_recipe() {
        let choices = image_choices(1);
        let image = choices.choices()[0].selection(choices.generation());
        let cases = [
            (
                UiCreateParameters::Container,
                UiCatalogNodeKind::Container,
                240.0,
                160.0,
                1,
            ),
            (
                UiCreateParameters::Text {
                    content: "Text".to_owned(),
                },
                UiCatalogNodeKind::Text,
                160.0,
                32.0,
                2,
            ),
            (
                UiCreateParameters::Image {
                    selection: Some(image),
                },
                UiCatalogNodeKind::Image,
                128.0,
                128.0,
                2,
            ),
            (
                UiCreateParameters::Button {
                    label: "Button".to_owned(),
                    action: Some(UiActionId::new("test.create").unwrap()),
                },
                UiCatalogNodeKind::Button,
                160.0,
                44.0,
                4,
            ),
        ];

        for (parameters, kind, width, height, component_count) in cases {
            let recipe = build_recipe(&parameters, Some(&choices)).unwrap();
            assert_eq!(recipe.kind(), kind);
            assert_eq!(recipe.components().len(), component_count);
            let node = recipe
                .components()
                .get(&UiAuthoringComponentKind::Node.component_type())
                .and_then(|record| codec::decode_node(&record.value).ok())
                .expect("every recipe must contain one valid node");
            assert_eq!(node.style().position_mode(), UiPositionMode::Flow);
            assert_eq!(node.style().width(), UiDimension::Px(width));
            assert_eq!(node.style().height(), UiDimension::Px(height));
            assert_eq!(node.style().left(), UiDimension::Px(0.0));
            assert_eq!(node.style().top(), UiDimension::Px(0.0));
            assert_eq!(node.style().flex_direction(), UiFlexDirection::Column);
            assert_eq!(node.style().justify_content(), UiJustify::Start);
            assert_eq!(node.style().align_items(), UiAlign::Stretch);
            assert_eq!(node.style().gap(), 0.0);
            assert_eq!(node.style().padding(), UiEdges::default());
            assert_eq!(node.style().flex_grow(), 0.0);
            assert_eq!(node.style().flex_shrink(), 1.0);
            assert_eq!(node.style().z_index(), 0);
            assert!(node.style().visible());
            assert_eq!(node.style().overflow(), UiOverflow::Visible);
            assert!(!node.style().stacking_context());
        }

        let button = build_recipe(
            &UiCreateParameters::Button {
                label: "Button".to_owned(),
                action: Some(UiActionId::new("test.create").unwrap()),
            },
            None,
        )
        .unwrap();
        assert!(
            !button
                .components()
                .contains_key(&UiAuthoringComponentKind::Text.component_type())
        );
        assert!(
            !button
                .components()
                .contains_key(&UiAuthoringComponentKind::Semantics.component_type())
        );
        assert_eq!(
            build_recipe(
                &UiCreateParameters::Button {
                    label: String::new(),
                    action: Some(UiActionId::new("test.create").unwrap()),
                },
                None,
            ),
            Err(UiStructureRejection::ButtonLabelRequired)
        );
    }

    #[test]
    fn image_recipe_requires_an_exact_current_choice() {
        let current = image_choices(2);
        let stale = image_choices(1);
        assert_eq!(
            build_recipe(
                &UiCreateParameters::Image { selection: None },
                Some(&current)
            ),
            Err(UiStructureRejection::ImageChoiceRequired)
        );
        assert_eq!(
            build_recipe(
                &UiCreateParameters::Image {
                    selection: Some(stale.choices()[0].selection(stale.generation())),
                },
                Some(&current),
            ),
            Err(UiStructureRejection::StaleImageChoice)
        );
        let accepted = build_recipe(
            &UiCreateParameters::Image {
                selection: Some(current.choices()[0].selection(current.generation())),
            },
            Some(&current),
        )
        .unwrap();
        let visual = accepted
            .components()
            .get(&UiAuthoringComponentKind::Visual.component_type())
            .unwrap();
        assert_eq!(visual.version, ComponentSchemaVersion(2));
        assert_eq!(
            visual
                .value
                .field("background")
                .unwrap()
                .field("image")
                .unwrap()
                .field_str("value")
                .unwrap(),
            "assets/ui/panel.png"
        );
    }

    #[test]
    fn placement_protects_roots_excludes_descendants_and_normalizes_no_op() {
        let root = id("root");
        let container = id("container");
        let before = id("before");
        let moving = id("moving");
        let descendant = id("moving/descendant");
        let prefab = PrefabDocument::new(
            [root.clone()],
            [
                root_entity(root.clone(), [container.clone()]),
                entity_from_recipe(container.clone(), [before.clone(), moving.clone()]),
                entity_from_recipe(before.clone(), []),
                entity_from_recipe(moving.clone(), [descendant.clone()]),
                entity_from_recipe(descendant.clone(), []),
            ],
        );
        let projection = crate::authoring_model::project_prefab_authoring(&prefab).unwrap();
        assert_eq!(
            projection.prepare_structure(
                &UiStructureIntent::Delete {
                    target: root.clone(),
                },
                None,
            ),
            Err(UiStructureRejection::ProtectedRoot)
        );
        let candidates = projection
            .structure_placement_candidates(Some(&moving))
            .unwrap();
        assert!(candidates.iter().all(|candidate| {
            candidate.parent() != &moving && candidate.parent() != &descendant
        }));
        assert!(candidates.iter().any(|candidate| {
            candidate.parent() == &container
                && matches!(
                    &candidate.placement().anchor,
                    ScenePlacementAnchor::After { sibling } if sibling == &before
                )
                && candidate.is_current()
        }));
        let container_choices = candidates
            .iter()
            .filter(|candidate| candidate.parent() == &container)
            .collect::<Vec<_>>();
        assert_eq!(container_choices.len(), 2);
        assert_eq!(
            container_choices
                .iter()
                .filter(|candidate| candidate.is_current())
                .count(),
            1
        );
        assert_eq!(
            projection
                .prepare_structure(
                    &UiStructureIntent::Move {
                        target: moving.clone(),
                        placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                            parent: container.clone(),
                        }),
                    },
                    None,
                )
                .unwrap(),
            UiPreparedStructure::NoOp { target: moving }
        );
        assert_eq!(
            projection.prepare_structure(
                &UiStructureIntent::Create {
                    parameters: UiCreateParameters::Container,
                    placement: ScenePlacement::after(
                        ScenePlacementOwner::Parent { parent: container },
                        id("missing-anchor"),
                    ),
                },
                None,
            ),
            Err(UiStructureRejection::InvalidSiblingAnchor)
        );
    }

    #[test]
    fn complete_source_topology_rejects_omitted_descendants_and_nested_roots() {
        let root = id("root");
        let omitted_owner = id("omitted-owner");
        let hidden = id("hidden");
        let nested_owner = id("nested-owner");
        let nested_root = id("nested-root");
        let hidden_record = SceneEntityRecord::new(hidden.clone());
        let prefab = PrefabDocument::new(
            [root.clone()],
            [
                root_entity(root, [omitted_owner.clone(), nested_owner.clone()]),
                entity_from_recipe(omitted_owner.clone(), [hidden.clone()]),
                hidden_record,
                entity_from_recipe(nested_owner.clone(), [nested_root.clone()]),
                root_entity(nested_root, []),
            ],
        );
        let projection = crate::authoring_model::project_prefab_authoring(&prefab).unwrap();
        assert_eq!(
            projection.prepare_structure(
                &UiStructureIntent::Delete {
                    target: omitted_owner,
                },
                None,
            ),
            Err(UiStructureRejection::OmittedSourceDescendant)
        );
        assert_eq!(
            projection.prepare_structure(
                &UiStructureIntent::Delete {
                    target: nested_owner,
                },
                None,
            ),
            Err(UiStructureRejection::NestedUiRoot)
        );
    }
}
