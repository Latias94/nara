//! Independent, finite process recovery through Nara's public CLI-only facade.

use std::{ffi::OsString, io::Write, path::PathBuf, process::ExitCode};

use nara_engine::{
    core::ItemLimit,
    fs::{ContentDigest, ReplaceReceipt},
    project_host::{
        AuthoringSourceOperationLimits, PendingAuthoringRecoveryInspection,
        PendingAuthoringRecoveryKind, PrefabExtractionRecovery,
        PrefabExtractionRecoveryMemberDisposition, PrefabExtractionRecoveryMemberState,
        PrefabExtractionRecoveryPhase, PrefabExtractionRecoveryReview,
        PrefabExtractionRecoveryUsage, PrefabExtractionRecoveryView,
    },
    project_tooling::host_fs,
    reflect::AuthoringSchemaBundle,
    scene::SceneCompilationLimits,
};
use serde_json::{Value, json};

const USAGE: &str = concat!(
    "usage: nara-prefab-extraction-recovery <inspect|resume> <root> ",
    "--host-work <nonzero> --open-handles <nonzero> --max-steps <nonzero> ",
    "[--all | --stop-after-files <count> | --stop-before-ready | --stop-after-ready]",
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    Inspect,
    Resume,
}

#[derive(Clone, Copy)]
enum Stop {
    All,
    Files(usize),
    BeforeReady,
    AfterReady,
}

struct Options {
    action: Action,
    root: PathBuf,
    limits: AuthoringSourceOperationLimits,
    max_steps: ItemLimit,
    stop: Option<Stop>,
}

fn text(value: Option<OsString>, label: &str) -> Result<String, String> {
    value
        .ok_or_else(|| format!("missing {label}; {USAGE}"))?
        .into_string()
        .map_err(|_| format!("{label} must be UTF-8"))
}

fn finite(value: String, label: &str) -> Result<ItemLimit, String> {
    let count = value
        .parse::<usize>()
        .map_err(|_| format!("{label} must be a nonzero finite integer"))?;
    ItemLimit::new(count).ok_or_else(|| format!("{label} must be nonzero"))
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args_os().skip(1);
    let action = match text(args.next(), "action")?.as_str() {
        "inspect" => Action::Inspect,
        "resume" => Action::Resume,
        _ => return Err(USAGE.into()),
    };
    let root = PathBuf::from(args.next().ok_or(USAGE)?);
    let mut work = None;
    let mut handles = None;
    let mut steps = None;
    let mut stop = None;
    while let Some(argument) = args.next() {
        let argument = text(Some(argument), "option")?;
        match argument.as_str() {
            "--host-work" if work.is_none() => {
                work = Some(finite(text(args.next(), "host work")?, "host work")?);
            }
            "--open-handles" if handles.is_none() => {
                handles = Some(finite(text(args.next(), "open handles")?, "open handles")?);
            }
            "--max-steps" if steps.is_none() => {
                steps = Some(finite(
                    text(args.next(), "maximum steps")?,
                    "maximum steps",
                )?);
            }
            "--all" if stop.is_none() => stop = Some(Stop::All),
            "--stop-before-ready" if stop.is_none() => stop = Some(Stop::BeforeReady),
            "--stop-after-ready" if stop.is_none() => stop = Some(Stop::AfterReady),
            "--stop-after-files" if stop.is_none() => {
                let count = text(args.next(), "file count")?
                    .parse::<usize>()
                    .map_err(|_| "file count must be a nonnegative finite integer")?;
                stop = Some(Stop::Files(count));
            }
            _ => return Err(format!("unknown or repeated option: {argument}; {USAGE}")),
        }
    }
    if action == Action::Inspect && stop.is_some() {
        return Err(format!(
            "stop selection is available only for resume; {USAGE}"
        ));
    }
    Ok(Options {
        action,
        root,
        limits: AuthoringSourceOperationLimits::default()
            .with_host_work(work.ok_or("--host-work is required")?)
            .with_open_handles(handles.ok_or("--open-handles is required")?),
        max_steps: steps.ok_or("--max-steps is required")?,
        stop: (action == Action::Resume).then_some(stop.unwrap_or(Stop::All)),
    })
}

