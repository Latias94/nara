use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use nara_asset::AssetRef;
use nara_core::ByteLimit;
use nara_identity::{AuthoredEntityReference, EntityReference};
use nara_reflect::{__private::component_values_exactly_match, ComponentValueRef};

use super::*;
use crate::{
    AuthoringEntityView, AuthoringPrefabComponentOverrideKind, AuthoringPrefabInstanceSnapshot,
    AuthoringPrefabRecordOrigin, AuthoringReadError, PrefabComponentSetMode,
    PrefabDeclarationSource, PrefabDocument, PrefabEntityOrigin, ResolvedPrefabSource,
    SceneCompilationFailureKind, SceneCompilationOptions, SceneCompilationWorkingSet,
    SceneComponentRecord, SourceDocumentKey,
    authoring_decode::{
        AuthoringDocumentState, PreparedPrefabAuthoringPatch, PreparedSceneAuthoringPatch,
        prepare_prefab_authoring_patch_with_admission, prepare_prefab_component_apply_patch,
        prepare_scene_authoring_patch_with_admission,
    },
    prefab::{
        ComponentApplyScope, SelectedInstanceCompilation, SelectedInstanceConsumer,
        SourceDocumentInput,
        inspection::{
            AuthoringPrefabComponentEditError, AuthoringPrefabInspectionError,
            CapturedPrefabInspection, copy_value_with_references,
        },
        try_compile_selected_authoring_instance_with_loader,
    },
};

#[derive(Clone, Copy)]
pub enum PrefabComponentApplySource<'a> {
    Scene(&'a SceneAuthoringSession),
    Prefab(&'a PrefabAuthoringSession),
}

impl<'a> PrefabComponentApplySource<'a> {
    fn capture(
        self,
    ) -> (
        SourceDocumentInput<'a>,
        Option<&'a AuthoringDocumentState>,
        SceneAuthoringRevision,
    ) {
        match self {
            Self::Scene(session) => (
                SourceDocumentInput::Scene(session.document()),
                session.authority.published(),
                session.revision(),
            ),
            Self::Prefab(session) => (
                SourceDocumentInput::Prefab(session.document()),
                session.authority.published(),
                session.revision(),
            ),
        }
    }
}

/// Both entity routes are relative to the containing source. The destination route selects a
/// direct source facet in the supplied Prefab session, including when a bound root has aliases.
pub struct PrefabComponentApplyRequest<'a> {
    pub containing: PrefabComponentApplySource<'a>,
    pub containing_revision: SceneAuthoringRevision,
    pub inspection: &'a AuthoringPrefabInstanceSnapshot,
    pub selected: &'a PrefabEntityTarget,
    pub destination_route: &'a PrefabEntityTarget,
    pub destination: &'a PrefabAuthoringSession,
    pub destination_revision: SceneAuthoringRevision,
    pub component: &'a ComponentTypeId,
    pub declaration_limits: PrefabDeclarationLimits,
    pub compilation: SceneCompilationOptions,
}

#[derive(Debug)]
pub enum PrefabComponentApplyPreparationError<E> {
    InvalidDestination,
    InvalidComponent,
    NonPortableReference,
    StaleBasis,
    WorkLimit,
    Patch(Box<ScenePatchReport>),
    Compilation(AuthoringPrefabInspectionError<E>),
    CandidateChanged,
}

impl<E> From<ScenePatchReport> for PrefabComponentApplyPreparationError<E> {
    fn from(report: ScenePatchReport) -> Self {
        Self::Patch(Box::new(report))
    }
}

#[derive(Debug)]
enum ContainingCandidate {
    Scene(PreparedSceneAuthoringPatch),
    Prefab(PreparedPrefabAuthoringPatch),
}

impl ContainingCandidate {
    fn document(&self) -> SourceDocumentInput<'_> {
        match self {
            Self::Scene(candidate) => SourceDocumentInput::Scene(&candidate.document),
            Self::Prefab(candidate) => SourceDocumentInput::Prefab(&candidate.document),
        }
    }
}

