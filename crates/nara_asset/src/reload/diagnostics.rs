use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, PublicDiagnosticIdentifier,
    SafeSummary,
};

use crate::{AssetError, AssetPath, AssetSourceKind};

use super::AssetSourceChangeKind;
use super::requests::AssetReloadRequest;
use super::resolution::{AssetReloadResolutionError, UNCLAIMED_RELOAD_REQUEST_CODE};

pub(super) fn source_change_resolution_failure_diagnostic(
    error: &AssetReloadResolutionError,
) -> Diagnostic {
    let context = error.diagnostic_context();
    let diagnostic = reload_failure_diagnostic(
        context.code,
        context.summary,
        context.path,
        context.change_kind,
        context.source_kind,
        context.reason,
    );
    let diagnostic = match context.admission {
        Some(source) => {
            let diagnostic = with_diagnostic_field(
                diagnostic,
                DiagnosticField::public_u64(
                    diagnostic_field_key("limit"),
                    usize_diagnostic_value(source.limit()),
                ),
            );
            match source.attempted() {
                Some(attempted) => with_diagnostic_field(
                    diagnostic,
                    DiagnosticField::public_u64(
                        diagnostic_field_key("attempted"),
                        usize_diagnostic_value(attempted),
                    ),
                ),
                None => diagnostic,
            }
        }
        None => diagnostic,
    };
    with_diagnostic_field(
        diagnostic,
        DiagnosticField::sensitive(diagnostic_field_key("error-detail")),
    )
}

pub(super) fn unclaimed_reload_request_diagnostic(request: &AssetReloadRequest) -> Diagnostic {
    reload_failure_diagnostic(
        UNCLAIMED_RELOAD_REQUEST_CODE,
        "Asset reload request was not claimed by its registered consumer",
        request.path(),
        request.source_change_kind(),
        Some(request.source_kind()),
        "consumer-did-not-claim",
    )
}

fn reload_failure_diagnostic(
    code: &'static str,
    summary: &'static str,
    path: &AssetPath,
    change_kind: AssetSourceChangeKind,
    source_kind: Option<&AssetSourceKind>,
    reason: &'static str,
) -> Diagnostic {
    let diagnostic = Diagnostic::error(
        DiagnosticCode::new(code).expect("asset diagnostic code literals must be valid"),
        SafeSummary::new(summary).expect("asset diagnostic summaries must be safe literals"),
    );
    let path_key = diagnostic_field_key("asset-path");
    let path_field = DiagnosticField::project_relative(path_key, path.as_str())
        .unwrap_or_else(|_| DiagnosticField::sensitive(path_key));
    let diagnostic = with_diagnostic_field(diagnostic, path_field);
    let diagnostic = with_diagnostic_field(
        diagnostic,
        DiagnosticField::public_identifier(
            diagnostic_field_key("change-kind"),
            PublicDiagnosticIdentifier::new(source_change_kind_identifier(change_kind))
                .expect("asset source change kind identifiers must be valid"),
        ),
    );
    let diagnostic = match source_kind {
        Some(source_kind) => with_diagnostic_field(
            diagnostic,
            DiagnosticField::public_identifier(
                diagnostic_field_key("source-kind"),
                PublicDiagnosticIdentifier::new(source_kind_identifier(source_kind))
                    .expect("asset source kind identifiers must be valid"),
            ),
        ),
        None => diagnostic,
    };
    with_diagnostic_field(
        diagnostic,
        DiagnosticField::public_identifier(
            diagnostic_field_key("reason"),
            PublicDiagnosticIdentifier::new(reason)
                .expect("asset reload reason identifiers must be valid"),
        ),
    )
}

fn source_change_kind_identifier(kind: AssetSourceChangeKind) -> &'static str {
    match kind {
        AssetSourceChangeKind::MetaModified => "meta-modified",
        AssetSourceChangeKind::Modified => "modified",
        AssetSourceChangeKind::Removed => "removed",
    }
}

fn source_kind_identifier(kind: &AssetSourceKind) -> &'static str {
    match kind {
        AssetSourceKind::Unknown => "unknown",
        AssetSourceKind::Image => "image",
        AssetSourceKind::Scene => "scene",
        AssetSourceKind::Prefab => "prefab",
        AssetSourceKind::Other(_) => "other",
    }
}

fn usize_diagnostic_value(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(super) fn asset_error_identifier(error: &AssetError) -> &'static str {
    match error {
        AssetError::IdSpaceExhausted => "id-space-exhausted",
        AssetError::InvalidPath(_) => "invalid-path",
        AssetError::ConflictingAssetIdentity { .. } => "conflicting-asset-identity",
        AssetError::PathAlreadyBound { .. } => "path-already-bound",
        AssetError::StableIdAlreadyBound { .. } => "stable-id-already-bound",
        AssetError::AssetIdAlreadyBoundToPath { .. } => "asset-id-already-bound-to-path",
        AssetError::AssetIdAlreadyBoundToStableId { .. } => "asset-id-already-bound-to-stable-id",
    }
}

fn diagnostic_field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("asset diagnostic field key literals must be valid")
}

fn with_diagnostic_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("asset diagnostics must use unique fields within the hard field limit")
}
