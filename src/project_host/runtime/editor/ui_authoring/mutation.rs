use nara_reflect::ComponentValueKind;
use nara_scene::AuthoringScalarEdit;
use nara_tooling::{
    EditorAtomicFieldEdit, EditorAtomicFieldEditError, EditorDocumentMutation, EditorFieldTarget,
    EditorFieldWrite, PrefabCompositionCommand, PrefabCompositionDraftError,
    PrefabCompositionEntityDraft,
};
use nara_ui::__private::{
    UiAuthoringCommitPolicy, UiAuthoringDirectEdit, UiAuthoringEditError, UiAuthoringEditability,
    UiAuthoringFieldKey, UiAuthoringFieldValue, UiPreparedStructure, UiStructureIntent,
    UiStructureRejection,
};
use thiserror::Error;

use super::EditorUiAuthoringFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditorUiMutationAuthority {
    PreservationSafeScalar,
    TrustedCompiled,
}

impl EditorUiAuthoringFrame {
    /// Returns whether this exact field can publish a mutation in the current execution lane.
    ///
    /// Generic authoring admits only one discrete, preservation-safe scalar write. Composite,
    /// continuous, direct-manipulation, and structural editing remain read-only until their
    /// owner modules provide equally strict authoring commands.
    #[doc(hidden)]
    #[must_use]
    pub fn field_editing_enabled(&self, key: &UiAuthoringFieldKey) -> bool {
        let Ok(projection) = self.projection() else {
            return false;
        };
        let Some(descriptor) = projection.field(key) else {
            return false;
        };
        if !matches!(descriptor.editability(), UiAuthoringEditability::Editable) {
            return false;
        }
        match self.mutation_authority {
            EditorUiMutationAuthority::TrustedCompiled => true,
            EditorUiMutationAuthority::PreservationSafeScalar => {
                descriptor.commit_policy() == UiAuthoringCommitPolicy::Discrete
                    && matches!(
                        descriptor.members(),
                        [member]
                            if matches!(
                                member.value_kind(),
                                ComponentValueKind::Bool
                                    | ComponentValueKind::I64
                                    | ComponentValueKind::U64
                                    | ComponentValueKind::F64
                                    | ComponentValueKind::String
                            )
                    )
            }
        }
    }

    /// Returns whether continuous field gestures and Canvas manipulation are admitted.
    #[doc(hidden)]
    #[must_use]
    pub const fn direct_editing_enabled(&self) -> bool {
        matches!(
            self.mutation_authority,
            EditorUiMutationAuthority::TrustedCompiled
        )
    }

    /// Returns whether closed-catalog structural mutations are admitted.
    #[doc(hidden)]
    #[must_use]
    pub const fn structure_editing_enabled(&self) -> bool {
        matches!(
            self.mutation_authority,
            EditorUiMutationAuthority::TrustedCompiled
        )
    }

    /// Creates one bounded, exact-revision tooling edit from the descriptor that owns UI field
    /// semantics. Toolkit Adapters do not reproduce composite lowering rules.
    pub fn lower_field_edit(
        &self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        if !self.direct_editing_enabled() {
            return Err(EditorUiAuthoringEditError::MutationUnavailable);
        }
        self.lower_atomic_field_edit(key, value)
    }

    fn lower_atomic_field_edit(
        &self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let descriptor = projection
            .field(key)
            .ok_or(EditorUiAuthoringEditError::FieldNotInFrame)?;
        let patch = descriptor
            .lower_edit(value)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        projection
            .validate_field_patch(key, descriptor.commit_policy(), &patch)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        self.atomic_edit_from_patch(key, &patch)
    }