/// Jointly verified candidates. Neither edit can be consumed as an independent history entry.
#[derive(Debug)]
pub struct PreparedPrefabComponentApply {
    containing: ContainingCandidate,
    destination: Arc<PrefabDocument>,
    destination_state: AuthoringDocumentState,
    destination_patch: ScenePatchDocument,
    destination_inverse: ScenePatchDocument,
    containing_revision: SceneAuthoringRevision,
    destination_revision: SceneAuthoringRevision,
    kind: AuthoringPrefabComponentOverrideKind,
}

impl PreparedPrefabComponentApply {
    pub fn containing_document(&self) -> SourceDocumentInput<'_> {
        self.containing.document()
    }

    pub fn containing_patch(&self) -> &ScenePatchDocument {
        match &self.containing {
            ContainingCandidate::Scene(candidate) => &candidate.patch,
            ContainingCandidate::Prefab(candidate) => &candidate.patch,
        }
    }

    pub fn destination_document(&self) -> &PrefabDocument {
        &self.destination
    }

    pub fn destination_entity(
        &self,
        entity: &SceneEntityId,
    ) -> Result<AuthoringEntityView<'_>, AuthoringReadError> {
        self.destination_state
            .entity_view(&self.destination.entities, entity)
    }

    pub const fn destination_patch(&self) -> &ScenePatchDocument {
        &self.destination_patch
    }

    pub const fn destination_inverse(&self) -> &ScenePatchDocument {
        &self.destination_inverse
    }

    pub const fn containing_revision(&self) -> SceneAuthoringRevision {
        self.containing_revision
    }

    pub const fn destination_revision(&self) -> SceneAuthoringRevision {
        self.destination_revision
    }

    pub const fn kind(&self) -> AuthoringPrefabComponentOverrideKind {
        self.kind
    }
}

