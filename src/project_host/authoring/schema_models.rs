//! Bounded Schema model and Adapter admission without file or project-readiness authority.

use std::{mem::size_of, sync::Arc};

use nara_core::{ItemLimit, SerdeShapeLimits};
use nara_project::EffectiveProjectSettings;
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleLimits, ComponentFieldSchema, ComponentSchema,
    ComponentSchemaOwnerSnapshot, ComponentShapeMember, ComponentTypeId, ComponentValue,
    PersistentComponentSchemaOwnerId,
};
use nara_scene::{
    __private::AuthoringAdapterReceipt, AuthoringAdapterEvidence,
    AuthoringDocumentPublicationError, AuthoringSchemaBasis,
};

use super::{
    AdmittedAuthoringManifest, AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError,
    AuthoringProjectAdmissionLimits, AuthoringSchemaReceipt, PROJECT_MANIFEST_PATH,
    adapter::{FirstPartyAuthoringAdapterError, project_authoring_adapters},
    bundle::AdmittedAuthoringSchemaBundle,
};
use crate::scene_retention::{
    ARC_CONTROL_BLOCK_BYTES, BTREE_ENTRY_OVERHEAD, schema_catalog_retained_bytes,
    string_retained_bytes,
};

pub(super) struct SelectedSchemaModels {
    pub(super) bundle: Arc<AuthoringSchemaBundle>,
    pub(super) adapters: Box<[AuthoringAdapterReceipt]>,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) retained_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum SchemaModelsError {
    #[error("authoring Adapter preparation failed")]
    Adapters(#[source] FirstPartyAuthoringAdapterError),
    #[error("authoring basis admission failed")]
    Basis(#[source] AuthoringDocumentPublicationError),
    #[error("Schema model admission exceeded its shared budget")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("Schema model accounting overflowed")]
    AccountingOverflow,
    #[error(
        "actual selected Schema storage ({actual} bytes) exceeds its admitted preparation reservation ({reserved} bytes)"
    )]
    SchemaPreparationBoundExceeded { reserved: usize, actual: usize },
}

struct SchemaPreparationReservation {
    limits: AuthoringSchemaBundleLimits,
    bytes: usize,
}

