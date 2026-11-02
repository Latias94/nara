//! Occurrence-local input arguments use the same authored route interpreter as payload leaves.

use std::borrow::Cow;
use std::mem::size_of;

use nara_asset::AssetRef;
use nara_identity::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReference, ReferenceInputId,
};

use super::{Compiler, NodeId, ScopeId, Site};
use crate::diagnostics::{error as diagnostic_error, usize_to_u64, with_public_u64};
use crate::prefab::PrefabExpansionWorkingSet;
use crate::provenance::provenance_string_bytes;
use crate::{
    PrefabEntityOrigin, PrefabExpansionFailure, PrefabExpansionFailureKind, PrefabInstanceHop,
    ResolvedPrefabSource,
};

pub(super) enum ResolvedReference {
    Local(NodeId),
    Persistent(EntityReference),
}

#[derive(Clone, Copy)]
pub(super) enum InputFailure {
    Undeclared,
    MissingArgument,
    Cycle,
}

pub(super) enum ReferenceResolution {
    Entered,
    Invalid {
        reason: PrefabExpansionFailureKind,
        input: Option<(ScopeId, InputFailure)>,
    },
    Resolved {
        reference: ResolvedReference,
        declaring_scope: ScopeId,
    },
}

impl ReferenceResolution {
    fn invalid(reason: PrefabExpansionFailureKind) -> Self {
        Self::Invalid {
            reason,
            input: None,
        }
    }

    fn input_failure(scope: ScopeId, failure: InputFailure) -> Self {
        Self::Invalid {
            reason: PrefabExpansionFailureKind::Reference,
            input: Some((scope, failure)),
        }
    }
}

