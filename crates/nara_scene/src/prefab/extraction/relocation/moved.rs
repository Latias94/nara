//! References in declarations moving from the original source into the extracted Prefab.
//!
//! Outward dependencies become explicit inputs with arguments in the original lexical scope.
//! This owner proposes declarations; the complete corpus/occurrence proof still admits their use.

use std::collections::BTreeMap;

use nara_identity::ReferenceInputId;

use super::*;
use crate::prefab::extraction::work::reference_clone_bytes;
use crate::prefab::extraction::{ExtractionError, ExtractionWork};

pub(in crate::prefab::extraction) struct MovedReferenceInputs<'relocation, 'cut, 'source> {
    relocation: &'relocation BranchRelocation<'cut, 'source>,
    arguments: BTreeMap<ReferenceInputId, EntityReference>,
}

impl<'relocation, 'cut, 'source> MovedReferenceInputs<'relocation, 'cut, 'source> {
    pub(in crate::prefab::extraction) fn new(
        relocation: &'relocation BranchRelocation<'cut, 'source>,
    ) -> Self {
        Self {
            relocation,
            arguments: BTreeMap::new(),
        }
    }

    pub(in crate::prefab::extraction) fn arguments(
        &self,
    ) -> &BTreeMap<ReferenceInputId, EntityReference> {
        &self.arguments
    }

    pub(in crate::prefab::extraction) fn into_arguments(
        self,
    ) -> BTreeMap<ReferenceInputId, EntityReference> {
        self.arguments
    }
    pub(in crate::prefab::extraction) fn rewrite<'reference, E>(
        &mut self,
        reference: &'reference EntityReference,
        mut admit: impl FnMut(Option<usize>) -> Result<(), E>,
    ) -> Result<Cow<'reference, EntityReference>, E> {
        let argument = match reference_meaning(self.relocation, reference, &mut admit)? {
            MovedReferenceMeaning::Resolved(reference) => return Ok(reference),
            MovedReferenceMeaning::Argument(argument) => argument,
        };
        for (input, previous) in &self.arguments {
            if same_argument(previous, argument.as_ref(), &mut admit)? {
                admit(input_reference_bytes(input))?;
                return Ok(Cow::Owned(input_reference(input.clone())));
            }
        }

        // Names belong only to this freshly constructed destination. No incumbent input table
        // is merged or shadowed, and iteration order fixes names for deterministic preparation.
        admit(Some(64))?;
        let input = ReferenceInputId::new(format!("extracted.{}", self.arguments.len()))
            .expect("a usize decimal fits the fixed valid input-name grammar");
        let argument_copy = match &argument {
            Cow::Borrowed(reference) => {
                admit(reference_scan_bytes(reference))?;
                reference_clone_bytes(reference)
            }
            // Retained route rewriting has already admitted and owns this allocation.
            Cow::Owned(_) => Some(0),
        };
        admit(
            argument_copy
                .and_then(|bytes| bytes.checked_add(256))
                .and_then(|bytes| bytes.checked_add(input.as_str().len().checked_mul(2)?))
                .and_then(|bytes| bytes.checked_add(input_reference_bytes(&input)?)),
        )?;
        let output = input_reference(input.clone());
        let argument = argument.into_owned();
        // No fallible admission follows mutation. A refusal preserves every previous binding.
        self.arguments.insert(input, argument);
        Ok(Cow::Owned(output))
    }
}

/// Stored names are selected by their already reviewed argument, independently of record order.
/// Validation consumes each declared input without allocating replacement input identities.
pub(in crate::prefab::extraction) struct StoredMovedReferenceInputs<
    'relocation,
    'cut,
    'source,
    'args,
> {
    relocation: &'relocation BranchRelocation<'cut, 'source>,
    arguments: &'args BTreeMap<ReferenceInputId, EntityReference>,
    used: Vec<bool>,
}

