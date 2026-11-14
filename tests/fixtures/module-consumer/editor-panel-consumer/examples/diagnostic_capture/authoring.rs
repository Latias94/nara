use std::{
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::Path,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

use nara::{
    app::RuntimeCandidateRetirementState,
    project_host::{
        EditorEditPreparationError, EditorProjectCloseProgress, EditorProjectOpenFailure,
        EditorProjectOpenHost, EditorProjectOpenProgress, EditorProjectSession,
        EditorSourceAssetDomain, EditorSourceAssetImporters,
    },
    reflect::{
        ComponentFieldId, ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue,
    },
    scene::AuthoringPropertyEdit,
};
use nara_asset::{
    AssetDomain, AssetImportError, AssetImportLimits, AssetImportSource, AssetMeta,
    AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetSourceKind, SourceAssetImporter,
    StableAssetId,
};
use nara_asset_domain_consumer::{TextDomain, TextResource, TextSettings};
use nara_editor_panel_module_consumer::snap_local_position::read_requests;
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentId, EditorDocumentMutation,
    EditorDocumentQueryRejection, EditorPropertyValue, EditorSourceAssetImportActionOutcome,
    EditorSourceAssetImportAttempt,
};

#[path = "../../tests/support/snap_project.rs"]
#[allow(dead_code)]
mod snap_project;

const TIMEOUT: Duration = Duration::from_secs(10);
const SOURCE_IMPORT_HOLD: Duration = Duration::from_millis(10);
static RUN_LOCK: Mutex<()> = Mutex::new(());
static PREPARATION_GATE: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> = Mutex::new(None);

