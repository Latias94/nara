//! Source-independent ActionMap CPU content and its concrete processed Runtime format.

use std::mem::size_of;

use nara_asset::{
    AssetDomain, AssetProductRef, AssetReferenceDemand, AssetRuntimeError, AssetRuntimeLimits,
    ImportedProductId, RuntimeAssetCodec,
};
use nara_core::{ByteLimit, PersistentFileContractError, SerdeShapeError};
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};

use crate::{
    ActionIdError, ActionMapDefinition, ActionMapError, ActionMapFileError, ActionMapFileLimits,
    ActionMapLimitKind,
    format::{
        action_map_nodes, action_map_string_bytes, decode_action_map, encode_action_map,
        encoded_action_map_bytes, require_primary,
    },
};

/// One complete primary Product and its immutable compiled Input definition.
///
/// This move-only CPU value contains no source provenance, World state or installation authority.
/// A Runtime content owner retains the actual typed acquisition when projecting its definition
/// into owning bindings; a borrowed definition cannot detach its shared storage from that owner.
#[derive(Debug)]
pub struct ActionMapAsset {
    product: AssetProductRef,
    definition: ActionMapDefinition,
}

impl ActionMapAsset {
    /// Admits primary identity and moves an already compiled definition. The concrete codec
    /// separately enforces its frozen wire and allocation limits when costing or encoding.
    pub fn new(
        product: AssetProductRef,
        definition: ActionMapDefinition,
    ) -> Result<Self, ActionMapFileError> {
        require_primary(product)?;
        Ok(Self {
            product,
            definition,
        })
    }

    #[must_use]
    pub const fn product(&self) -> AssetProductRef {
        self.product
    }

    #[must_use]
    pub const fn definition(&self) -> &ActionMapDefinition {
        &self.definition
    }

    #[must_use]
    pub fn into_parts(self) -> (AssetProductRef, ActionMapDefinition) {
        (self.product, self.definition)
    }
}

impl AsRef<ActionMapDefinition> for ActionMapAsset {
    fn as_ref(&self) -> &ActionMapDefinition {
        &self.definition
    }
}

/// The primary ActionMap CPU product family, independent of source importing or World state.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionMapDomain;

impl AssetDomain for ActionMapDomain {
    type Product = ActionMapAsset;

    fn contains_product(product: &ActionMapAsset, id: ImportedProductId) -> bool {
        product.product().product() == id
    }
}

/// Version one of the strict `runtime_action_map` JSON format.
///
/// The source and processed formats use one finite declaration compiler and strict object
/// reader, with separate envelope kinds. Required Product references are explicitly empty.
/// This codec freezes `ActionMapFileLimits::default()`: 256 KiB encoded, depth 16, 16,384 nodes,
/// 1,024 items per container, 128 bytes per string and 128 KiB total decoded string bytes;
/// definitions admit at most 64 contexts, 1,024 bindings and 32 bindings per physical input.
///
/// Work is the source limits' conservative `predecode_work_bytes()` reservation, covering input,
/// parsing, structural keys, compiler scratch and writer self-validation. Retained output is the
/// inline asset plus the definition's complete modeled compiled cost, with the definition's
/// inline wrapper subtracted once because it is already inside the asset. The maximum substitutes
/// the default definition retention cap for its actual cost. Compiled tables, strings and shared
/// ownership are charged once in full, even if another bindings handle shares them. Host/base
/// wrappers, encoded buffers and independent mutable World state have separate reservations.
/// These are logical costs rather than allocator interception or RSS measurements.
///
/// Cost queries borrow existing storage and allocate no query or compiler scratch. Code-first
/// values must fit the same definition and canonical wire limits before encoding. Encoding counts
/// borrowed output before allocating it and self-validates the actual processed bytes through the
/// same reader. Neither operation calls the source reader or changes an input envelope's header.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionMapRuntimeCodec;