/// Prepares an exact selected-layer Revert and one direct destination edit. The loader must
/// retain the captured source generation across both passes. Disk and dependent validation
/// belong to the project publication owner.
pub fn try_prepare_prefab_component_apply_with_loader<F, E>(
    request: PrefabComponentApplyRequest<'_>,
    loader: F,
) -> Result<PreparedPrefabComponentApply, PrefabComponentApplyPreparationError<E>>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    use PrefabComponentApplyPreparationError as Error;

    let basis = request.inspection.schema_basis();
    let (source, containing_state, containing_revision) = request.containing.capture();
    let containing_state = containing_state.ok_or(Error::StaleBasis)?;
    let destination_document = request.destination.document();
    let destination_state = request
        .destination
        .authority
        .published()
        .ok_or(Error::StaleBasis)?;
    if containing_revision != request.containing_revision
        || request.destination.revision() != request.destination_revision
        || destination_state.schema_basis() != *basis
        || containing_state.schema_basis() != *basis
    {
        return Err(Error::StaleBasis);
    }
    if containing_state.source() == destination_state.source() {
        return Err(Error::InvalidDestination);
    }
    let route = request.destination_route;
    if route.through.len() > request.declaration_limits.max_depth()
        || route
            .logical_bytes_up_to(request.declaration_limits.max_logical_bytes())
            .is_none()
    {
        return Err(Error::WorkLimit);
    }
    let (Some(hop), PrefabEntityOrigin::SourceEntity(entity)) =
        (route.through.last(), &route.entity)
    else {
        return Err(Error::InvalidDestination);
    };
    let destination_key = &hop.expected_base;
    let (anchor, declaration_index, intent, cost) = request
        .inspection
        .component_apply_declaration(
            source,
            request.containing_revision,
            request.selected,
            request.component,
            request.declaration_limits,
        )
        .map_err(|error| match error {
            AuthoringPrefabComponentEditError::BudgetExceeded => Error::WorkLimit,
            AuthoringPrefabComponentEditError::StaleObservedIntent => Error::StaleBasis,
            _ => Error::InvalidComponent,
        })?;
    let kind = AuthoringPrefabComponentOverrideKind::of(intent);
    let mut work = request.compilation.limits.working_bytes().get();
    spend(
        &mut work,
        cost.nodes
            .checked_mul(512)
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes().checked_mul(2)?)),
    )
    .ok_or(Error::WorkLimit)?;
    let mut queries = BTreeSet::new();
    let mut reference_work = (0usize, 0usize);
    let operation = match intent {
        PrefabComponentIntent::Set { mode, record, .. } => {
            let value = rebase_record(record, &route.through, &mut queries, &mut reference_work)?;
            match mode {
                PrefabComponentSetMode::Add => ScenePatchOperation::AddComponent {
                    entity: entity.clone(),
                    component: request.component.clone(),
                    value,
                },
                PrefabComponentSetMode::Replace => ScenePatchOperation::ReplaceComponent {
                    entity: entity.clone(),
                    component: request.component.clone(),
                    value,
                },
            }
        }
        PrefabComponentIntent::Suppress { .. } => ScenePatchOperation::RemoveComponent {
            entity: entity.clone(),
            component: request.component.clone(),
        },
        PrefabComponentIntent::Properties { .. } => return Err(Error::InvalidComponent),
    };
    let scope = ComponentApplyScope {
        selected: request.selected,
        destination: route,
        component: request.component,
        destination_document,
        declaration: Some(declaration_index),
    };
    let mut compiler = PairCompiler {
        anchor,
        basis,
        options: request.compilation,
        destination_key,
        destination_document,
        queries: queries.into_iter().collect(),
        reference_work,
        remaining: work,
        loader,
    };
    let (original, original_proof) = compiler.compile(source, scope, None)?;
    let entities = match source {
        SourceDocumentInput::Scene(document) => &document.entities,
        SourceDocumentInput::Prefab(document) => &document.entities,
    };
    let revert = lower_revert_with_admission(
        entities,
        &PrefabInstancePath::new(anchor.clone()),
        request.declaration_limits,
        |bytes| spend(&mut compiler.remaining, bytes).ok_or(PrefabDeclarationError::BudgetExceeded),
        |overlay| {
            overlay
                .components
                .get(declaration_index)
                .filter(|current| std::ptr::eq(*current, intent))
                .map(|_| {
                    Some(PrefabOverlaySlot::Component {
                        index: declaration_index,
                    })
                })
                .ok_or(PrefabDeclarationError::StaleObservedIntent)
        },
    )
    .map_err(PrefabDeclarationError::report)?;
    let mut admit = |_, bytes| {
        spend(&mut compiler.remaining, bytes).ok_or_else(|| {
            history_error_report(
                "scene.authoring-prefab-component-apply-work-limit",
                "Component Apply exceeds its preparation work allowance",
            )
        })
    };
    let containing = match source {
        SourceDocumentInput::Scene(document) => {
            ContainingCandidate::Scene(prepare_scene_authoring_patch_with_admission(
                document,
                containing_state,
                &revert,
                false,
                &mut admit,
            )?)
        }
        SourceDocumentInput::Prefab(document) => {
            ContainingCandidate::Prefab(prepare_prefab_authoring_patch_with_admission(
                document,
                containing_state,
                &revert,
                false,
                &mut admit,
            )?)
        }
    };
    let PreparedPrefabAuthoringPatch {
        document,
        state,
        patch,
        report,
    } = prepare_prefab_component_apply_patch(
        destination_document,
        destination_state,
        &ScenePatchDocument::new([operation]),
        &mut admit,
    )?;
    let destination = Arc::new(document);
    let (candidate, candidate_proof) = compiler.compile(
        containing.document(),
        ComponentApplyScope {
            destination_document: &destination,
            declaration: None,
            ..scope
        },
        Some(&destination),
    )?;
    if original_proof != candidate_proof
        || !verifies_inheritance(
            &original,
            &candidate,
            &request,
            destination_key,
            entity,
            kind,
        )
    {
        return Err(Error::CandidateChanged);
    }
    let destination_inverse = report.inverse.ok_or(Error::CandidateChanged)?;
    Ok(PreparedPrefabComponentApply {
        containing,
        destination,
        destination_state: state,
        destination_patch: patch,
        destination_inverse,
        containing_revision,
        destination_revision: request.destination_revision,
        kind,
    })
}

fn spend(remaining: &mut usize, bytes: Option<usize>) -> Option<()> {
    *remaining = remaining.checked_sub(bytes?)?;
    Some(())
}

type ReferenceProof = BTreeMap<AuthoredEntityReference, EntityReference>;

struct PairCompiler<'a, F> {
    anchor: &'a SceneEntityId,
    basis: &'a AuthoringSchemaBasis,
    options: SceneCompilationOptions,
    destination_key: &'a SourceDocumentKey,
    destination_document: &'a PrefabDocument,
    queries: Vec<AuthoredEntityReference>,
    reference_work: (usize, usize),
    remaining: usize,
    loader: F,
}