    /// Lowers one discrete field edit into the lane-specific document mutation.
    ///
    /// The generic lane deliberately converts only a single scalar write to the nominal
    /// preservation-safe command instead of routing through a raw Prefab patch.
    pub fn lower_discrete_field_edit(
        &self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorDocumentMutation, EditorUiAuthoringEditError> {
        if !self.field_editing_enabled(key) {
            return Err(EditorUiAuthoringEditError::MutationUnavailable);
        }
        let edit = self.lower_atomic_field_edit(key, value)?;
        match self.mutation_authority {
            EditorUiMutationAuthority::TrustedCompiled => {
                Ok(EditorDocumentMutation::ApplyPrefabFieldEdit { edit })
            }
            EditorUiMutationAuthority::PreservationSafeScalar => {
                let [write] = edit.writes() else {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                };
                Ok(EditorDocumentMutation::ApplyAuthoringScalarEdit {
                    document: self.document,
                    base_revision: self.revision,
                    edit: AuthoringScalarEdit::new(
                        edit.target().entity().clone(),
                        edit.target().component().clone(),
                        write.field().clone(),
                        write.value().clone(),
                    ),
                })
            }
        }
    }

    /// Binds one closed two-dimensional manipulation to this exact document revision.
    ///
    /// Canonical position and size member fields remain owned by the UI authoring projection.
    pub fn lower_direct_edit(
        &self,
        edit: &UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        if !self.direct_editing_enabled() {
            return Err(EditorUiAuthoringEditError::MutationUnavailable);
        }
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let (key, patch) = projection
            .lower_direct_edit(edit)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        projection
            .validate_direct_patch(&key, &patch)
            .map_err(EditorUiAuthoringEditError::InvalidValue)?;
        self.atomic_edit_from_patch(&key, &patch)
    }

    /// Lowers one current closed-catalog intent into an exact-revision tooling mutation.
    pub fn lower_structure_intent(
        &self,
        intent: &UiStructureIntent,
    ) -> Result<EditorDocumentMutation, EditorUiAuthoringEditError> {
        if !self.structure_editing_enabled() {
            return Err(EditorUiAuthoringEditError::MutationUnavailable);
        }
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let image_choices = match intent {
            UiStructureIntent::Create {
                parameters: nara_ui::__private::UiCreateParameters::Image { .. },
                ..
            } => Some(
                self.image_choices()
                    .map_err(EditorUiAuthoringEditError::InvalidStructure)?,
            ),
            _ => self.image_choices().ok(),
        };
        let prepared = projection
            .prepare_structure(intent, image_choices)
            .map_err(EditorUiAuthoringEditError::InvalidStructure)?;
        let command = match prepared {
            UiPreparedStructure::Create { recipe, placement } => {
                let draft = PrefabCompositionEntityDraft::new(recipe.components().clone())
                    .map_err(EditorUiAuthoringEditError::StructureDraft)?;
                PrefabCompositionCommand::CreateEntity { draft, placement }
            }
            UiPreparedStructure::Move { target, placement } => {
                PrefabCompositionCommand::MoveEntity {
                    entity: target,
                    to: placement,
                }
            }
            UiPreparedStructure::Delete { target } => {
                PrefabCompositionCommand::RemoveEntity { entity: target }
            }
            UiPreparedStructure::NoOp { target } => {
                let UiStructureIntent::Move { placement, .. } = intent else {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                };
                PrefabCompositionCommand::MoveEntity {
                    entity: target,
                    to: placement.clone(),
                }
            }
        };
        Ok(EditorDocumentMutation::ComposePrefab {
            document: self.document,
            base_revision: self.revision,
            command,
        })
    }

    fn atomic_edit_from_patch(
        &self,
        key: &UiAuthoringFieldKey,
        patch: &nara_scene::ScenePatchDocument,
    ) -> Result<EditorAtomicFieldEdit, EditorUiAuthoringEditError> {
        let projection = self
            .projection()
            .map_err(EditorUiAuthoringEditError::ProjectionUnavailable)?;
        let descriptor = projection
            .field(key)
            .ok_or(EditorUiAuthoringEditError::FieldNotInFrame)?;
        let writes = patch
            .operations
            .iter()
            .map(|operation| {
                let nara_scene::ScenePatchOperation::SetField {
                    entity,
                    component,
                    component_version,
                    field,
                    value,
                } = operation
                else {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                };
                if entity != key.entity()
                    || component != key.component()
                    || *component_version != descriptor.component_version()
                {
                    return Err(EditorUiAuthoringEditError::DescriptorMismatch);
                }
                EditorFieldWrite::new(field.clone(), value.kind(), value.clone())
                    .map_err(EditorUiAuthoringEditError::AtomicEdit)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let target = EditorFieldTarget::new(
            self.document,
            self.revision,
            key.entity().clone(),
            key.component().clone(),
            descriptor.component_version(),
            key.primary_field().clone(),
        );
        EditorAtomicFieldEdit::new(target, writes).map_err(EditorUiAuthoringEditError::AtomicEdit)
    }

    pub(in super::super) fn admits_field_edit(
        &self,
        edit: &EditorAtomicFieldEdit,
        commit_policy: UiAuthoringCommitPolicy,
    ) -> bool {
        let target = edit.target();
        if target.document() != self.document || target.base_revision() != self.revision {
            return false;
        }
        let key = UiAuthoringFieldKey::new(
            target.entity().clone(),
            target.component().clone(),
            target.field().clone(),
        );
        self.projection().ok().is_some_and(|projection| {
            let version_matches = projection.field(&key).is_some_and(|descriptor| {
                descriptor.component_version() == target.component_version()
            });
            if !version_matches {
                return false;
            }
            let patch = edit.to_patch();
            match commit_policy {
                UiAuthoringCommitPolicy::Discrete => projection
                    .validate_field_patch(&key, commit_policy, &patch)
                    .is_ok(),
                UiAuthoringCommitPolicy::Continuous => projection
                    .validate_field_patch(&key, commit_policy, &patch)
                    .or_else(|_| projection.validate_direct_patch(&key, &patch))
                    .is_ok(),
            }
        })
    }

    pub(in super::super) fn admits_structure_command(
        &self,
        command: &PrefabCompositionCommand,
    ) -> bool {
        let Ok(projection) = self.projection() else {
            return false;
        };
        match command {
            PrefabCompositionCommand::CreateEntity { draft, placement } => projection
                .revalidate_create_recipe(draft.components(), placement, self.image_choices().ok())
                .is_ok(),
            PrefabCompositionCommand::MoveEntity { entity, to } => match projection
                .prepare_structure(
                    &UiStructureIntent::Move {
                        target: entity.clone(),
                        placement: to.clone(),
                    },
                    None,
                ) {
                Ok(UiPreparedStructure::Move { target, placement }) => {
                    target == *entity && placement == *to
                }
                Ok(UiPreparedStructure::NoOp { target }) => target == *entity,
                _ => false,
            },
            PrefabCompositionCommand::RemoveEntity { entity } => matches!(
                projection.prepare_structure(
                    &UiStructureIntent::Delete {
                        target: entity.clone(),
                    },
                    None,
                ),
                Ok(UiPreparedStructure::Delete { target }) if target == *entity
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum EditorUiAuthoringEditError {
    #[error("the UI authoring projection is unavailable: {0}")]
    ProjectionUnavailable(nara_ui::__private::UiAuthoringProjectionError),
    #[error("the UI authoring field does not belong to this exact frame")]
    FieldNotInFrame,
    #[error("the UI authoring mutation is unavailable in the current execution lane")]
    MutationUnavailable,
    #[error("the UI authoring value is invalid: {0}")]
    InvalidValue(UiAuthoringEditError),
    #[error("the UI descriptor produced a mismatched patch")]
    DescriptorMismatch,
    #[error("the bounded tooling edit rejected the UI descriptor output: {0}")]
    AtomicEdit(EditorAtomicFieldEditError),
    #[error("the UI structure intent is invalid: {0}")]
    InvalidStructure(UiStructureRejection),
    #[error("the bounded Prefab composition draft is invalid: {0}")]
    StructureDraft(PrefabCompositionDraftError),
}