struct DelayedTextDomain;
impl AssetDomain for DelayedTextDomain {
    type Product = TextResource;
}
impl SourceAssetImporter for DelayedTextDomain {
    type Settings = TextSettings;
    const SOURCE_KIND: &'static str = "consumer.delayed-text";
    fn limits() -> AssetImportLimits {
        TextDomain::limits()
    }
    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<TextResource, AssetImportError<E>> {
        let product = TextDomain::prepare(source)?;
        let gate = PREPARATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some((entered, release)) = gate {
            let _ = entered.send(());
            let _ = release.recv();
        }
        Ok(product)
    }
}
impl EditorSourceAssetDomain for DelayedTextDomain {
    type Domain = Self;
    fn default_settings() -> TextSettings {
        TextSettings { max_characters: 64 }
    }
}
struct Release(mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[derive(serde::Serialize)]
pub struct AuthoringReport {
    pub product: serde_json::Value,
    pub imported: bool,
    pub old_source_preserved: bool,
    pub metadata_identity_preserved: bool,
    pub metadata_changed: bool,
    pub metadata_preserved_while_held: bool,
    pub rejection_stale_revision: bool,
    pub document_unchanged_after_rejection: bool,
    pub history_before_rejection: u64,
    pub history_after_rejection: u64,
    pub redo_before_rejection: u64,
    pub redo_after_rejection: u64,
    pub successful_edit_value: f64,
    pub last_good_operation_usable: bool,
    pub worker_actual_return: bool,
    pub session_closed: bool,
    pub open_host_closed: bool,
    pub import_hold_elapsed_ns: u64,
    pub import_elapsed_ns: u64,
    pub edit_elapsed_ns: u64,
    pub elapsed_ns: u64,
}

pub fn run() -> Result<AuthoringReport, String> {
    let _serial = RUN_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let started = Instant::now();
    let project = snap_project::fixture([0.0, 32.0]);
    let mut host = EditorProjectOpenHost::new();
    let mut session = None;
    let mut rejected = None;
    let mut pending = None;
    let result = catch_unwind(AssertUnwindSafe(|| {
        fs::copy(
            project.root.join(snap_project::SOURCE),
            project.root.join("scenes/startup.scene.json"),
        )
        .map_err(|_| "authoring scene setup failed")?;
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::try_random().map_err(|_| "authoring identity setup failed")?,
                AssetPath::new("caption.txt").map_err(|_| "authoring asset setup failed")?,
                AssetSourceKind::Other(DelayedTextDomain::SOURCE_KIND.into()),
            ),
            TextSettings { max_characters: 64 },
        )
        .to_json_string()
        .map_err(|_| "authoring metadata setup failed")?;
        fs::write(project.root.join("assets/caption.txt.meta"), metadata)
            .map_err(|_| "authoring metadata setup failed")?;
        let mut importers = EditorSourceAssetImporters::new();
        importers
            .add::<DelayedTextDomain>()
            .map_err(|_| "authoring importer setup failed")?;
        host = EditorProjectOpenHost::new().with_source_asset_importers(importers);
        pending = Some(host.begin(project.capability(), None));
        let deadline = Instant::now() + TIMEOUT;
        while session.is_none() {
            let attempt = pending.take().ok_or("authoring open attempt unavailable")?;
            match host.advance(attempt) {
                EditorProjectOpenProgress::Opened(opened) => session = Some(opened),
                EditorProjectOpenProgress::Pending(next) => pending = Some(next),
                EditorProjectOpenProgress::Rejected(failure) => {
                    rejected = Some(failure);
                    return Err("authoring project open rejected".into());
                }
            }
            if Instant::now() >= deadline {
                return Err("authoring project open timed out".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        exercise(
            session.as_mut().ok_or("authoring session unavailable")?,
            &project.root,
        )
    }));
    drop(pending.take());
    PREPARATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    let cleanup = catch_unwind(AssertUnwindSafe(|| {
        close(&mut session, &mut host, &mut rejected)
    }));
    if !matches!(cleanup, Ok(true)) {
        std::mem::forget((session, host, rejected, project));
        return Err("authoring cleanup remains retained".into());
    }
    match result {
        Ok(Ok(mut report)) => {
            report.session_closed = true;
            report.open_host_closed = true;
            report.product = serde_json::json!({
                "imported": report.imported,
                "old_source_preserved": report.old_source_preserved,
                "metadata_identity_preserved": report.metadata_identity_preserved,
                "metadata_changed": report.metadata_changed,
                "metadata_preserved_while_held": report.metadata_preserved_while_held,
                "rejection_stale_revision": report.rejection_stale_revision,
                "document_unchanged_after_rejection": report.document_unchanged_after_rejection,
                "history_before_rejection": report.history_before_rejection,
                "history_after_rejection": report.history_after_rejection,
                "redo_before_rejection": report.redo_before_rejection,
                "redo_after_rejection": report.redo_after_rejection,
                "successful_edit_value": report.successful_edit_value,
                "last_good_operation_usable": report.last_good_operation_usable,
                "worker_actual_return": report.worker_actual_return,
                "session_closed": report.session_closed,
                "open_host_closed": report.open_host_closed,
            });
            report.elapsed_ns = elapsed_ns(started);
            Ok(report)
        }
        Ok(Err(error)) => Err(error),
        Err(payload) => resume_unwind(payload),
    }
}

fn exercise(session: &mut EditorProjectSession, root: &Path) -> Result<AuthoringReport, String> {
    settle(session, |s| {
        let frame = s.current_frame();
        frame
            .project_browser()
            .entry("assets/caption.txt")
            .is_some()
            && !frame.project_browser().has_pending_work()
            && !frame.image_import_work_pending()
    })?;
    let sources = source_bytes(root)?;
    let metadata_path = root.join("assets/caption.txt.meta");
    let metadata_before = fs::read(&metadata_path).map_err(|_| "authoring metadata read failed")?;
    let original = AssetMetaIdentityCandidate::decode_json_bytes(&metadata_before)
        .map_err(|_| "authoring original metadata invalid")?;
    let import_started = Instant::now();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = Release(release_tx);
    *PREPARATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some((entered_tx, release_rx));
    let action = DelayedTextDomain::import_action(
        "assets/caption.txt",
        &TextSettings {
            max_characters: 128,
        },
    )
    .map_err(|_| "authoring import request rejected")?;
    if !matches!(
        dispatch(session, EditorAction::SourceAssetImport(action))?,
        EditorActionOutcome::SourceAssetImport(EditorSourceAssetImportActionOutcome::Started)
    ) {
        return Err("authoring import did not start".into());
    }
    entered_rx
        .recv_timeout(TIMEOUT)
        .map_err(|_| "authoring import did not enter preparation")?;
    let held = Instant::now();
    std::thread::sleep(SOURCE_IMPORT_HOLD);
    session.drive_editor_frame(Duration::ZERO);
    let metadata_preserved_while_held = session.current_frame().source_asset_import_work_pending()
        && fs::read(&metadata_path).map_err(|_| "authoring held metadata read failed")?
            == metadata_before;
    let import_hold_elapsed_ns = elapsed_ns(held);
    drop(release);
    settle(session, |s| {
        let frame = s.current_frame();
        !frame.source_asset_import_work_pending()
            && !frame.image_import_work_pending()
            && !frame.project_browser().has_pending_work()
    })?;
    let frame = session.current_frame();
    let imported = matches!(
        frame.source_asset_import().map(|view| &view.attempt),
        Some(EditorSourceAssetImportAttempt::MetadataPublished { stable_id })
            if *stable_id == original.identity().stable_id()
    );
    let worker_actual_return = imported && !frame.source_asset_import_work_pending();
    let metadata_after =
        fs::read(&metadata_path).map_err(|_| "authoring completed metadata read failed")?;
    let completed = AssetMetaIdentityCandidate::decode_json_bytes(&metadata_after)
        .map_err(|_| "authoring completed metadata invalid")?;
    let metadata_identity_preserved = completed.identity() == original.identity();
    let metadata_changed = metadata_after != metadata_before;
    let settings_valid = completed
        .admit::<TextSettings>()
        .map_err(|_| "authoring completed settings invalid")?
        .settings()
        .max_characters
        == 128;
    if !imported
        || !metadata_identity_preserved
        || !metadata_changed
        || !metadata_preserved_while_held
        || !settings_valid
    {
        return Err("authoring import did not preserve its contract".into());
    }
    let import_elapsed_ns = elapsed_ns(import_started);
    let edit_started = Instant::now();
    let document = frame
        .workspace()
        .active_document
        .ok_or("authoring document unavailable")?;
    let captured = session
        .document_query_basis(document)
        .map_err(|_| "authoring basis unavailable")?;
    let edits = [AuthoringPropertyEdit {
        entity: snap_project::id("actor"),
        component: ComponentTypeId::new(snap_project::TRANSFORM),
        component_version: ComponentSchemaVersion::ONE,
        property: ComponentPropertyPath::new(ComponentFieldId::new("translation.x")),
        value: ComponentValue::f64(23.0).map_err(|_| "authoring edit value invalid")?,
    }];
    let prepared = session
        .prepare_authoring_property_edit(&captured, &edits)
        .map_err(|_| "authoring valid preparation failed")?;
    applied(session.commit_authoring_edit(prepared))?;
    session.drive_editor_frame(Duration::ZERO);
    let before = session.current_frame();
    let history_before = before
        .workspace()
        .document(document)
        .ok_or("authoring edited document unavailable")?
        .history;
    let value_before = value(session, document)?;
    let rejection_stale_revision = matches!(
        session.prepare_authoring_property_edit(&captured, &edits),
        Err(EditorEditPreparationError::QueryBasis(
            EditorDocumentQueryRejection::StaleRevision
        ))
    );
    session.drive_editor_frame(Duration::ZERO);
    let after = session.current_frame();
    let history_after = after
        .workspace()
        .document(document)
        .ok_or("authoring rejected document unavailable")?
        .history;
    let document_unchanged_after_rejection =
        before.workspace() == after.workspace() && value(session, document)? == value_before;
    if !rejection_stale_revision
        || !document_unchanged_after_rejection
        || value_before != 23.0
        || history_before.undo_depth != 1
        || history_before.redo_depth != 0
    {
        return Err("authoring stale edit did not preserve its contract".into());
    }
    for (redo, expected) in [(false, 0.0), (true, 23.0)] {
        let base_revision = session
            .document_query_basis(document)
            .map_err(|_| "authoring history basis unavailable")?
            .revision;
        let mutation = if redo {
            EditorDocumentMutation::Redo {
                document,
                base_revision,
            }
        } else {
            EditorDocumentMutation::Undo {
                document,
                base_revision,
            }
        };
        applied(dispatch(session, EditorAction::Mutate(mutation))?)?;
        if value(session, document)? != expected {
            return Err("authoring last successful edit is unusable".into());
        }
    }
    let successful_edit_value = value(session, document)?;
    let last_good_operation_usable = session
        .current_frame()
        .workspace()
        .document(document)
        .is_some_and(|tab| tab.history == history_before)
        && successful_edit_value == 23.0;
    let old_source_preserved = source_bytes(root)? == sources;
    if !old_source_preserved || !last_good_operation_usable {
        return Err("authoring completed state did not preserve its contract".into());
    }
    Ok(AuthoringReport {
        product: serde_json::Value::Null,
        imported,
        old_source_preserved,
        metadata_identity_preserved,
        metadata_changed,
        metadata_preserved_while_held,
        rejection_stale_revision,
        document_unchanged_after_rejection,
        history_before_rejection: history_before.undo_depth as u64,
        history_after_rejection: history_after.undo_depth as u64,
        redo_before_rejection: history_before.redo_depth as u64,
        redo_after_rejection: history_after.redo_depth as u64,
        successful_edit_value,
        last_good_operation_usable,
        worker_actual_return,
        session_closed: false,
        open_host_closed: false,
        import_hold_elapsed_ns,
        import_elapsed_ns,
        edit_elapsed_ns: elapsed_ns(edit_started),
        elapsed_ns: 0,
    })
}

fn settle(
    session: &mut EditorProjectSession,
    ready: impl Fn(&EditorProjectSession) -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + TIMEOUT;
    while !ready(session) {
        if Instant::now() >= deadline {
            return Err("authoring operation timed out".into());
        }
        session.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

fn dispatch(
    session: &mut EditorProjectSession,
    action: EditorAction,
) -> Result<EditorActionOutcome, String> {
    let revision = session.current_frame().revision();
    let report = session
        .dispatch_editor_actions(EditorActionBatch::new(revision, [action]))
        .map_err(|_| "authoring action batch rejected")?;
    session.drive_editor_frame(Duration::ZERO);
    report
        .into_outcomes()
        .into_iter()
        .next()
        .ok_or_else(|| "authoring action outcome missing".into())
}

fn applied(outcome: EditorActionOutcome) -> Result<(), String> {
    match outcome {
        EditorActionOutcome::Workspace(report) if report.applied => Ok(()),
        _ => Err("authoring document mutation rejected".into()),
    }
}

fn value(session: &EditorProjectSession, document: EditorDocumentId) -> Result<f64, String> {
    let basis = session
        .document_query_basis(document)
        .map_err(|_| "authoring read basis unavailable")?;
    let [request, _] = read_requests(&basis, &snap_project::id("actor"))
        .map_err(|_| "authoring read request rejected")?;
    match session
        .read_document_property(request)
        .map_err(|_| "authoring property read rejected")?
        .value
    {
        EditorPropertyValue::Present(ComponentValue::F64(value)) => Ok(value.get()),
        _ => Err("authoring property value unavailable".into()),
    }
}

fn source_bytes(root: &Path) -> Result<Vec<Vec<u8>>, String> {
    [
        "assets/caption.txt",
        "assets/suffix.txt",
        "scenes/startup.scene.json",
        snap_project::SOURCE,
    ]
    .into_iter()
    .map(|path| {
        fs::read(root.join(path)).map_err(|_| "authoring original source read failed".into())
    })
    .collect()
}

fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn close(
    session: &mut Option<EditorProjectSession>,
    host: &mut EditorProjectOpenHost,
    rejected: &mut Option<EditorProjectOpenFailure>,
) -> bool {
    let deadline = Instant::now() + TIMEOUT;
    let mut session_closed = session.is_none();
    let mut cleanup_failed = false;
    loop {
        if !session_closed
            && let Some(session) = session.as_mut()
            && let EditorProjectCloseProgress::Complete(receipt) = session.advance_close()
        {
            cleanup_failed |= receipt.has_cleanup_failure();
            session_closed = true;
        }
        let rejected_closed = rejected.as_mut().is_none_or(|failure| {
            let retired = failure.advance_retirement() == RuntimeCandidateRetirementState::Retired;
            cleanup_failed |= failure
                .close_evidence()
                .is_some_and(|evidence| evidence.has_terminal_failure());
            retired
        });
        let report = host.poll_shutdown();
        cleanup_failed |= report.has_cleanup_failure();
        if session_closed && rejected_closed && report.is_complete() {
            return !cleanup_failed;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