impl<F, E> PairCompiler<'_, F>
where
    F: FnMut(&AssetRef, &mut SceneCompilationWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    fn compile(
        &mut self,
        source: SourceDocumentInput<'_>,
        scope: ComponentApplyScope<'_>,
        replacement: Option<&Arc<PrefabDocument>>,
    ) -> Result<(CapturedPrefabInspection, ReferenceProof), PrefabComponentApplyPreparationError<E>>
    {
        use PrefabComponentApplyPreparationError as Error;
        let limit = ByteLimit::new(self.remaining).ok_or(Error::WorkLimit)?;
        let mut total = self.options.limits.working_bytes();
        let retained = total
            .get()
            .checked_sub(self.remaining)
            .ok_or(Error::WorkLimit)?;
        let mut alias_changed = false;
        let mut exhausted = false;
        let SelectedInstanceCompilation {
            inspection,
            component_apply_references,
            diagnostics,
            failure,
            work_bytes,
            ..
        } = try_compile_selected_authoring_instance_with_loader(
            source,
            self.anchor,
            SelectedInstanceConsumer::ComponentApply {
                scope,
                reference_queries: &self.queries,
                initial_reference_work: self.reference_work,
            },
            self.basis,
            self.options
                .with_limits(self.options.limits.with_working_bytes(limit)),
            None,
            |asset, working| {
                if exhausted || alias_changed {
                    return Ok(None);
                }
                let Some(mut loading) = working.including_retained_working_bytes(retained) else {
                    exhausted = true;
                    return Ok(None);
                };
                let mut loaded = (self.loader)(asset, &mut loading)?;
                total = total.min(loading.working_byte_limit());
                let Some(available) = total.get().checked_sub(retained).and_then(ByteLimit::new)
                else {
                    exhausted = true;
                    return Ok(None);
                };
                working.restrict_working_bytes(available);
                if let Some(loaded) = loaded.as_mut()
                    && loaded.key() == self.destination_key
                {
                    if !std::ptr::eq(loaded.document(), self.destination_document) {
                        alias_changed = true;
                        return Ok(None);
                    }
                    if let Some(replacement) = replacement {
                        loaded.replace_authoring_source_document(
                            Arc::clone(replacement),
                            self.basis.clone(),
                        );
                    }
                }
                Ok::<_, E>(loaded)
            },
        )
        .map_err(|error| Error::Compilation(AuthoringPrefabInspectionError::Loader(error)))?;
        if alias_changed {
            return Err(Error::InvalidDestination);
        }
        if exhausted {
            return Err(Error::WorkLimit);
        }
        let inspection = inspection.ok_or_else(|| {
            Error::Compilation(AuthoringPrefabInspectionError::Compilation {
                failure: failure
                    .unwrap_or_else(|| SceneCompilationFailureKind::OwnerAdmission.into()),
                diagnostics,
            })
        })?;
        self.options.limits = self.options.limits.with_working_bytes(total);
        self.remaining = total.get().checked_sub(retained).ok_or(Error::WorkLimit)?;
        spend(&mut self.remaining, work_bytes).ok_or(Error::WorkLimit)?;
        if component_apply_references.len() != self.queries.len() {
            return Err(Error::NonPortableReference);
        }
        Ok((inspection, component_apply_references))
    }
}