fn phase_name(phase: PrefabExtractionRecoveryPhase) -> &'static str {
    use PrefabExtractionRecoveryPhase as P;
    match phase {
        P::Inspecting => "inspecting",
        P::Review => "review",
        P::PublishingSources => "publishing-sources",
        P::PreparingReady => "preparing-ready",
        P::PublishingReady => "publishing-ready",
        P::Complete => "complete",
        P::Closing => "closing",
        P::Closed => "closed",
    }
}

fn member_name(state: PrefabExtractionRecoveryMemberState) -> &'static str {
    use PrefabExtractionRecoveryMemberState as M;
    match state {
        M::Original => "original",
        M::Output => "output",
        M::Unchanged => "unchanged",
        M::NotCreated => "not-created",
        M::MissingOriginal => "missing-original",
        M::Conflict => "conflict",
    }
}

fn disposition_name(disposition: PrefabExtractionRecoveryMemberDisposition) -> &'static str {
    use PrefabExtractionRecoveryMemberDisposition as D;
    match disposition {
        D::Guard => "guard",
        D::SaveCurrent => "save-current",
        D::ReplaceSource => "replace-source",
        D::RewriteClosed => "rewrite-closed",
    }
}

fn usage_json(usage: PrefabExtractionRecoveryUsage) -> Value {
    json!({
        "host_work": usage.host_work,
        "peak_open_handles": usage.peak_open_handles,
        "source_work_nodes": usage.source_work_nodes,
        "source_work_bytes": usage.source_work_bytes,
    })
}

fn compilation_limits_json(limits: SceneCompilationLimits) -> Value {
    json!({
        "materialized_entities": limits.materialized_entities().get(),
        "materialized_components": limits.materialized_components().get(),
        "materialized_value_nodes": limits.materialized_value_nodes().get(),
        "materialized_value_bytes": limits.materialized_value_bytes().get(),
        "resolved_instances": limits.resolved_instances().get(),
        "overlay_declarations": limits.overlay_declarations().get(),
        "generated_identifier_bytes": limits.generated_identifier_bytes().get(),
        "single_generated_identifier_bytes": limits.single_generated_identifier_bytes().get(),
        "provenance_bytes": limits.provenance_bytes().get(),
        "working_bytes": limits.working_bytes().get(),
        "reference_work_nodes": limits.reference_work_nodes().get(),
        "reference_work_bytes": limits.reference_work_bytes().get(),
    })
}

fn record_view(report: &mut Value, view: PrefabExtractionRecoveryView) {
    report["phase"] = json!(phase_name(view.phase));
    report["source_count"] = json!(view.source_count);
    report["file_count"] = json!(view.file_count);
    report["completed_files"] = json!(view.completed_files);
    report["limits"] = json!({
        "host_work": view.limits.host_work().get(),
        "open_handles": view.limits.open_handles().get(),
    });
    report["compilation_limits"] = compilation_limits_json(view.compilation_limits);
    report["usage"] = usage_json(view.usage);
    report["uncertain"] = json!(view.uncertain);
    report["cleanup_pending"] = json!(view.cleanup_pending);
    report["refusal"] = json!(view.refusal.map(|kind| format!("{kind:?}")));
    report["ready_published"] = json!(view.phase == PrefabExtractionRecoveryPhase::Complete);
}

fn schema_json(bundle: &AuthoringSchemaBundle) -> Value {
    let owners: Vec<_> = bundle
        .owners()
        .iter()
        .map(|owner| {
            json!({
                "id": owner.owner_id().as_str(),
                "component_count": owner.current().components().len(),
                "has_predecessor": owner.predecessor().is_some(),
            })
        })
        .collect();
    json!({"fingerprint": format!("{:?}", bundle.fingerprint()), "owners": owners})
}