/// Admits one actual bundle/Adapter basis under the enclosing operation's continuing allowance.
/// The caller owns input/native admission and the candidate-storage scope for refused preparation.
pub(super) fn prepare_schema_models(
    encoded: &[u8],
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SelectedSchemaModels, SchemaModelsError> {
    let reservation = reserve_schema(encoded.len(), budget)?;
    let result = (|| {
        let bundle =
            AuthoringSchemaBundle::from_json_bytes_with_limits(encoded, reservation.limits)
                .map_err(AuthoringProjectAdmissionError::BundleInvalid)?;
        let adapters = project_authoring_adapters(&bundle).map_err(SchemaModelsError::Adapters)?;
        // Measure real current/predecessor catalogs through their existing cost owner. The
        // basis's schema copies and Adapter indexes have their own conservative actual bound;
        // unused format capacity must not become permanently decoded or retained ownership.
        let actual = selected_schema_retained_bytes(&bundle, &adapters)?;
        if actual > reservation.bytes {
            return Err(SchemaModelsError::SchemaPreparationBoundExceeded {
                reserved: reservation.bytes,
                actual,
            });
        }
        let basis =
            AuthoringSchemaBasis::admit(&bundle, &adapters).map_err(SchemaModelsError::Basis)?;
        Ok((
            SelectedSchemaModels {
                bundle: Arc::new(bundle),
                adapters,
                basis,
                retained_bytes: actual,
            },
            actual,
        ))
    })();
    // All partial models are dropped by the closure on failure. Its parsing/Adapter work was
    // admitted cumulatively before it ran; only the now-unused storage reservation is refunded.
    let actual = result.as_ref().map_or(0, |(_, actual)| *actual);
    budget
        .reconcile_retained_reservation(reservation.bytes, actual)
        .map_err(|_| SchemaModelsError::AccountingOverflow)?;
    let (models, actual) = result?;
    // reserve_schema also bounded against the decoded residual. This records the actual selected
    // models once, rather than charging its maximum possible node capacity as decoded content.
    budget.charge_source_bytes(0, actual, 0, 0)?;
    Ok(models)
}

/// Prepares an ordinary capture baseline for already admitted actual models. Existing model
/// admission supplies the catalog/basis/Adapter bound; no parser, IO or nominal admission is repeated.
/// Operation work and its larger selected ceiling never become ordinary capture policy.
pub(super) fn ordinary_schema_capture_budget(
    limits: AuthoringProjectAdmissionLimits,
    manifest_bytes: usize,
    bundle_bytes: usize,
    bundle_path_bytes: usize,
    models_retained_bytes: usize,
) -> Result<AuthoringProjectAdmissionBudget, SchemaModelsError> {
    let mut capture = AuthoringProjectAdmissionBudget::new(limits);
    capture.charge_manifest_bytes(manifest_bytes)?;
    capture.charge_bundle_bytes(bundle_bytes)?;
    // Effective settings are a finite projection of this manifest. Bound their inline data,
    // strings and vector slots by the encoded input instead of keeping its parser capacity.
    let settings = add(
        size_of::<EffectiveProjectSettings>(),
        mul(manifest_bytes, 64)?,
    )?;
    let storage = add(
        add(models_retained_bytes, settings)?,
        add(
            add(
                size_of::<AuthoringSchemaReceipt>(),
                size_of::<AdmittedAuthoringManifest>(),
            )?,
            add(
                size_of::<AdmittedAuthoringSchemaBundle>(),
                add(bundle_path_bytes, mul(ARC_CONTROL_BLOCK_BYTES, 4)?)?,
            )?,
        )?,
    )?;
    capture.charge_source_bytes(0, storage, 0, storage)?;
    // The root is shared; these two actual file capabilities belong to the new Schema receipt.
    capture.retain_open_handles(2)?;
    capture.unique_files = 2;
    capture.path_bytes = add(PROJECT_MANIFEST_PATH.len(), bundle_path_bytes)?;
    capture.dependency_edges = 1;
    capture.max_traversal_depth = 1;
    // These bytes have already supplied model admission. Future capture starts new work
    // while inheriting their complete content and live-storage/native reservations.
    capture.decode_work = 0;
    capture.ensure_within_limits()?;
    Ok(capture)
}

pub(super) fn selected_schema_retained_bytes(
    bundle: &AuthoringSchemaBundle,
    adapters: &[AuthoringAdapterReceipt],
) -> Result<usize, SchemaModelsError> {
    let mut total = add(size_of::<AuthoringSchemaBundle>(), ARC_CONTROL_BLOCK_BYTES)?;
    for owner in bundle.owners() {
        let current = schema_catalog_retained_bytes(owner.current())
            .map_err(|_| SchemaModelsError::AccountingOverflow)?;
        let predecessor = owner
            .predecessor()
            .map_or(Ok(0), schema_catalog_retained_bytes)
            .map_err(|_| SchemaModelsError::AccountingOverflow)?;
        // The admitted bundle, basis catalogs and basis's component-schema map are bounded by
        // three actual catalog copies. This intentionally overcounts shared predecessor data,
        // while preserving every alias, tombstone, value shape/default and catalog capacity.
        total = add(total, mul(add(current, predecessor)?, 3)?)?;
        let owner_text = string_retained_bytes(owner.owner_id().as_str())
            .map_err(|_| SchemaModelsError::AccountingOverflow)?;
        total = add(
            total,
            add(
                mul(size_of::<ComponentSchemaOwnerSnapshot>(), 2)?,
                add(owner_text, mul(ARC_CONTROL_BLOCK_BYTES, 2)?)?,
            )?,
        )?;
        for component in owner.current().components() {
            let component_text = string_retained_bytes(component.id().as_str())
                .map_err(|_| SchemaModelsError::AccountingOverflow)?;
            let keys = add(
                size_of::<ComponentTypeId>(),
                size_of::<PersistentComponentSchemaOwnerId>(),
            )?;
            let entry = add(
                keys,
                add(
                    size_of::<Option<AuthoringAdapterEvidence>>(),
                    BTREE_ENTRY_OVERHEAD,
                )?,
            )?;
            // A component map retains its independent key, owner locator and possible data
            // Adapter owner locator. The schema's own strings are covered by the catalog cost.
            total = add(
                total,
                add(entry, add(component_text, mul(owner_text, 2)?)?)?,
            )?;
        }
    }
    total = add(total, size_of::<Box<[AuthoringAdapterReceipt]>>())?;
    for adapter in adapters {
        total = add(
            total,
            adapter
                .checked_retained_bytes()
                .ok_or(SchemaModelsError::AccountingOverflow)?,
        )?;
    }
    Ok(total)
}

fn reserve_schema(
    encoded: usize,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<SchemaPreparationReservation, SchemaModelsError> {
    // Keep the existing format owner's parser and lower its node ceiling to the actual residual
    // preparation budget. Wire admission, admitted catalogs, basis maps and Adapter indexes may
    // coexist. A larger string does not unconditionally imply that many complete catalog nodes.
    let limits = AuthoringSchemaBundleLimits::default();
    let shape = limits.shape();
    let slot = add(
        add(
            size_of::<ComponentSchema>(),
            size_of::<ComponentFieldSchema>(),
        )?,
        add(
            size_of::<ComponentShapeMember>(),
            add(size_of::<ComponentValue>(), 192)?,
        )?,
    )?;
    let available = budget
        .limits
        .retained_candidate_bytes()
        .get()
        .saturating_sub(budget.retained_candidate_bytes)
        .min(
            budget
                .limits
                .aggregate_decoded_bytes()
                .get()
                .saturating_sub(budget.decoded_bytes),
        );
    let fixed = add(
        mul(encoded, 8)?,
        mul(limits.owners().get(), size_of::<AuthoringAdapterReceipt>())?,
    )?;
    let nodes = available.saturating_sub(fixed) / mul(slot, 3)?;
    let nodes = nodes.min(shape.nodes().get()).min(encoded.max(1));
    let nodes = ItemLimit::new(nodes).ok_or(SchemaModelsError::Admission(
        AuthoringProjectAdmissionError::RetainedCandidateBytesExceeded {
            maximum: budget.limits.retained_candidate_bytes().get(),
        },
    ))?;
    let reserved = add(fixed, mul(nodes.get(), mul(slot, 3)?)?)?;
    // Work is cumulative, including refused attempts. Charge it separately before temporary
    // retention so a work refusal cannot strand a reservation that was never used.
    budget.charge_source_bytes(0, 0, mul(encoded, 6)?, 0)?;
    budget.charge_source_bytes(0, 0, 0, reserved)?;
    Ok(SchemaPreparationReservation {
        limits: limits.with_shape(SerdeShapeLimits::new(
            shape.depth(),
            nodes,
            ItemLimit::new(shape.container_items().get().min(nodes.get()))
                .expect("bundle container allowance is non-zero"),
            shape.string_bytes(),
            shape.total_string_bytes(),
        )),
        bytes: reserved,
    })
}

fn add(left: usize, right: usize) -> Result<usize, SchemaModelsError> {
    left.checked_add(right)
        .ok_or(SchemaModelsError::AccountingOverflow)
}

fn mul(left: usize, right: usize) -> Result<usize, SchemaModelsError> {
    left.checked_mul(right)
        .ok_or(SchemaModelsError::AccountingOverflow)
}
