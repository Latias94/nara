//! Bounded pending work revealed by already admitted members, shared by preparation and Check.

use std::{cmp::Reverse, collections::BinaryHeap, mem::size_of};

use super::{
    DecodeLease, RuntimeCodecDefinitionId, RuntimeContentBase, RuntimeContentBudgetKind,
    RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentMemberKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PendingMember {
    Scene(usize),
    Asset(RuntimeCodecDefinitionId, usize),
}

impl PendingMember {
    fn index(self) -> usize {
        match self {
            Self::Scene(index) | Self::Asset(_, index) => index,
        }
    }
}

pub(super) struct MemberTraversal {
    seen: Vec<bool>,
    pending: BinaryHeap<Reverse<PendingMember>>,
    _work: DecodeLease,
}

impl MemberTraversal {
    pub(super) fn new(base: &RuntimeContentBase) -> Result<Self, RuntimeContentError> {
        let count = base.catalog().members().len();
        let bytes = count
            .checked_mul(size_of::<bool>() + size_of::<Reverse<PendingMember>>())
            .ok_or(RuntimeContentError::BudgetOverflow {
                kind: RuntimeContentBudgetKind::WorkBytes,
            })?;
        let work = base.inner.budget.reserve(0, bytes)?;
        Ok(Self {
            seen: vec![false; count],
            pending: BinaryHeap::with_capacity(count),
            _work: work,
        })
    }

    pub(super) fn enqueue(
        &mut self,
        base: &RuntimeContentBase,
        index: usize,
    ) -> Result<(), RuntimeContentError> {
        if self.seen[index] {
            return Ok(());
        }
        let member = &base.catalog().members()[index];
        let pending = match member.kind() {
            RuntimeContentMemberKind::Scene { .. } => PendingMember::Scene(index),
            RuntimeContentMemberKind::Asset { .. } => {
                PendingMember::Asset(base.plan().runtime_codecs().definition(member)?, index)
            }
        };
        self.seen[index] = true;
        self.pending.push(Reverse(pending));
        Ok(())
    }

    pub(super) fn next(&mut self) -> Option<usize> {
        self.pending.pop().map(|Reverse(member)| member.index())
    }

    /// Call only after the actual member has passed complete payload/dependency admission.
    pub(super) fn follow_admitted(
        &mut self,
        base: &RuntimeContentBase,
        index: usize,
        include_deferred: bool,
    ) -> Result<(), RuntimeContentError> {
        for dependency in base.catalog().members()[index].dependencies() {
            let demanded = match dependency.role() {
                RuntimeContentDependencyRole::Required => true,
                RuntimeContentDependencyRole::Reference => include_deferred,
                RuntimeContentDependencyRole::Soft => false,
            };
            if demanded {
                self.enqueue(base, base.member_index(dependency.product())?)?;
            }
        }
        Ok(())
    }
}