impl RuntimeAssetCodec<ActionMapAsset> for ActionMapRuntimeCodec {
    const RUNTIME_KIND: &'static str = "runtime_action_map";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        let limits = ActionMapFileLimits::default();
        AssetRuntimeLimits::new(
            limits.encoded_bytes(),
            byte_limit(
                limits
                    .predecode_work_bytes()
                    .expect("fixed ActionMap work limit"),
            ),
            byte_limit(
                retained_bytes(limits.definition().retained_bytes().get())
                    .expect("fixed ActionMap retention limit"),
            ),
        )
    }

    fn encode_runtime(product: &ActionMapAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        Self::runtime_retained_bytes(product)?;
        encode_action_map(
            product.product(),
            product.definition(),
            ActionMapFileLimits::default(),
            Self::RUNTIME_KIND,
        )
        .map(Vec::into_boxed_slice)
        .map_err(file_error)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<ActionMapAsset, AssetRuntimeError> {
        let (product, definition) =
            decode_action_map(bytes, ActionMapFileLimits::default(), Self::RUNTIME_KIND)
                .map_err(file_error)?;
        let asset = ActionMapAsset::new(product, definition).map_err(file_error)?;
        Self::runtime_retained_bytes(&asset)?;
        Ok(asset)
    }

    fn runtime_retained_bytes(product: &ActionMapAsset) -> Result<usize, AssetRuntimeError> {
        let limits = ActionMapFileLimits::default();
        let definition = product.definition();
        let domain = limits.definition();
        let cost = definition.cost();
        require_primary(product.product()).map_err(file_error)?;
        for (kind, required, limit) in [
            (
                "contexts",
                definition.contexts().len(),
                domain.contexts().get(),
            ),
            (
                "bindings",
                definition.bindings().len(),
                domain.bindings().get(),
            ),
            ("work-bytes", cost.work_bytes(), domain.work_bytes().get()),
            (
                "retained-bytes",
                cost.retained_bytes(),
                domain.retained_bytes().get(),
            ),
        ] {
            if required > limit {
                return Err(budget_error(kind, Some(required), limit));
            }
        }
        for context in definition.contexts() {
            let required = context.context.as_str().len();
            if required > domain.identifier_bytes().get() {
                return Err(budget_error(
                    "identifier-bytes",
                    Some(required),
                    domain.identifier_bytes().get(),
                ));
            }
        }
        for binding in definition.bindings() {
            for required in [
                binding.action.as_str().len(),
                binding.context.as_str().len(),
            ] {
                if required > domain.identifier_bytes().get() {
                    return Err(budget_error(
                        "identifier-bytes",
                        Some(required),
                        domain.identifier_bytes().get(),
                    ));
                }
            }
            let required = definition.indices_for_input(binding.input).count();
            if required > domain.bindings_per_input().get() {
                return Err(budget_error(
                    "bindings-per-input",
                    Some(required),
                    domain.bindings_per_input().get(),
                ));
            }
        }
        let nodes = action_map_nodes(definition);
        if nodes > limits.shape().nodes().get() {
            return Err(budget_error(
                "shape-nodes",
                Some(nodes),
                limits.shape().nodes().get(),
            ));
        }
        let strings = action_map_string_bytes(definition, Self::RUNTIME_KIND);
        if strings > limits.shape().total_string_bytes().get() {
            return Err(budget_error(
                "shape-total-string-bytes",
                Some(strings),
                limits.shape().total_string_bytes().get(),
            ));
        }
        encoded_action_map_bytes(product.product(), definition, limits, Self::RUNTIME_KIND)
            .map_err(file_error)?;
        retained_bytes(cost.retained_bytes()).ok_or_else(|| budget_overflow("retained-bytes"))
    }

    fn visit_runtime_dependencies<E>(
        _product: &ActionMapAsset,
        _visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(product: &ActionMapAsset, reference: AssetProductRef) -> bool {
        product.product() == reference
    }
}

fn retained_bytes(compiled: usize) -> Option<usize> {
    compiled
        .checked_sub(size_of::<ActionMapDefinition>())?
        .checked_add(size_of::<ActionMapAsset>())
}

fn byte_limit(bytes: usize) -> ByteLimit {
    ByteLimit::new(bytes).expect("fixed nonzero Runtime ActionMap byte limit")
}

fn file_error(error: ActionMapFileError) -> AssetRuntimeError {
    match error {
        ActionMapFileError::Contract(PersistentFileContractError::WrongKind { .. }) => failure(
            "input.runtime-kind",
            "Runtime ActionMap requires the runtime_action_map file kind",
        ),
        ActionMapFileError::Contract(PersistentFileContractError::UnsupportedFormatVersion {
            ..
        }) => failure(
            "input.runtime-version",
            "Runtime ActionMap requires format version one",
        ),
        ActionMapFileError::Contract(PersistentFileContractError::EngineVersionTooOld {
            ..
        }) => failure(
            "input.runtime-engine-version",
            "Runtime ActionMap requires a newer engine version",
        ),
        ActionMapFileError::PrimaryProductRequired { .. } => failure(
            "input.runtime-product",
            "Runtime ActionMap requires a complete primary Product",
        ),
        ActionMapFileError::ContextLimitExceeded { maximum } => {
            budget_error("contexts", maximum.checked_add(1), maximum)
        }
        ActionMapFileError::BindingLimitExceeded { maximum } => {
            budget_error("bindings", maximum.checked_add(1), maximum)
        }
        ActionMapFileError::IdentifierBytesExceeded { observed, maximum } => {
            budget_error("identifier-bytes", Some(observed), maximum)
        }
        ActionMapFileError::Identifier(ActionIdError::TooLong { bytes, limit }) => {
            budget_error("identifier-bytes", Some(bytes), limit)
        }
        ActionMapFileError::Definition(ActionMapError::LimitExceeded {
            kind,
            required,
            limit,
        }) => {
            let kind = match kind {
                ActionMapLimitKind::Bindings => "bindings",
                ActionMapLimitKind::Contexts => "contexts",
                ActionMapLimitKind::IdentifierBytes => "identifier-bytes",
                ActionMapLimitKind::BindingsPerInput => "bindings-per-input",
                ActionMapLimitKind::WorkBytes => "work-bytes",
                ActionMapLimitKind::RetainedBytes => "retained-bytes",
                ActionMapLimitKind::RuntimeStateBytes => "runtime-state-bytes",
            };
            budget_error(kind, Some(required), limit)
        }
        ActionMapFileError::Definition(ActionMapError::CostOverflow) => {
            budget_overflow("definition-cost")
        }
        ActionMapFileError::Definition(_) => failure(
            "input.runtime-definition",
            "Runtime ActionMap requires a finite valid compiled definition",
        ),
        ActionMapFileError::EncodedBytesExceeded { observed, maximum } => {
            budget_error("encoded-bytes", Some(observed), maximum)
        }
        ActionMapFileError::Shape(error) => match error {
            SerdeShapeError::DepthExceeded { maximum } => {
                budget_error("shape-depth", maximum.checked_add(1), maximum)
            }
            SerdeShapeError::NodeLimitExceeded { maximum } => {
                budget_error("shape-nodes", maximum.checked_add(1), maximum)
            }
            SerdeShapeError::ContainerItemLimitExceeded { maximum } => {
                budget_error("shape-container-items", maximum.checked_add(1), maximum)
            }
            // Shape errors retain the ceiling, not the observed string length or running total.
            SerdeShapeError::StringLimitExceeded { maximum } => {
                budget_error("shape-string-bytes", None, maximum)
            }
            SerdeShapeError::TotalStringLimitExceeded { maximum } => {
                budget_error("shape-total-string-bytes", None, maximum)
            }
            SerdeShapeError::DuplicateMapKey => metadata_error(),
        },
        _ => metadata_error(),
    }
}

fn metadata_error() -> AssetRuntimeError {
    failure(
        "input.runtime-metadata",
        "Runtime ActionMap requires bounded strict JSON records",
    )
}

fn budget_error(kind: &'static str, required: Option<usize>, limit: usize) -> AssetRuntimeError {
    let mut diagnostic = diagnostic(
        "input.runtime-budget",
        "Runtime ActionMap exceeds its finite codec budget",
    )
    .try_with_field(budget_kind(kind))
    .expect("fixed unique Runtime ActionMap budget field");
    if let Some(required) = required {
        diagnostic = diagnostic
            .try_with_field(DiagnosticField::public_u64(
                field_key("required"),
                u64::try_from(required).unwrap_or(u64::MAX),
            ))
            .expect("fixed unique Runtime ActionMap required field");
    }
    AssetRuntimeError::new(
        diagnostic
            .try_with_field(DiagnosticField::public_u64(
                field_key("limit"),
                u64::try_from(limit).unwrap_or(u64::MAX),
            ))
            .expect("fixed unique Runtime ActionMap limit field"),
    )
}

fn budget_overflow(kind: &'static str) -> AssetRuntimeError {
    AssetRuntimeError::new(
        diagnostic(
            "input.runtime-budget-overflow",
            "Runtime ActionMap budget calculation overflowed",
        )
        .try_with_field(budget_kind(kind))
        .expect("fixed Runtime ActionMap budget field"),
    )
}

fn budget_kind(kind: &'static str) -> DiagnosticField {
    DiagnosticField::public_identifier(
        field_key("budget-kind"),
        PublicDiagnosticIdentifier::new(kind).expect("fixed Runtime ActionMap budget identifier"),
    )
}

fn field_key(key: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(key).expect("fixed Runtime ActionMap diagnostic field key")
}

fn diagnostic(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("fixed valid Runtime ActionMap diagnostic code"),
        SafeSummary::new(summary).expect("fixed safe Runtime ActionMap summary"),
    )
}

fn failure(code: &'static str, summary: &'static str) -> AssetRuntimeError {
    AssetRuntimeError::new(diagnostic(code, summary))
}
