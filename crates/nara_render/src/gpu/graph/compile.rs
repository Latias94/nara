//! Sparse, deterministic serial compilation for one complete Host candidate.

use std::{cmp::Reverse, collections::BinaryHeap};

use super::{CompiledGraph, GraphBuilder, GraphError, LimitKind, Origin, reserved};

pub(super) type ReadyOperation = Reverse<(u64, usize)>;

/// One node in a producer's outgoing list per explicit or implicit local dependency. Duplicate
/// reads deliberately retain duplicate edges, with matching indegree increments/decrements.
#[derive(Debug)]
pub(super) struct DependencyEdge {
    consumer: usize,
    next: usize,
}

const NO_EDGE: usize = usize::MAX;

impl GraphBuilder {
    pub(crate) fn compile(self) -> Result<CompiledGraph, GraphError> {
        if self.outputs.is_empty() {
            return Err(GraphError::MissingOutputGroup);
        }
        // Unused reservations are errors too. No unresolved resource becomes implicitly imported
        // or silently initialized merely because a different output has a complete dependency path.
        for resource in &self.resources {
            if matches!(resource.origin, Origin::Local { producer: None }) {
                return Err(GraphError::MissingProducer(resource.version.index));
            }
        }

        // Count the actual sparse scratch before allocating any of it. Its capacity upper bound
        // was included in GraphLimits::modeled_metadata_bytes before builder construction.
        let mut edge_count = 0usize;
        for operation in &self.operations {
            for version in operation.dependencies() {
                if self.resources[version.index].origin.producer().is_some() {
                    edge_count = edge_count
                        .checked_add(1)
                        .filter(|count| *count <= self.limits.max_accesses)
                        .ok_or(GraphError::LimitExceeded(LimitKind::Accesses))?;
                }
            }
        }
        let count = self.operations.len();
        let mut indegrees = reserved(count)?;
        indegrees.resize(count, 0usize);
        let mut heads = reserved(count)?;
        heads.resize(count, NO_EDGE);
        let mut edges = reserved(edge_count)?;
        let mut order = reserved(count)?;
        let mut ready: BinaryHeap<ReadyOperation> = BinaryHeap::new();
        ready
            .try_reserve_exact(count)
            .map_err(|_| GraphError::AllocationFailed)?;

        for (consumer, operation) in self.operations.iter().enumerate() {
            for version in operation.dependencies() {
                if let Some(producer) = self.resources[version.index].origin.producer() {
                    edges.push(DependencyEdge {
                        consumer,
                        next: heads[producer],
                    });
                    heads[producer] = edges.len() - 1;
                    indegrees[consumer] += 1;
                }
            }
        }
        for (index, indegree) in indegrees.iter().enumerate() {
            if *indegree == 0 {
                ready.push(Reverse((self.operations[index].key, index)));
            }
        }
        // Each operation enters the heap once, and every dependency visits one sparse edge.
        // Keys, rather than declaration indices or outgoing-list order, break all ready ties.
        while let Some(Reverse((_, producer))) = ready.pop() {
            order.push(producer);
            let mut edge = heads[producer];
            while edge != NO_EDGE {
                let dependency = &edges[edge];
                let consumer = dependency.consumer;
                indegrees[consumer] -= 1;
                if indegrees[consumer] == 0 {
                    ready.push(Reverse((self.operations[consumer].key, consumer)));
                }
                edge = dependency.next;
            }
        }
        if order.len() != count {
            return Err(GraphError::Cycle);
        }
        Ok(CompiledGraph {
            owner: self.owner,
            resources: self.resources,
            operations: self.operations,
            order,
            outputs: self.outputs,
            modeled_metadata_bytes: self.modeled_metadata_bytes,
            modeled_resource_bytes: self.modeled_resource_bytes,
        })
    }
}
