//! Proposed address rewrites from one exact source-owned cut.
//!
//! Entity continuity and instance-slot continuity differ at the retained root anchor. These
//! rewrites construct authored intent; they do not prove a route against the admitted corpus.

use std::{borrow::Cow, mem::size_of};

use nara_identity::{AuthoredEntityReference, AuthoredReferenceTarget, EntityReference};

use crate::{
    PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceBinding, PrefabInstanceHop,
    SceneEntityId, SourceDocumentKey, SourceDocumentKind,
};

use super::{SourceCut, SourceDocumentInput};

mod moved;
pub(super) use moved::{MovedReferenceInputs, StoredMovedReferenceInputs};

#[cfg(test)]
mod compiler_tests;
#[cfg(test)]
mod moved_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddressRole {
    Entity,
    InstanceSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RelocationSetupError {
    SourceKind,
    DestinationKind,
    IdentityCollision,
}

pub(super) struct BranchRelocation<'cut, 'source> {
    cut: &'cut SourceCut<'source>,
    original: &'cut SourceDocumentKey,
    destination: &'cut SourceDocumentKey,
}

impl<'cut, 'source> BranchRelocation<'cut, 'source> {
    pub(super) fn original(&self) -> &SourceDocumentKey {
        self.original
    }

    pub(super) fn new(
        cut: &'cut SourceCut<'source>,
        original: &'cut SourceDocumentKey,
        destination: &'cut SourceDocumentKey,
    ) -> Result<Self, RelocationSetupError> {
        let kind = match cut.source() {
            SourceDocumentInput::Scene(_) => SourceDocumentKind::Scene,
            SourceDocumentInput::Prefab(_) => SourceDocumentKind::Prefab,
        };
        if original.kind() != kind {
            return Err(RelocationSetupError::SourceKind);
        }
        if destination.kind() != SourceDocumentKind::Prefab {
            return Err(RelocationSetupError::DestinationKind);
        }
        if original == destination {
            return Err(RelocationSetupError::IdentityCollision);
        }
        Ok(Self {
            cut,
            original,
            destination,
        })
    }

    /// Rewrites a target whose declaration stays in its original source environment.
    ///
    /// The root's effective entity remains at P:k, while its original instance slot moves to
    /// Q:k. A route entering that old slot therefore needs the Q hop even though an entity
    /// reference ending at k does not. The source owner supplies the actual initial scope;
    /// matching later predicates proposes rewrites, not authority to enter those occurrences.
    pub(super) fn retained_target<'target, E>(
        &self,
        scope: &SourceDocumentKey,
        target: &'target PrefabEntityTarget,
        role: AddressRole,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Cow<'target, PrefabEntityTarget>, E> {
        let through = self.retained_route(
            scope,
            &target.through,
            Some((&target.entity, role)),
            size_of::<PrefabEntityTarget>()
                .checked_add(origin_bytes(&target.entity).unwrap_or(usize::MAX)),
            &mut admit,
        )?;
        Ok(match through {
            None => Cow::Borrowed(target),
            Some(through) => Cow::Owned(PrefabEntityTarget {
                through,
                entity: target.entity.clone(),
            }),
        })
    }