fn rebase_record<E>(
    record: &SceneComponentRecord,
    prefix: &[crate::PrefabInstanceHop],
    queries: &mut BTreeSet<AuthoredEntityReference>,
    reference_work: &mut (usize, usize),
) -> Result<SceneComponentRecord, PrefabComponentApplyPreparationError<E>> {
    use PrefabComponentApplyPreparationError as Error;
    let mut rewrite = |reference: &EntityReference| match reference {
        EntityReference::Persistent { .. } => Ok(reference.clone()),
        EntityReference::SceneLocal { .. } => Err(Error::NonPortableReference),
        EntityReference::Authored {
            reference: authored,
        } => {
            let suffix = authored
                .through
                .strip_prefix(prefix)
                .ok_or(Error::NonPortableReference)?;
            if !queries.contains(authored) {
                let cost = reference.cost();
                reference_work.0 = reference_work
                    .0
                    .checked_add(cost.nodes())
                    .ok_or(Error::WorkLimit)?;
                reference_work.1 = reference_work
                    .1
                    .checked_add(cost.logical_bytes())
                    .ok_or(Error::WorkLimit)?;
                queries.insert(authored.clone());
            }
            Ok(EntityReference::Authored {
                reference: AuthoredEntityReference {
                    through: suffix.to_vec(),
                    target: authored.target.clone(),
                },
            })
        }
    };
    let mut rebased = SceneComponentRecord::new(
        record.version,
        copy_value_with_references(record.value().into(), &mut rewrite)?,
    );
    rebased.owner.clone_from(&record.owner);
    for (field, value) in &record.contextual_defaults {
        rebased.contextual_defaults.insert(
            field.clone(),
            copy_value_with_references(value.into(), &mut rewrite)?,
        );
    }
    Ok(rebased)
}

fn verifies_inheritance(
    original: &CapturedPrefabInspection,
    candidate: &CapturedPrefabInspection,
    request: &PrefabComponentApplyRequest<'_>,
    destination: &SourceDocumentKey,
    entity: &SceneEntityId,
    kind: AuthoringPrefabComponentOverrideKind,
) -> bool {
    let Some(candidate_components) = candidate
        .targets
        .get(request.selected)
        .and_then(|entity| candidate.entities.get(entity))
    else {
        return false;
    };
    let actual = candidate_components.get(request.component);
    if kind == AuthoringPrefabComponentOverrideKind::Suppressed {
        return actual.is_none();
    }
    let Some((actual, expected)) = actual.zip(
        original
            .targets
            .get(request.selected)
            .and_then(|entity| original.entities.get(entity))
            .and_then(|components| components.get(request.component)),
    ) else {
        return false;
    };
    let Some((actual_value, expected_value)) =
        actual.payload.as_ref().zip(expected.payload.as_ref())
    else {
        return false;
    };
    matches!(actual.fact.record(), AuthoringPrefabRecordOrigin::SourceEntity {
        source: PrefabDeclarationSource::Prefab(source),
        entity: owner_entity,
    } if source == destination && owner_entity == entity)
        && actual.claims.is_empty()
        && actual.fact.owner() == expected.fact.owner()
        && actual.fact.version() == expected.fact.version()
        && actual.defaulted_roots == expected.defaulted_roots
        && resolved_values_match(
            actual_value.value(),
            &candidate.references,
            expected_value.value(),
            &original.references,
        )
}

fn resolved_values_match(
    actual: ComponentValueRef<'_>,
    actual_refs: &BTreeMap<SceneEntityId, EntityReference>,
    expected: ComponentValueRef<'_>,
    expected_refs: &BTreeMap<SceneEntityId, EntityReference>,
) -> bool {
    fn endpoint<'a>(
        reference: &'a EntityReference,
        resolved: &'a BTreeMap<SceneEntityId, EntityReference>,
    ) -> Option<&'a EntityReference> {
        match reference {
            EntityReference::SceneLocal { entity } => resolved.get(entity),
            EntityReference::Persistent { .. } => Some(reference),
            EntityReference::Authored { .. } => None,
        }
    }
    if let Some(actual) = actual.as_entity_reference() {
        return endpoint(actual, actual_refs).is_some_and(|actual| {
            expected
                .as_entity_reference()
                .and_then(|expected| endpoint(expected, expected_refs))
                == Some(actual)
        });
    }
    if let Some(actual) = actual.as_map() {
        return expected.as_map().is_some_and(|expected| {
            actual.len() == expected.len()
                && actual.iter().zip(expected.iter()).all(
                    |((actual_key, actual), (expected_key, expected))| {
                        actual_key == expected_key
                            && resolved_values_match(actual, actual_refs, expected, expected_refs)
                    },
                )
        });
    }
    if let Some(actual) = actual.as_list() {
        return expected.as_list().is_some_and(|expected| {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected.iter())
                    .all(|(actual, expected)| {
                        resolved_values_match(actual, actual_refs, expected, expected_refs)
                    })
        });
    }
    actual
        .as_value()
        .zip(expected.as_value())
        .is_some_and(|(actual, expected)| component_values_exactly_match(actual, expected))
}