fn review_json(review: PrefabExtractionRecoveryReview<'_>) -> Value {
    let originals: Vec<_> = review
        .sources()
        .map(|source| {
            json!({
                "key": source.key,
                "path": source.project_path,
                "encoding": format!("{:?}", source.encoding),
                "explicit_native": source.explicit_native,
                "disposition": disposition_name(source.disposition),
                "observed": member_name(source.observed),
                "original_disk_bytes": source.original_disk.len(),
                "reviewed_working_bytes": source.reviewed_working.len(),
                "dirty": source.original_disk != source.reviewed_working,
                "output_bytes": source.output.map(<[u8]>::len),
            })
        })
        .collect();
    let q = review.destination();
    json!({
        "selected_profile": review.selected_profile(),
        "selected_source": review.selected_source(),
        "selected_root": review.selected_root(),
        "schema": schema_json(review.schema_bundle()),
        "unchanged_bundle_observed": member_name(review.unchanged_bundle_state()),
        "originals": originals,
        "q": {
            "key": q.key,
            "path": q.project_path,
            "encoding": format!("{:?}", q.encoding),
            "output_bytes": q.output.len(),
            "observed": member_name(q.observed),
            "metadata": {
                "path": q.metadata_path,
                "output_bytes": q.metadata_output.len(),
                "observed": member_name(q.metadata_observed),
            },
        },
    })
}

fn digest_json(digest: ContentDigest) -> Value {
    let hash: String = digest
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    json!({"length": digest.length(), "hash": hash})
}

fn receipt_json(receipt: &ReplaceReceipt) -> Value {
    let durability = receipt.durability();
    json!({
        "previous_identity_present": receipt.previous_identity().is_some(),
        "published_identity_present": receipt.published_identity().is_some(),
        "published_is_candidate": receipt.published_identity() == Some(receipt.candidate_identity()),
        "identity_evidence": format!("{:?}", receipt.publication_identity_evidence()),
        "parent_authorization": format!("{:?}", receipt.parent_authorization()),
        "atomicity": format!("{:?}", receipt.publication_atomicity()),
        "conflict_protection": format!("{:?}", receipt.conflict_protection()),
        "source_binding": format!("{:?}", receipt.source_binding()),
        "written_content_digest": digest_json(receipt.written_content_digest()),
        "published_content_digest": receipt.published_content_digest().map(digest_json),
        "verified_content_digest": receipt.verified_content_digest().map(digest_json),
        "durability": {
            "data_synced": format!("{:?}", durability.data_synced()),
            "file_metadata_synced": format!("{:?}", durability.file_metadata_synced()),
            "name_published": format!("{:?}", durability.name_published()),
            "parent_directory_synced": format!("{:?}", durability.parent_directory_synced()),
        },
    })
}

fn failure_json(recovery: &PrefabExtractionRecovery) -> Value {
    match recovery.failure() {
        Some(failure) => json!({
            "kind": format!("{:?}", failure.kind()),
            "message": failure.to_string(),
            "diagnostic_codes": failure.diagnostics().map(|report| {
                report.iter().take(16).map(|diagnostic| diagnostic.code().as_str()).collect::<Vec<_>>()
            }),
        }),
        None => Value::Null,
    }
}

fn reached(stop: Stop, view: PrefabExtractionRecoveryView) -> bool {
    match stop {
        Stop::Files(count) => view.completed_files >= count,
        Stop::BeforeReady => view.phase == PrefabExtractionRecoveryPhase::PublishingReady,
        Stop::AfterReady | Stop::All => view.phase == PrefabExtractionRecoveryPhase::Complete,
    }
}

/// Closing never advances publication. A failed close leaves the caller's owner intact.
fn close(
    recovery: &mut PrefabExtractionRecovery,
    max_steps: ItemLimit,
    report: &mut Value,
) -> bool {
    let mut attempts = 0;
    let mut error = None;
    for _ in 0..max_steps.get() {
        attempts += 1;
        error = recovery.close().err();
        let view = recovery.view();
        if !view.cleanup_pending {
            break;
        }
    }
    let view = recovery.view();
    let resolved = error.is_none()
        && !view.cleanup_pending
        && !view.uncertain
        && matches!(
            view.phase,
            PrefabExtractionRecoveryPhase::Closed | PrefabExtractionRecoveryPhase::Complete
        );
    report["close"] = json!({
        "resolved": resolved,
        "attempts": attempts,
        "phase": phase_name(view.phase),
        "cleanup_pending": view.cleanup_pending,
        "uncertain": view.uncertain,
        "usage": usage_json(view.usage),
        "error": error.map(|kind| format!("{kind:?}")),
        "failure": failure_json(recovery),
    });
    resolved
}