    /// Re-expresses source-owned structural intent in Q only when its initial coordinate moves
    /// with this cut. External structural dependencies stay in the containing source's intent.
    pub(super) fn moved_target<'target, E>(
        &self,
        target: &'target PrefabEntityTarget,
        role: AddressRole,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Option<Cow<'target, PrefabEntityTarget>>, E> {
        let origin = target
            .through
            .first()
            .map_or(&target.entity, |hop| &hop.anchor);
        if !self.crosses(self.original, origin, AddressRole::InstanceSlot, &mut admit)? {
            return Ok(None);
        }
        self.retained_target(self.destination, target, role, admit)
            .map(Some)
    }

    pub(super) fn retained_binding<'binding, E>(
        &self,
        binding: &'binding PrefabInstanceBinding,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Cow<'binding, PrefabInstanceBinding>, E> {
        let PrefabInstanceBinding::Root { base, root } = binding else {
            return Ok(Cow::Borrowed(binding));
        };
        match self.retained_target(base, root, AddressRole::Entity, &mut admit)? {
            Cow::Borrowed(_) => Ok(Cow::Borrowed(binding)),
            Cow::Owned(root) => {
                admit(
                    size_of::<PrefabInstanceBinding>()
                        .checked_add(base.checked_retained_bytes().unwrap_or(usize::MAX)),
                )?;
                Ok(Cow::Owned(PrefabInstanceBinding::Root {
                    base: base.clone(),
                    root,
                }))
            }
        }
    }

    /// Payload references use their actual lexical declaration scope, independently of an
    /// overlay operation's structural target. Inputs and Persistent references keep their role.
    pub(super) fn retained_reference<'reference, E>(
        &self,
        scope: &SourceDocumentKey,
        reference: &'reference EntityReference,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Cow<'reference, EntityReference>, E> {
        match reference {
            EntityReference::Persistent { .. } => Ok(Cow::Borrowed(reference)),
            EntityReference::SceneLocal { entity } => {
                if !self.source_entity_crosses(scope, entity, AddressRole::Entity, &mut admit)? {
                    return Ok(Cow::Borrowed(reference));
                }
                admit(
                    size_of::<EntityReference>()
                        .checked_add(size_of::<AuthoredEntityReference>())
                        .and_then(|bytes| bytes.checked_add(entity.as_str().len()))
                        .and_then(|bytes| bytes.checked_add(self.destination_hop_bytes()?)),
                )?;
                Ok(Cow::Owned(EntityReference::Authored {
                    reference: AuthoredEntityReference {
                        through: vec![self.destination_hop()],
                        target: AuthoredReferenceTarget::Entity(PrefabEntityOrigin::SourceEntity(
                            entity.clone(),
                        )),
                    },
                }))
            }
            EntityReference::Authored {
                reference: authored,
            } => {
                let terminal = match &authored.target {
                    AuthoredReferenceTarget::Entity(entity) => Some((entity, AddressRole::Entity)),
                    AuthoredReferenceTarget::Input(_) => None,
                };
                let endpoint_bytes = match &authored.target {
                    AuthoredReferenceTarget::Entity(entity) => origin_bytes(entity),
                    AuthoredReferenceTarget::Input(input) => {
                        size_of::<AuthoredReferenceTarget>().checked_add(input.as_str().len())
                    }
                };
                let through = self.retained_route(
                    scope,
                    &authored.through,
                    terminal,
                    endpoint_bytes.and_then(|bytes| {
                        bytes
                            .checked_add(size_of::<EntityReference>())?
                            .checked_add(size_of::<AuthoredEntityReference>())
                    }),
                    &mut admit,
                )?;
                Ok(match through {
                    None => Cow::Borrowed(reference),
                    Some(through) => Cow::Owned(EntityReference::Authored {
                        reference: AuthoredEntityReference {
                            through,
                            target: authored.target.clone(),
                        },
                    }),
                })
            }
        }
    }

    fn retained_route<E>(
        &self,
        scope: &SourceDocumentKey,
        route: &[PrefabInstanceHop],
        terminal: Option<(&PrefabEntityOrigin, AddressRole)>,
        output_bytes: Option<usize>,
        admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Option<Vec<PrefabInstanceHop>>, E> {
        let mut inserted = 0usize;
        let mut current = scope;
        for hop in route {
            admit(Some(size_of::<PrefabInstanceHop>()))?;
            if self.crosses(current, &hop.anchor, AddressRole::InstanceSlot, admit)? {
                inserted = inserted.saturating_add(1);
            }
            current = &hop.expected_base;
        }
        if let Some((entity, role)) = terminal
            && self.crosses(current, entity, role, admit)?
        {
            inserted = inserted.saturating_add(1);
        }
        if inserted == 0 {
            return Ok(None);
        }

        let bytes = output_bytes
            .and_then(|bytes| {
                route.iter().try_fold(bytes, |bytes, hop| {
                    bytes
                        .checked_add(size_of::<PrefabInstanceHop>())?
                        .checked_add(origin_bytes(&hop.anchor)?)?
                        .checked_add(hop.expected_base.checked_retained_bytes()?)
                })
            })
            .and_then(|bytes| {
                bytes.checked_add(inserted.checked_mul(self.destination_hop_bytes()?)?)
            });
        admit(bytes)?;
        let mut through = Vec::with_capacity(route.len().saturating_add(inserted));
        current = scope;
        for hop in route {
            if self.crosses(current, &hop.anchor, AddressRole::InstanceSlot, admit)? {
                through.push(self.destination_hop());
            }
            through.push(hop.clone());
            current = &hop.expected_base;
        }
        if let Some((entity, role)) = terminal
            && self.crosses(current, entity, role, admit)?
        {
            through.push(self.destination_hop());
        }
        Ok(Some(through))
    }

    fn destination_hop_bytes(&self) -> Option<usize> {
        size_of::<PrefabInstanceHop>()
            .checked_add(self.cut.root().id.as_str().len())?
            .checked_add(self.destination.checked_retained_bytes()?)
    }

    fn destination_hop(&self) -> PrefabInstanceHop {
        PrefabInstanceHop {
            anchor: PrefabEntityOrigin::SourceEntity(self.cut.root().id.clone()),
            expected_base: self.destination.clone(),
        }
    }

    fn crosses<E>(
        &self,
        scope: &SourceDocumentKey,
        origin: &PrefabEntityOrigin,
        role: AddressRole,
        admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<bool, E> {
        let PrefabEntityOrigin::SourceEntity(entity) = origin else {
            return Ok(false);
        };
        self.source_entity_crosses(scope, entity, role, admit)
    }

    fn source_entity_crosses<E>(
        &self,
        scope: &SourceDocumentKey,
        entity: &SceneEntityId,
        role: AddressRole,
        admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<bool, E> {
        admit(
            scope
                .checked_retained_bytes()
                .and_then(|bytes| bytes.checked_add(self.original.checked_retained_bytes()?)),
        )?;
        if scope != self.original {
            return Ok(false);
        }
        admit(
            entity
                .as_str()
                .len()
                .checked_add(self.cut.root().id.as_str().len()),
        )?;
        if role == AddressRole::Entity && entity == &self.cut.root().id {
            return Ok(false);
        }
        self.cut.contains_entity(entity, admit)
    }
}

fn origin_bytes(origin: &PrefabEntityOrigin) -> Option<usize> {
    size_of::<PrefabEntityOrigin>().checked_add(origin.entity().as_str().len())
}