impl<'root, 'context, 'owner, F, E> Compiler<'root, 'context, 'owner, F, E>
where
    F: FnMut(&AssetRef, &mut PrefabExpansionWorkingSet) -> Result<Option<ResolvedPrefabSource>, E>,
{
    /// Admission of an opened occurrence checks every provided name, including unused arguments.
    /// Missing values remain legal until a final surviving leaf demands that input.
    pub(super) fn admit_input_arguments(
        &mut self,
        site: &Site,
        source: &ResolvedPrefabSource,
    ) -> Result<(), PrefabExpansionFailure> {
        let inputs = &source.document().inputs;
        let mut bytes = inputs.iter().try_fold(0usize, |bytes, input| {
            bytes
                .checked_add(128)?
                .checked_add(provenance_string_bytes(input.as_str())?)
        });
        if let Some(arguments) = self
            .instance(site)
            .and_then(|instance| instance.arguments.as_ref())
        {
            bytes = bytes.and_then(|bytes| {
                arguments.values.iter().try_fold(
                    bytes.checked_add(arguments.base.checked_retained_bytes()?)?,
                    |bytes, (input, reference)| {
                        bytes
                            .checked_add(128)?
                            .checked_add(provenance_string_bytes(input.as_str())?)?
                            .checked_add(reference_clone_bytes(reference)?)
                    },
                )
            });
        }
        self.admit(bytes)?;
        let arguments = self
            .instance(site)
            .and_then(|instance| instance.arguments.as_ref());
        if let Some(arguments) = arguments {
            if &arguments.base != source.key() {
                self.context.diagnostics.push(diagnostic_error(
                    "scene.prefab-input-base-changed",
                    "Reference arguments do not match the selected Prefab source",
                ));
                return Err(PrefabExpansionFailureKind::SourceChanged.into());
            }
            if arguments.values.keys().any(|input| !inputs.contains(input)) {
                self.context.diagnostics.push(diagnostic_error(
                    "scene.prefab-input-argument-undeclared",
                    "Reference arguments contain an input not declared by the selected Prefab",
                ));
                return Err(PrefabExpansionFailureKind::Reference.into());
            }
        }
        Ok(())
    }

    fn input_argument(&self, scope: ScopeId, input: &ReferenceInputId) -> Option<&EntityReference> {
        self.scopes[scope]
            .site
            .as_ref()
            .and_then(|site| self.instance(site))?
            .arguments
            .as_ref()?
            .values
            .get(input)
    }

    /// A binding chain has one successor at a time. Its active keys are actual occurrences,
    /// never canonical source keys; resolving another leaf starts a fresh chain.
    pub(super) fn reference_target(
        &mut self,
        scope: ScopeId,
        target: &AuthoredEntityReference,
    ) -> Result<ReferenceResolution, PrefabExpansionFailure> {
        let mut declaring_scope = scope;
        let mut reference = Cow::Borrowed(target);
        let mut active = Vec::<(ScopeId, ReferenceInputId)>::new();
        loop {
            let mut current = declaring_scope;
            for hop in &reference.through {
                self.admit(Some(size_of::<ScopeId>() + size_of::<NodeId>()))?;
                let Some(&anchor) = self.scopes[current].nodes.get(&hop.anchor) else {
                    return Ok(ReferenceResolution::invalid(
                        PrefabExpansionFailureKind::MissingTarget,
                    ));
                };
                let previous = self.scopes.len();
                let entered = self.open_slot(anchor)?;
                if previous != self.scopes.len() {
                    return Ok(ReferenceResolution::Entered);
                }
                let Some(entered) = entered else {
                    return Ok(ReferenceResolution::invalid(
                        PrefabExpansionFailureKind::MissingTarget,
                    ));
                };
                current = entered;
                if self.scopes[current]
                    .source
                    .as_ref()
                    .is_none_or(|source| source.key() != &hop.expected_base)
                {
                    return Ok(ReferenceResolution::invalid(
                        PrefabExpansionFailureKind::SourceChanged,
                    ));
                }
            }
            let input = match &reference.target {
                AuthoredReferenceTarget::Entity(entity) => {
                    return Ok(match self.scopes[current].nodes.get(entity) {
                        Some(&node) => ReferenceResolution::Resolved {
                            reference: ResolvedReference::Local(node),
                            declaring_scope,
                        },
                        None => {
                            ReferenceResolution::invalid(PrefabExpansionFailureKind::MissingTarget)
                        }
                    });
                }
                AuthoredReferenceTarget::Input(input) => input,
            };
            let inputs = match self.scopes[current].source.as_ref() {
                Some(source) => Some(&source.document().inputs),
                None if current == 0 => self.root.inputs,
                None => None,
            };
            if !inputs.is_some_and(|inputs| inputs.contains(input)) {
                return Ok(ReferenceResolution::input_failure(
                    current,
                    InputFailure::Undeclared,
                ));
            }
            if active
                .iter()
                .any(|(scope, previous)| *scope == current && previous == input)
            {
                return Ok(ReferenceResolution::input_failure(
                    current,
                    InputFailure::Cycle,
                ));
            }
            let maximum = self.context.materialization_limits().depth().get();
            if active.len() >= maximum {
                self.context.diagnostics.push(with_public_u64(
                    diagnostic_error(
                        "scene.prefab-input-depth-exceeded",
                        "Reference input resolution exceeded its admitted chain depth",
                    ),
                    "maximum-depth",
                    usize_to_u64(maximum),
                ));
                return Err(PrefabExpansionFailureKind::WorkLimit.into());
            }
            let Some(argument) = self.input_argument(current, input) else {
                return Ok(ReferenceResolution::input_failure(
                    current,
                    InputFailure::MissingArgument,
                ));
            };
            self.admit(reference_clone_bytes(argument).and_then(|bytes| {
                bytes
                    .checked_add(size_of::<(ScopeId, ReferenceInputId)>())?
                    .checked_add(provenance_string_bytes(input.as_str())?)
            }))?;
            active.push((current, input.clone()));
            let argument = self
                .input_argument(current, input)
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?
                .clone();
            declaring_scope = self.scopes[current]
                .site
                .as_ref()
                .ok_or(PrefabExpansionFailureKind::OwnerAdmission)?
                .source;
            match argument {
                EntityReference::Authored {
                    reference: argument,
                } => reference = Cow::Owned(argument),
                EntityReference::SceneLocal { entity } => {
                    return Ok(
                        match self.scopes[declaring_scope]
                            .nodes
                            .get(&PrefabEntityOrigin::SourceEntity(entity))
                        {
                            Some(&node) => ReferenceResolution::Resolved {
                                reference: ResolvedReference::Local(node),
                                declaring_scope,
                            },
                            None => ReferenceResolution::invalid(
                                PrefabExpansionFailureKind::MissingTarget,
                            ),
                        },
                    );
                }
                argument @ EntityReference::Persistent { .. } => {
                    return Ok(ReferenceResolution::Resolved {
                        reference: ResolvedReference::Persistent(argument),
                        declaring_scope,
                    });
                }
            }
        }
    }

    pub(super) fn push_input_failure(&mut self, failure: InputFailure) {
        let (code, message) = match failure {
            InputFailure::Undeclared => (
                "scene.prefab-input-undeclared",
                "A surviving reference uses an input not declared by its occurrence",
            ),
            InputFailure::MissingArgument => (
                "scene.prefab-input-argument-missing",
                "A surviving reference input has no argument in its actual occurrence",
            ),
            InputFailure::Cycle => (
                "scene.prefab-input-cycle",
                "Reference input arguments form a cycle on the active resolution chain",
            ),
        };
        self.context
            .diagnostics
            .push(diagnostic_error(code, message));
    }
}

pub(super) fn authored_clone_bytes(reference: &AuthoredEntityReference) -> Option<usize> {
    let endpoint = match &reference.target {
        AuthoredReferenceTarget::Entity(entity) => entity.entity().as_str(),
        AuthoredReferenceTarget::Input(input) => input.as_str(),
    };
    reference.through.iter().try_fold(
        size_of::<AuthoredEntityReference>().checked_add(provenance_string_bytes(endpoint)?)?,
        |bytes, hop| {
            bytes
                .checked_add(size_of::<PrefabInstanceHop>())?
                .checked_add(provenance_string_bytes(hop.anchor.entity().as_str())?)?
                .checked_add(hop.expected_base.checked_retained_bytes()?)
        },
    )
}

pub(super) fn reference_clone_bytes(reference: &EntityReference) -> Option<usize> {
    size_of::<EntityReference>().checked_add(match reference {
        EntityReference::SceneLocal { entity } => provenance_string_bytes(entity.as_str())?,
        EntityReference::Persistent { entity } => {
            provenance_string_bytes(entity.namespace.as_str())?
        }
        EntityReference::Authored { reference } => authored_clone_bytes(reference)?,
    })
}
