use nara_core::ItemLimit;
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    DiagnosticReportSettings, PublicDiagnosticIdentifier, SafeSummary,
};
use nara_ecs::Entity;
use nara_identity::{SceneEntityId, SourceDocumentKind, SourceDocumentOrigin};
use nara_scene::{SceneEntityProvenance, SceneEntitySource, SceneOccurrenceKey, SourceDocumentKey};

use super::{COMPONENT_ID, CollisionInput, CollisionOrigin};

pub(super) fn new_report() -> DiagnosticReport {
    let settings = DiagnosticReportSettings::default()
        .with_field_limit(ItemLimit::new(32).expect("collision diagnostic field limit is nonzero"))
        .expect("collision diagnostics use the admitted field limit");
    DiagnosticReport::new(settings)
}

pub(super) fn single(diagnostic: Diagnostic) -> Box<DiagnosticReport> {
    let mut report = new_report();
    report.push(diagnostic);
    Box::new(report)
}

pub(super) fn closed() -> Box<DiagnosticReport> {
    single(error(
        "fixture.motion.collision.closed",
        "Collision ownership has been closed",
    ))
}

pub(super) fn error(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::new(code).expect("collision diagnostic code literals are valid"),
        SafeSummary::new(summary).expect("collision diagnostic summary literals are safe"),
    )
}

pub(super) fn with_input(
    diagnostic: Diagnostic,
    input: &CollisionInput<'_>,
    index: usize,
) -> Diagnostic {
    let diagnostic = locator(diagnostic, "component", COMPONENT_ID);
    let diagnostic = unsigned(diagnostic, "input-index", index as u64);
    let diagnostic = unsigned(diagnostic, "domain", input.instance.domain_id().get());
    let diagnostic = unsigned(diagnostic, "instance", input.instance.instance_id().get());
    unsigned(diagnostic, "revision", input.revision.generation())
}

pub(super) fn with_entity(
    diagnostic: Diagnostic,
    entity_id: &SceneEntityId,
    entity: Option<Entity>,
    source: Option<&SceneEntitySource>,
) -> Diagnostic {
    let mut diagnostic = locator(diagnostic, "scene-entity", entity_id.as_str());
    if let Some(entity) = entity {
        diagnostic = unsigned(diagnostic, "runtime-entity", entity.to_bits());
    }
    if let Some(source) = source {
        diagnostic = authored_context(
            diagnostic,
            source.containing_source(),
            source.occurrence(),
            source.provenance(),
        );
    }
    diagnostic
}

pub(super) fn with_property(diagnostic: Diagnostic, field: &'static str) -> Diagnostic {
    locator(diagnostic, "field", field)
}

pub(super) fn with_origin(
    diagnostic: Diagnostic,
    entity: Entity,
    origin: &CollisionOrigin,
) -> Diagnostic {
    let diagnostic = locator(diagnostic, "component", origin.component.as_str());
    let diagnostic = unsigned(diagnostic, "runtime-entity", entity.to_bits());
    let diagnostic = unsigned(diagnostic, "revision", origin.revision.generation());
    authored_context(
        diagnostic,
        &origin.source,
        &origin.occurrence,
        &origin.provenance,
    )
}

fn authored_context(
    diagnostic: Diagnostic,
    source: &SourceDocumentKey,
    occurrence: &SceneOccurrenceKey,
    provenance: &SceneEntityProvenance,
) -> Diagnostic {
    let kind = match source.kind() {
        SourceDocumentKind::Scene => "scene",
        SourceDocumentKind::Prefab => "prefab",
    };
    let diagnostic = locator(diagnostic, "source-kind", kind);
    let diagnostic = match source.origin() {
        SourceDocumentOrigin::Named { universe, name } => {
            let diagnostic = locator(diagnostic, "source-origin", "named");
            let diagnostic = locator(diagnostic, "source-universe", &universe.to_string());
            locator(diagnostic, "source", name)
        }
        SourceDocumentOrigin::ProjectLocation { universe, path } => {
            let diagnostic = locator(diagnostic, "source-origin", "project-location");
            let diagnostic = locator(diagnostic, "source-universe", &universe.to_string());
            locator(diagnostic, "source", path.as_str())
        }
        SourceDocumentOrigin::StableAsset { stable_id } => {
            let diagnostic = locator(diagnostic, "source-origin", "stable-asset");
            locator(diagnostic, "source", &stable_id.to_string())
        }
    };
    let diagnostic = locator(diagnostic, "occurrence-namespace", occurrence.namespace());
    let diagnostic = locator(diagnostic, "occurrence-slot", occurrence.slot());
    let diagnostic = locator(diagnostic, "entity", provenance.local_entity_id().as_str());
    let diagnostic = unsigned(
        diagnostic,
        "projection-depth",
        provenance
            .projection_path()
            .map_or(0, |path| path.steps().len() as u64),
    );
    match provenance.projection_path() {
        Some(path) => locator(diagnostic, "projection-anchor", path.root_anchor().as_str()),
        None => diagnostic,
    }
}

fn locator(diagnostic: Diagnostic, key: &'static str, value: &str) -> Diagnostic {
    let key = DiagnosticFieldKey::new(key).expect("collision diagnostic field literals are valid");
    let field = match PublicDiagnosticIdentifier::new(value) {
        Ok(value) => DiagnosticField::public_identifier(key, value),
        Err(_) => DiagnosticField::sensitive(key),
    };
    diagnostic
        .try_with_field(field)
        .expect("collision diagnostic fields are unique and within the hard limit")
}

fn unsigned(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    let key = DiagnosticFieldKey::new(key).expect("collision diagnostic field literals are valid");
    diagnostic
        .try_with_field(DiagnosticField::public_u64(key, value))
        .expect("collision diagnostic fields are unique and within the hard limit")
}
