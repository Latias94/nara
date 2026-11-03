//! Bounded wire decoding shared by live preparation and explicit process recovery.

use std::mem::size_of;

use nara_scene::{
    PrefabDocument, PrefabDocumentCandidate, PrefabInstance, SceneDocument, SceneDocumentCandidate,
    SceneEntityPlacement, SceneEntityRecord, SceneFileDecodingAdmissionError, SceneFileEncoding,
    SceneFileLimits, SceneFilePublicationError, SceneFormatError, ScenePlacementParent,
    ScenePlacementSlot, SourceDocumentKind,
};

use super::{
    AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    project::AuthoringProjectSourceAdmissionError,
};

#[cfg(test)]
mod tests;

pub(super) enum SourceTree {
    Scene(SceneDocument),
    Prefab(PrefabDocument),
}

impl SourceTree {
    pub(super) fn retained_bytes(&self) -> Result<usize, SourceDecodeError> {
        match self {
            Self::Scene(document) => document.retained_bytes(),
            Self::Prefab(document) => document.retained_bytes(),
        }
        .map_err(SourceDecodeError::Project)
    }

    pub(super) fn admit_canonical_comparison(
        &self,
        encoded: usize,
        budget: &mut AuthoringProjectAdmissionBudget,
    ) -> Result<(), SourceDecodeError> {
        let (entities, placements) = match self {
            Self::Scene(document) => (document.entities.as_slice(), document.placements.as_slice()),
            Self::Prefab(document) => {
                (document.entities.as_slice(), document.placements.as_slice())
            }
        };
        // Canonical comparison also validates local placement depth before serialization.
        // Its entity/target indices receive the same key bound as initial source admission;
        // large scalar payloads still participate only in linear domain/serializer work.
        let work = checked_sum([
            self.retained_bytes()?,
            encoded,
            source_index_work(entities, placements)?,
        ])?;
        budget.charge_source_bytes(0, 0, work, 0)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SourceDecodeError {
    #[error("source decoding exceeded the shared Host allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("source decoding exceeded its admitted allowance")]
    Project(#[from] AuthoringProjectSourceAdmissionError),
    #[error("source wire decoding failed")]
    Format(#[from] SceneFormatError),
    #[error("source structure is invalid")]
    Structure(#[source] SceneFilePublicationError),
}

pub(super) fn release_tree(
    tree: SourceTree,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceDecodeError> {
    let bytes = tree.retained_bytes()?;
    drop(tree);
    budget.reconcile_retained_reservation(bytes, 0)?;
    Ok(())
}

pub(super) fn decode_source(
    kind: SourceDocumentKind,
    encoding: SceneFileEncoding,
    bytes: &[u8],
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SourceTree, SourceDecodeError> {
    let before = budget.retained_candidate_bytes;
    let result = (|| {
        let reservation =
            reserve_source_decode(bytes.len(), limits.encoded_bytes().get(), 0, limits, budget)?;
        let tree = match kind {
            SourceDocumentKind::Scene => SceneDocumentCandidate::decode_bytes_with_admission(
                bytes,
                encoding,
                reservation.limits,
                |document| {
                    admit_source_validation(
                        &document.entities,
                        &document.placements,
                        document.retained_bytes()?,
                        bytes.len(),
                        budget,
                    )
                },
            )
            .map_err(|error| decode_error(error, &reservation))?
            .into_composition_source()
            .map(SourceTree::Scene),
            SourceDocumentKind::Prefab => PrefabDocumentCandidate::decode_bytes_with_admission(
                bytes,
                encoding,
                reservation.limits,
                |document| {
                    admit_source_validation(
                        &document.entities,
                        &document.placements,
                        document.retained_bytes()?,
                        bytes.len(),
                        budget,
                    )
                },
            )
            .map_err(|error| decode_error(error, &reservation))?
            .into_composition_source()
            .map(SourceTree::Prefab),
        }
        .map_err(SourceDecodeError::Structure)?;
        let retained = tree.retained_bytes()?;
        budget.reconcile_retained_reservation(reservation.bytes, retained)?;
        budget.charge_source_bytes(0, retained, retained, 0)?;
        Ok(tree)
    })();
    if result.is_err() {
        // Every attempted parser/tree local is gone. Keep cumulative work, not dropped storage.
        let temporary = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        budget.reconcile_retained_reservation(temporary, 0)?;
    }
    result
}

fn decode_error(
    error: SceneFileDecodingAdmissionError<SourceDecodeError>,
    reservation: &CompositionDecodeReservation,
) -> SourceDecodeError {
    match error {
        SceneFileDecodingAdmissionError::Format(error) => {
            match reservation.capacity_error(&error) {
                Some(error) => SourceDecodeError::Admission(error),
                None => SourceDecodeError::Format(error),
            }
        }
        SceneFileDecodingAdmissionError::Admission(error) => error,
    }
}

fn admit_source_validation(
    entities: &[SceneEntityRecord],
    placements: &[SceneEntityPlacement],
    retained: usize,
    encoded: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<(), SourceDecodeError> {
    // The raw payload is still unsorted. Admit canonical ordering and the later local structure
    // indices before either starts, including invalid sources that will fail those checks.
    let work = checked_sum([retained, encoded, source_index_work(entities, placements)?])?;
    budget.charge_source_bytes(0, 0, work, 0)?;
    Ok(())
}

fn source_index_work(
    entities: &[SceneEntityRecord],
    placements: &[SceneEntityPlacement],
) -> Result<usize, SourceDecodeError> {
    let records = checked_sum([entities.len(), placements.len()])?;
    let comparisons = checked_sum([(usize::BITS - records.leading_zeros()) as usize, 2])?;
    let mut keys = records;
    for entity in entities {
        keys = checked_sum([keys, entity.id.as_str().len()])?;
    }
    for placement in placements {
        let parent = match &placement.placement.parent {
            ScenePlacementParent::Root => None,
            ScenePlacementParent::Entity(target) => Some(target),
        };
        let sibling = match &placement.placement.slot {
            ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) => Some(target),
            ScenePlacementSlot::First | ScenePlacementSlot::Last => None,
        };
        for target in std::iter::once(&placement.target)
            .chain(parent)
            .chain(sibling)
        {
            keys = checked_sum([
                keys,
                crate::scene_retention::target_retained_bytes(target)
                    .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            ])?;
        }
    }
    Ok(checked_product(keys, comparisons)?)
}

/// The production decoder enforces this shape ceiling before allocating domain data.
/// Parser/domain memory is reserved separately from the finite wire scan work.
pub(super) struct CompositionDecodeReservation {
    pub(super) bytes: usize,
    pub(super) limits: SceneFileLimits,
    pub(super) constrained_nodes: bool,
    pub(super) constrained_container_items: bool,
    pub(super) maximum_retained: usize,
}

impl CompositionDecodeReservation {
    pub(super) fn capacity_error(
        &self,
        error: &SceneFormatError,
    ) -> Option<AuthoringProjectAdmissionError> {
        let constrained = match error.shape_error() {
            Some(nara_core::SerdeShapeError::NodeLimitExceeded { .. }) => self.constrained_nodes,
            Some(nara_core::SerdeShapeError::ContainerItemLimitExceeded { .. }) => {
                self.constrained_container_items
            }
            _ => false,
        };
        constrained.then_some(
            AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
                maximum: self.maximum_retained,
            },
        )
    }
}

/// Reserves the existing decoder's actual shape ceiling before it constructs owned source data.
/// Borrowed reviewed inputs use the same model as file-backed composition sources.
pub(super) fn reserve_source_decode(
    encoded_len: usize,
    read_limit: usize,
    source_metadata_bytes: usize,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<CompositionDecodeReservation, AuthoringProjectSourceAdmissionError> {
    let encoded = encoded_len.max(1);
    let maximum_retained = budget.limits.retained_candidate_bytes().get();
    let available = maximum_retained.saturating_sub(budget.retained_candidate_bytes);
    let bound = |nodes| {
        checked_sum([
            checked_product(encoded, 8)?,
            checked_product(nodes, 32)?,
            checked_product(limits.shape().container_items().get().min(nodes), 64)?,
            checked_product(
                limits.entities().get().min(nodes),
                checked_sum([
                    size_of::<SceneEntityRecord>(),
                    3 * size_of::<nara_scene::SceneEntityId>(),
                    128,
                ])?,
            )?,
            checked_product(
                limits.components().get().min(nodes),
                checked_sum([
                    size_of::<nara_scene::SceneComponentRecord>(),
                    size_of::<nara_reflect::ComponentTypeId>(),
                    128,
                ])?,
            )?,
            checked_product(
                limits.prefab_instances().get().min(nodes),
                size_of::<PrefabInstance>(),
            )?,
            checked_product(limits.overlay_declarations().get().min(nodes), 512)?,
            checked_product(limits.component_value_nodes().get().min(nodes), 96)?,
            checked_product(limits.diagnostic_sources().get().min(nodes), 128)?,
            source_metadata_bytes,
            4 * size_of::<usize>(),
        ])
    };
    let mut lower = 0;
    let mut upper = limits.shape().nodes().get().min(encoded);
    while lower < upper {
        let midpoint = lower + (upper - lower) / 2 + 1;
        if bound(midpoint)? <= available {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }
    let nodes = nara_core::ItemLimit::new(lower).ok_or(
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: maximum_retained,
        },
    )?;
    let shape = limits.shape();
    let limits = limits
        .with_encoded_bytes(
            nara_core::ByteLimit::new(read_limit)
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
        )
        .with_shape(nara_core::SerdeShapeLimits::new(
            shape.depth(),
            nodes,
            nara_core::ItemLimit::new(shape.container_items().get().min(nodes.get()))
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?,
            shape.string_bytes(),
            shape.total_string_bytes(),
        ));
    let bytes = bound(nodes.get())?;
    // Shape, header and payload parsing each scan the actual encoded input. Memory capacity
    // is not work performed; whitespace must not masquerade as millions of domain records.
    budget.charge_source_bytes(0, 0, checked_product(encoded_len, 3)?, bytes)?;
    let constrained_nodes = nodes.get() < shape.nodes().get().min(encoded);
    Ok(CompositionDecodeReservation {
        bytes,
        limits,
        constrained_nodes,
        constrained_container_items: constrained_nodes
            && nodes.get() < shape.container_items().get(),
        maximum_retained,
    })
}

pub(super) trait AuthoringDocumentMetrics {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError>;
}

impl AuthoringDocumentMetrics for SceneDocument {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        crate::scene_retention::scene_retained_bytes(self)
            .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)
    }
}

impl AuthoringDocumentMetrics for PrefabDocument {
    fn retained_bytes(&self) -> Result<usize, AuthoringProjectSourceAdmissionError> {
        crate::scene_retention::prefab_retained_bytes(self)
            .map_err(|_| AuthoringProjectSourceAdmissionError::AccountingOverflow)
    }
}

pub(super) fn checked_product(
    count: usize,
    bytes: usize,
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    count
        .checked_mul(bytes)
        .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
}

pub(super) fn checked_sum<const N: usize>(
    values: [usize; N],
) -> Result<usize, AuthoringProjectSourceAdmissionError> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(value)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
    })
}