fn emit(report: &Value, code: u8) -> ExitCode {
    let mut output = std::io::stdout().lock();
    let result = serde_json::to_writer(&mut output, report)
        .map_err(std::io::Error::other)
        .and_then(|()| output.write_all(b"\n"))
        .and_then(|()| output.flush());
    if let Err(error) = result {
        let _ = writeln!(std::io::stderr().lock(), "recovery output failed: {error}");
        return ExitCode::from(2);
    }
    ExitCode::from(code)
}

fn unresolved(report: &mut Value) -> ! {
    report["status"] = json!("unresolved");
    let _ = emit(report, 2);
    // The caller still owns every unresolved native publisher. Exit the whole process
    // without turning a destructor into a cleanup or publication-completion claim.
    std::process::exit(2);
}

fn run(options: Options) -> ExitCode {
    let mut report = json!({
        "action": if options.action == Action::Inspect { "inspect" } else { "resume" },
        "status": "rejected",
        "operation": null,
        "limits": {
            "host_work": options.limits.host_work().get(),
            "open_handles": options.limits.open_handles().get(),
        },
        "max_steps": options.max_steps.get(),
        "steps": 0,
        "review": null,
        "outcome_consumed": false,
    });
    let root = match host_fs::writable_directory(&options.root) {
        Ok(root) => root,
        Err(error) => {
            report["failure"] = json!({"stage": "root-open", "message": error.to_string()});
            return emit(&report, 2);
        }
    };
    let inspection = match PendingAuthoringRecoveryInspection::inspect(root, options.limits) {
        Ok(inspection) => inspection,
        Err(error) => {
            report["refusal"] = json!(format!("{:?}", error.kind()));
            report["failure"] = json!({
                "stage": "pending-inspection",
                "kind": format!("{:?}", error.kind()),
                "message": error.to_string(),
                "host_work_limit_exceeded": error.host_work_limit_exceeded(),
                "open_handles_limit_exceeded": error.open_handles_limit_exceeded(),
            });
            return emit(&report, 2);
        }
    };
    report["operation"] = json!({
        "path": inspection.reference().recovery_path().as_str(),
        "digest": inspection.reference().recovery_digest(),
    });
    report["usage"] = json!({
        "host_work": inspection.usage().host_work,
        "peak_open_handles": inspection.usage().peak_open_handles,
    });
    match inspection.kind() {
        PendingAuthoringRecoveryKind::PrefabExtraction => {}
        PendingAuthoringRecoveryKind::AuthoredSchemaUpgrade => {
            report["refusal"] = json!("WrongKind");
            report["failure"] = json!({
                "stage": "dispatch", "kind": "WrongKind", "observed_kind": "authored-schema-upgrade",
                "message": "the authenticated operation requires the authored-upgrade recovery consumer",
            });
            return emit(&report, 2);
        }
    }
    let mut recovery =
        match PrefabExtractionRecovery::inspect(inspection, SceneCompilationLimits::default()) {
            Ok(recovery) => recovery,
            Err(error) => {
                // Initial inspection owns read-only transport, never a native publisher.
                report["refusal"] = json!(format!("{:?}", error.kind()));
                report["failure"] = json!({
                    "stage": "extraction-inspection", "kind": format!("{:?}", error.kind()),
                    "message": error.to_string(),
                });
                return emit(&report, 2);
            }
        };

    let mut steps = 0;
    let mut reviewed_basis = None;
    let mut returned_refusal = None;
    let result: Result<&str, String> = loop {
        let view = recovery.view();
        match view.phase {
            PrefabExtractionRecoveryPhase::Inspecting => {}
            PrefabExtractionRecoveryPhase::Review => {
                let Some(review) = recovery.review() else {
                    break Err("Review did not expose its borrowed fixed inputs".into());
                };
                reviewed_basis = Some(review.schema_basis().clone());
                report["review"] = review_json(review);
                if options.action == Action::Inspect {
                    break Ok("inspected");
                }
                if matches!(options.stop, Some(Stop::Files(count)) if count > view.file_count) {
                    break Err(
                        "stop file count exceeds the complete reviewed file membership".into(),
                    );
                }
            }
            PrefabExtractionRecoveryPhase::PublishingSources
            | PrefabExtractionRecoveryPhase::PreparingReady
            | PrefabExtractionRecoveryPhase::PublishingReady
            | PrefabExtractionRecoveryPhase::Complete => {
                if let Some(stop) = options.stop {
                    if reached(stop, view) {
                        break Ok(if matches!(stop, Stop::All) {
                            "complete"
                        } else {
                            "stopped"
                        });
                    }
                }
                if view.phase == PrefabExtractionRecoveryPhase::Complete {
                    break Err("recovery passed the requested stop boundary".into());
                }
            }
            PrefabExtractionRecoveryPhase::Closing | PrefabExtractionRecoveryPhase::Closed => {
                break Err("recovery closed before reaching the requested boundary".into());
            }
        }
        if steps == options.max_steps.get() {
            break Err("finite step limit reached before the requested boundary".into());
        }
        steps += 1;
        let next = if view.phase == PrefabExtractionRecoveryPhase::Review {
            // Only the explicit resume action can reach this confirmation.
            recovery.confirm()
        } else {
            recovery.advance()
        };
        if let Err(kind) = next {
            returned_refusal = Some(kind);
            break Err(format!("recovery step refused: {kind:?}"));
        }
    };
    report["steps"] = json!(steps);
    record_view(&mut report, recovery.view());
    if let Some(kind) = returned_refusal {
        report["refusal"] = json!(format!("{kind:?}"));
    }
    report["failure"] = failure_json(&recovery);
    if let Err(message) = &result {
        report["driver_error"] = json!(message);
    }
    // Even failures and deliberately interrupted processes explicitly close the retained owner.
    if !close(&mut recovery, options.max_steps, &mut report) {
        unresolved(&mut report);
    }
    let status = match result {
        Ok(status) => status,
        Err(_) => return emit(&report, 2),
    };
    if status != "complete" {
        report["status"] = json!(status);
        return emit(&report, 0);
    }
    let outcome = match recovery.into_outcome() {
        Ok(outcome) => outcome,
        Err(mut retained) => {
            report["driver_error"] = json!("Complete did not return its actual Ready outcome");
            report["failure"] = failure_json(&retained);
            if !close(&mut retained, options.max_steps, &mut report) {
                unresolved(&mut report);
            }
            return emit(&report, 2);
        }
    };
    let ready = outcome.ready_receipt();
    let basis_preserved = reviewed_basis
        .as_ref()
        .is_some_and(|basis| outcome.schema_basis() == basis);
    report["outcome_consumed"] = json!(true);
    report["outcome"] = json!({
        "operation": outcome.operation(),
        "ready_receipt": receipt_json(ready),
        "ready_parent_directory_sync": format!("{:?}", outcome.ready_parent_directory_sync()),
        "schema": schema_json(outcome.schema_bundle()),
        "schema_basis_preserved": basis_preserved,
        "source_keys": outcome.source_keys().collect::<Vec<_>>(),
    });
    if !basis_preserved
        || ready.verified_content_digest().is_none()
        || ready.previous_identity().is_none()
        || ready.published_identity() != Some(ready.candidate_identity())
    {
        report["driver_error"] = json!(
            "the actual outcome lacks reviewed Schema or confirmed Ready replacement evidence"
        );
        return emit(&report, 2);
    }
    report["status"] = json!("complete");
    emit(&report, 0)
}

fn main() -> ExitCode {
    match options() {
        Ok(options) => run(options),
        Err(message) => emit(
            &json!({
                "status": "rejected", "failure": {"stage": "arguments", "message": message},
            }),
            2,
        ),
    }
}