impl<'relocation, 'cut, 'source, 'args>
    StoredMovedReferenceInputs<'relocation, 'cut, 'source, 'args>
{
    pub(in crate::prefab::extraction) fn new(
        relocation: &'relocation BranchRelocation<'cut, 'source>,
        arguments: &'args BTreeMap<ReferenceInputId, EntityReference>,
        work: &mut ExtractionWork,
    ) -> Result<Self, ExtractionError> {
        work.admit(
            arguments
                .len()
                .checked_mul(size_of::<bool>())
                .and_then(|bytes| bytes.checked_add(size_of::<Self>())),
        )?;
        Ok(Self {
            relocation,
            arguments,
            used: vec![false; arguments.len()],
        })
    }

    pub(in crate::prefab::extraction) fn rewrite<'reference>(
        &mut self,
        reference: &'reference EntityReference,
        work: &mut ExtractionWork,
    ) -> Result<Cow<'reference, EntityReference>, ExtractionError> {
        let mut admit = |bytes| work.admit(bytes);
        let argument = match reference_meaning(self.relocation, reference, &mut admit)? {
            MovedReferenceMeaning::Resolved(reference) => return Ok(reference),
            MovedReferenceMeaning::Argument(argument) => argument,
        };
        for (index, (input, previous)) in self.arguments.iter().enumerate() {
            if same_argument(previous, argument.as_ref(), &mut admit)? {
                admit(input_reference_bytes(input))?;
                let result = input_reference(input.clone());
                self.used[index] = true;
                return Ok(Cow::Owned(result));
            }
        }
        Err(ExtractionError::InvalidSource)
    }

    pub(in crate::prefab::extraction) fn finish(
        self,
        declared: &std::collections::BTreeSet<ReferenceInputId>,
        work: &mut ExtractionWork,
    ) -> Result<(), ExtractionError> {
        work.visit(self.used.len(), 0, 0)?;
        if self.arguments.len() != declared.len() || self.used.iter().any(|used| !used) {
            return Err(ExtractionError::InvalidSource);
        }
        for (argument, declared) in self.arguments.keys().zip(declared) {
            work.visit(
                1,
                argument
                    .as_str()
                    .len()
                    .saturating_add(declared.as_str().len()),
                0,
            )?;
            if argument != declared {
                return Err(ExtractionError::InvalidSource);
            }
        }
        Ok(())
    }
}

enum MovedReferenceMeaning<'reference> {
    Resolved(Cow<'reference, EntityReference>),
    Argument(Cow<'reference, EntityReference>),
}

fn reference_meaning<'reference, E>(
    relocation: &BranchRelocation<'_, '_>,
    reference: &'reference EntityReference,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<MovedReferenceMeaning<'reference>, E> {
    if matches!(reference, EntityReference::Persistent { .. }) {
        return Ok(MovedReferenceMeaning::Resolved(Cow::Borrowed(reference)));
    }
    let local = match reference {
        EntityReference::SceneLocal { entity } => relocation.source_entity_crosses(
            relocation.original,
            entity,
            AddressRole::InstanceSlot,
            admit,
        )?,
        EntityReference::Authored { reference } => {
            let origin = reference
                .through
                .first()
                .map(|hop| &hop.anchor)
                .or_else(|| match &reference.target {
                    AuthoredReferenceTarget::Entity(entity) => Some(entity),
                    AuthoredReferenceTarget::Input(_) => None,
                });
            match origin {
                Some(origin) => relocation.crosses(
                    relocation.original,
                    origin,
                    AddressRole::InstanceSlot,
                    admit,
                )?,
                None => false,
            }
        }
        EntityReference::Persistent { .. } => unreachable!("handled above"),
    };
    if local {
        // The first environment moved; later explicitly entered P occurrences still relocate.
        relocation
            .retained_reference(relocation.destination, reference, admit)
            .map(MovedReferenceMeaning::Resolved)
    } else {
        relocation
            .retained_reference(relocation.original, reference, admit)
            .map(MovedReferenceMeaning::Argument)
    }
}

fn same_argument<E>(
    previous: &EntityReference,
    argument: &EntityReference,
    admit: &mut impl FnMut(Option<usize>) -> Result<(), E>,
) -> Result<bool, E> {
    // Admit authored-hop cost scans from O(1) lengths before inspecting either value.
    admit(
        reference_scan_bytes(previous)
            .and_then(|bytes| bytes.checked_add(reference_scan_bytes(argument)?)),
    )?;
    admit(
        previous
            .cost()
            .logical_bytes()
            .checked_add(argument.cost().logical_bytes())
            .and_then(|bytes| bytes.checked_add(size_of::<EntityReference>())),
    )?;
    Ok(previous == argument)
}

fn input_reference(input: ReferenceInputId) -> EntityReference {
    EntityReference::Authored {
        reference: AuthoredEntityReference::input(input),
    }
}

fn reference_scan_bytes(reference: &EntityReference) -> Option<usize> {
    let hops = match reference {
        EntityReference::Authored { reference } => reference.through.len(),
        _ => 0,
    };
    hops.checked_add(1)?
        .checked_mul(size_of::<PrefabInstanceHop>())
}

fn input_reference_bytes(input: &ReferenceInputId) -> Option<usize> {
    size_of::<EntityReference>()
        .checked_add(size_of::<AuthoredEntityReference>())?
        .checked_add(input.as_str().len())
}
