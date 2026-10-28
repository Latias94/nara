//! Deterministic owner/worker boundaries; internal Ready values do not grant managed approval.

use std::{
    sync::{Condvar, Mutex},
    time::Instant,
};

use nara_tasks::TaskReturnObserver;

use super::*;

const WAIT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IoPhase {
    BeforeWork,
    AfterWork,
}

#[derive(Default)]
struct GateState {
    entered: bool,
    released: bool,
}

pub(super) struct IoGate {
    phase: IoPhase,
    shared: Arc<(Mutex<GateState>, Condvar)>,
}

pub(super) struct IoControl {
    shared: Arc<(Mutex<GateState>, Condvar)>,
}

impl IoGate {
    pub(super) fn new(phase: IoPhase) -> (Self, IoControl) {
        let shared = Arc::new((Mutex::new(GateState::default()), Condvar::new()));
        (
            Self {
                phase,
                shared: Arc::clone(&shared),
            },
            IoControl { shared },
        )
    }

    pub(super) fn wait_at(&self, phase: IoPhase) {
        if self.phase != phase {
            return;
        }
        let (state, changed) = &*self.shared;
        let mut state = state.lock().unwrap_or_else(|error| error.into_inner());
        state.entered = true;
        changed.notify_all();
        // Even a regressed blocking owner poll must eventually return to a bounded assertion.
        let (mut state, _) = changed
            .wait_timeout_while(state, WAIT * 2, |state| !state.released)
            .unwrap_or_else(|error| error.into_inner());
        state.released = true;
    }
}

impl IoControl {
    fn wait_until_entered(&self) {
        let (state, changed) = &*self.shared;
        let state = state.lock().unwrap_or_else(|error| error.into_inner());
        let (state, _) = changed
            .wait_timeout_while(state, WAIT, |state| !state.entered)
            .unwrap_or_else(|error| error.into_inner());
        assert!(
            state.entered,
            "selection IO did not reach {WAIT:?} gate deadline"
        );
        assert!(
            !state.released,
            "selection IO gate timed out before observation"
        );
    }

    fn release(&self) {
        let (state, changed) = &*self.shared;
        state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .released = true;
        changed.notify_all();
    }

    fn assert_pending(&self, returned: &TaskReturnObserver) {
        let state = self
            .shared
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(state.entered && !state.released);
        assert_eq!(returned.state(), TaskReturnState::Pending);
    }

    fn poll<T>(&self, poll: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let value = poll();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "selection owner poll blocked on held IO"
        );
        value
    }
}

impl Drop for IoControl {
    fn drop(&mut self) {
        self.release();
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod native {
    use std::fs;

    use nara_fs::ContentDigest;

    use super::super::super::tests::fixture::{Project, wait};
    use super::*;

    #[derive(Clone, Copy)]
    enum Lane {
        Operation,
        Disable,
    }

    fn observation(
        view: &EditorSourceToolSelectionView,
        lane: Lane,
    ) -> &Option<EditorSourceToolSelectionAttempt> {
        match lane {
            Lane::Operation => &view.operation,
            Lane::Disable => &view.disable,
        }
    }

    fn assert_attempt(
        editor: &EditorProjectSession,
        lane: Lane,
        attempt: &EditorSourceToolAttemptId,
        outcome: Option<Outcome>,
    ) {
        let view = editor.source_tool_selection_view();
        let current = observation(&view, lane).as_ref().expect("admitted attempt");
        assert_eq!(&current.attempt, attempt, "{view:?}");
        assert_eq!(current.outcome, outcome, "{view:?}");
    }

    fn wait_for_outcome(
        editor: &mut EditorProjectSession,
        lane: Lane,
        attempt: &EditorSourceToolAttemptId,
        outcome: Outcome,
    ) {
        wait(editor, |editor| {
            observation(&editor.source_tool_selection_view(), lane)
                .as_ref()
                .is_some_and(|current| &current.attempt == attempt && current.outcome.is_some())
        });
        assert_attempt(editor, lane, attempt, Some(outcome));
    }

    fn desired(enabled: bool) -> EditorSourceToolDesiredSelection {
        // Observe/Disable accepts saved intent as data, without installation or execution approval.
        EditorSourceToolDesiredSelection {
            installed: EditorSourceToolInstalledSelection {
                package: "test.finite-source-payload".into(),
                directory: "tools/finite-source-payload".into(),
                installation: [1; 32],
                receipt: ContentDigest::from_parts(32, [2; 32]),
                manifest: ContentDigest::from_parts(64, [3; 32]),
            },
            enabled,
        }
    }

    fn seed(project: &Project) -> Vec<u8> {
        let path = project.root.join(SELECTION_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let graph = Graph::default().select(desired(true)).unwrap();
        let bytes = store::encode(&graph).unwrap();
        fs::write(path, &bytes).unwrap();
        bytes
    }

    fn loaded(editor: &mut EditorProjectSession) -> EditorSourceToolSelectionView {
        wait(editor, |editor| {
            editor.source_tool_selection_view().phase == Phase::Idle
        });
        let view = editor.source_tool_selection_view();
        assert_eq!(
            view.operation.as_ref().unwrap().outcome,
            Some(Outcome::Observed)
        );
        assert!(editor.source_tool_view().active.is_none());
        view
    }

    fn hold(editor: &mut EditorProjectSession, phase: IoPhase) -> IoControl {
        let (gate, control) = IoGate::new(phase);
        assert!(
            editor
                .source_tools
                .selection
                .next_io_gate
                .replace(gate)
                .is_none()
        );
        control
    }

    fn returned(editor: &EditorProjectSession) -> TaskReturnObserver {
        editor
            .source_tools
            .selection
            .pending
            .as_ref()
            .unwrap()
            .task
            .as_ref()
            .expect("selection IO was admitted")
            .return_observer()
    }

    #[test]
    fn public_disable_reports_completion_only_after_worker_physical_return() {
        let project = Project::new();
        seed(&project);
        project.run(|editor| {
            let loaded = loaded(editor);
            let held = hold(editor, IoPhase::AfterWork);
            let attempt = editor.disable_installed_source_tool(&loaded.basis).unwrap();
            let returned = returned(editor);
            held.wait_until_entered();
            // The actual write has finished, but the worker and its work mutex remain held.
            let disk =
                store::decode(&fs::read(project.root.join(SELECTION_PATH)).unwrap()).unwrap();
            assert_eq!(disk.desired, Some(desired(false)));
            for _ in 0..3 {
                held.poll(|| editor.drive_editor_frame(Duration::ZERO));
                held.assert_pending(&returned);
                assert_attempt(editor, Lane::Disable, &attempt, None);
                let view = editor.source_tool_selection_view();
                assert_eq!(view.operation, loaded.operation);
                assert_eq!(view.phase, Phase::Publishing);
                assert_eq!(view.desired, Some(desired(true)));
                assert!(view.active_origin.is_none());
                assert_eq!(
                    held.poll(|| editor.disable_installed_source_tool(&view.basis)),
                    Err(Error::Busy)
                );
                assert_eq!(editor.source_tool_selection_view(), view);
            }
            held.release();
            wait_for_outcome(editor, Lane::Disable, &attempt, Outcome::Disabled);
            assert_eq!(returned.state(), TaskReturnState::Returned);
            let view = editor.source_tool_selection_view();
            assert_eq!(view.operation, loaded.operation);
            assert_eq!(view.desired, Some(desired(false)));
            assert_eq!(view.phase, Phase::Idle);
        });
    }

    #[test]
    fn cancelling_a_session_only_ready_activation_is_stale() {
        let project = Project::new();
        project.run(|editor| {
            loaded(editor);
            let definition = project.tool();
            let attempt = EditorSourceToolAttemptId::issue();
            editor.source_tools.attempt = Some(attempt.clone());
            editor.source_tool_ready(ReadyTool {
                active: EditorSourceToolActive::admit(
                    EditorSourceToolActivationId::issue(),
                    definition.owner.clone(),
                    definition.component.clone(),
                    None,
                )
                .unwrap(),
                definition,
                attempt: attempt.clone(),
                selection: None,
            });
            let before = editor.source_tool_view();
            assert!(before.active.is_some());
            assert!(!before.probing);
            assert_eq!(
                editor.cancel_source_tool_attempt(&attempt),
                Err(EditorSourceToolError::StaleAttempt)
            );
            assert_eq!(editor.source_tool_view(), before);
        });
    }

    #[test]
    fn reconciling_fences_new_execution_while_disable_remains_available() {
        let project = Project::new();
        seed(&project);
        project.run(|editor| {
            loaded(editor);
            editor.enable_source_tool(project.tool()).unwrap();
            wait(editor, |editor| !editor.source_tool_view().probing);
            let active = editor.source_tool_view().active.unwrap().id().clone();
            let original = editor.source_tool_selection_view();
            let held = hold(editor, IoPhase::AfterWork);
            let reconcile = editor
                .reconcile_source_tool_selection(&original.basis)
                .unwrap();
            let returned = returned(editor);
            held.wait_until_entered();
            assert_eq!(
                editor.source_tool_selection_view().phase,
                Phase::Reconciling
            );
            assert_eq!(
                held.poll(|| editor.enable_source_tool(project.tool())),
                Err(EditorSourceToolError::Busy)
            );
            let document = editor.workspace().active_document().unwrap();
            let basis = editor.document_query_basis(document).unwrap();
            assert_eq!(
                held.poll(|| editor.request_source_tool(
                    &active,
                    &basis,
                    EditorSourceToolIntent::Attach {
                        entity: nara_scene::SceneEntityId::new("target").unwrap()
                    },
                    BTreeMap::new(),
                )),
                Err(EditorSourceToolError::Busy)
            );
            assert_eq!(editor.source_tool_view().active.unwrap().id(), &active);
            held.poll(|| editor.disable_source_tool(&active)).unwrap();
            assert!(editor.source_tool_view().active.is_none());
            held.assert_pending(&returned);
            held.release();
            wait_for_outcome(editor, Lane::Operation, &reconcile, Outcome::Reconciled);
            assert!(editor.source_tool_view().active.is_none());
        });
    }

    #[test]
    fn same_leaf_and_digest_under_replaced_parent_do_not_admit_old_host() {
        let project = Project::new();
        let bytes = seed(&project);
        project.run(|editor| {
            loaded(editor);
            let root = editor.source_project_root();
            let host = editor.source_tools.selection.host.as_ref().unwrap().clone();
            let (identity, parent, digest) = host
                .try_with(|host| {
                    assert!(admits_publication(&root, host));
                    host.accepted_source_observation().unwrap()
                })
                .unwrap();
            let fixed_parent = project.root.join(".nara/source-tools");
            let displaced_parent = project.root.join(".nara/displaced-source-tools");
            let held_leaf = project.root.join(".nara/held-selection.json");
            // Move the exact leaf out before replacing its now-empty parent, then move it back.
            fs::rename(fixed_parent.join("selection.json"), &held_leaf).unwrap();
            fs::rename(&fixed_parent, &displaced_parent).unwrap();
            fs::create_dir(&fixed_parent).unwrap();
            fs::rename(&held_leaf, fixed_parent.join("selection.json")).unwrap();
            let current = root
                .open_file_exact(&RelativePath::new(SELECTION_PATH).unwrap())
                .unwrap();
            assert_eq!(current.identity(), identity);
            current.verify_digest(digest, store::BYTE_LIMIT).unwrap();
            assert_eq!(fs::read(project.root.join(SELECTION_PATH)).unwrap(), bytes);
            let current_parent = root
                .open_directory_exact(&RelativePath::new(".nara/source-tools").unwrap())
                .unwrap();
            assert_ne!(Some(current_parent.identity()), parent);
            assert!(
                !host
                    .try_with(|host| admits_publication(&root, host))
                    .unwrap()
            );
            let displaced = root
                .open_directory_exact(&RelativePath::new(".nara/displaced-source-tools").unwrap())
                .unwrap();
            assert_eq!(Some(displaced.identity()), parent);
        });
    }

    // Installation publication and exact temporary discard use the qualified Windows tier.
    #[cfg(windows)]
    mod windows {
        use std::thread;

        use nara_app::{drive_runtime_quarantine, runtime_quarantine_status};
        use nara_fs::{CapabilityRights, HostCapabilityOptions, RelativeComponent, TrustMode};
        use serde_json::json;

        use super::*;
        use crate::project_host::EditorProjectCloseProgress;
        use crate::project_host::file_publication::CleanupTestControl;

        fn strict_directory(path: &Path) -> DirectoryCapability {
            use std::os::windows::fs::OpenOptionsExt;

            let directory = fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(path)
                .unwrap();
            DirectoryCapability::from_host_handle(
                directory,
                HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
            )
            .unwrap()
        }

        fn installed_selection(
            project: &Project,
        ) -> (EditorSourceToolDesiredSelection, DirectoryCapability) {
            let source = project.root.join("release-input");
            fs::create_dir(&source).unwrap();
            let definition = project.tool();
            let files = [
                (
                    "catalog.json",
                    project
                        .owner
                        .current()
                        .to_json_string()
                        .unwrap()
                        .into_bytes(),
                ),
                (
                    "Tool.dll",
                    b"inert fixture; never approved or executed".to_vec(),
                ),
            ];
            let mut members = Vec::new();
            for (path, bytes) in files {
                fs::write(source.join(path), &bytes).unwrap();
                let digest = ContentDigest::of_bytes(&bytes);
                let hash: String = digest
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect();
                members.push(json!({
                    "path": path, "kind": "tool", "length": digest.length(), "blake3": hash,
                }));
            }
            let manifest = serde_json::to_vec(&json!({
                "format_version": 1,
                "package": "test.finite-source-payload",
                "release": "1",
                "host_target": "x86_64-pc-windows-msvc",
                "authority": "trusted_process",
                "protocol_version": 1,
                "owner": project.owner.owner_id().as_str(),
                "component": definition.component.as_str(),
                "catalog": "catalog.json", "entry": "Tool.dll", "requires": [], "files": members,
            }))
            .unwrap();
            fs::write(source.join("nara-tool.json"), &manifest).unwrap();
            let manifest_digest = ContentDigest::of_bytes(&manifest);
            let release = PreparedEditorSourceToolRelease::capture(
                &strict_directory(&source),
                manifest_digest,
            )
            .unwrap();
            fs::create_dir(project.root.join("tools")).unwrap();
            let project_root = strict_directory(&project.root);
            let tools = project_root
                .open_directory_exact(&RelativePath::new("tools").unwrap())
                .unwrap();
            let plan = EditorSourceToolInstallationPlan::new(
                tools,
                RelativeComponent::new("finite-source-payload").unwrap(),
                release,
            )
            .unwrap();
            let installation = match plan.publish() {
                EditorSourceToolInstallationOutcome::Published(installation) => installation,
                other => panic!("real code-free installation failed: {other:?}"),
            };
            installation.verify().unwrap();
            let desired = EditorSourceToolDesiredSelection {
                installed: EditorSourceToolInstalledSelection {
                    package: installation.package().into(),
                    directory: "tools/finite-source-payload".into(),
                    installation: *installation.receipt().installation_id(),
                    receipt: installation.receipt_digest(),
                    manifest: manifest_digest,
                },
                enabled: true,
            };
            graph::verify_entries(
                &project_root,
                &Graph::default().select(desired.clone()).unwrap().entries,
            )
            .unwrap();
            (desired, project_root)
        }

        #[test]
        fn registration_revalidates_payload_before_publishing_and_keeps_source_usable() {
            use nara_tooling::EditorSourceToolInstallationRequest;

            let project = Project::new();
            let (desired, root) = installed_selection(&project);
            project.run(|editor| {
                let initial = loaded(editor);
                let source = project.persisted();
                let request = EditorSourceToolInstallationRequest {
                    directory: Arc::clone(&desired.installed.directory),
                    receipt: desired.installed.receipt,
                    requirements: Vec::new(),
                };
                let prepared = editor
                    .prepare_source_tool_installations(
                        &initial.basis,
                        root.share(),
                        &[request.clone()],
                    )
                    .unwrap();
                let bytes =
                    fs::read(project.root.join("tools/finite-source-payload/Tool.dll")).unwrap();
                let held = hold(editor, IoPhase::BeforeWork);
                let attempt = editor.register_source_tool_installations(prepared).unwrap();
                held.wait_until_entered();
                fs::write(
                    project.root.join("tools/finite-source-payload/Tool.dll"),
                    b"changed after review",
                )
                .unwrap();
                held.release();
                wait_for_outcome(
                    editor,
                    Lane::Operation,
                    &attempt,
                    Outcome::Failed(Error::Installation),
                );
                assert!(editor.source_tool_selection_view().installed.is_empty());
                assert!(!project.root.join(SELECTION_PATH).exists());
                assert!(editor.source_tool_view().active.is_none());
                assert_eq!(project.persisted(), source);

                fs::write(
                    project.root.join("tools/finite-source-payload/Tool.dll"),
                    bytes,
                )
                .unwrap();
                let basis = editor.source_tool_selection_view().basis;
                let prepared = editor
                    .prepare_source_tool_installations(&basis, root.share(), &[request])
                    .unwrap();
                let attempt = editor.register_source_tool_installations(prepared).unwrap();
                wait_for_outcome(editor, Lane::Operation, &attempt, Outcome::Registered);
                assert_eq!(editor.source_tool_selection_view().installed.len(), 1);
                assert!(editor.source_tool_selection_view().desired.is_none());
                assert_eq!(project.persisted(), source);
            });
        }

        #[test]
        fn confirmed_withdrawal_keeps_files_and_failed_publication_never_restarts_the_route() {
            use nara_tooling::EditorSourceToolWithdrawalMode;

            let project = Project::new();
            let (desired, root) = installed_selection(&project);
            project.run(|editor| {
                loaded(editor);
                let attempt = ready(editor, &project, desired.clone(), root);
                wait_for_outcome(editor, Lane::Operation, &attempt, Outcome::Enabled);
                let source = project.persisted();
                let initial = editor.source_tool_selection_view();
                let prepared = editor
                    .prepare_source_tool_withdrawal(
                        &initial.basis,
                        &desired.installed.installation,
                        EditorSourceToolWithdrawalMode::RejectDependents,
                    )
                    .unwrap();
                assert_eq!(prepared.installations().len(), 1);
                assert!(editor.source_tool_view().active.is_some());
                drop(prepared);
                assert_eq!(editor.source_tool_selection_view(), initial);
                let prepared = editor
                    .prepare_source_tool_withdrawal(
                        &initial.basis,
                        &desired.installed.installation,
                        EditorSourceToolWithdrawalMode::Cascade,
                    )
                    .unwrap();
                let held = hold(editor, IoPhase::BeforeWork);
                let attempt = editor.withdraw_source_tool_installations(prepared).unwrap();
                held.wait_until_entered();
                assert!(editor.source_tool_view().active.is_none());
                fs::write(
                    project.root.join(SELECTION_PATH),
                    b"external selection edit",
                )
                .unwrap();
                held.release();
                wait_for_outcome(
                    editor,
                    Lane::Operation,
                    &attempt,
                    Outcome::Failed(Error::Changed),
                );
                assert!(editor.source_tool_view().active.is_none());
                assert_eq!(
                    editor.source_tool_selection_view().installed,
                    initial.installed
                );
                assert!(
                    project
                        .root
                        .join("tools/finite-source-payload/Tool.dll")
                        .exists()
                );
                assert_eq!(
                    fs::read(project.root.join(SELECTION_PATH)).unwrap(),
                    b"external selection edit"
                );
                assert_eq!(project.persisted(), source);
            });
        }

        #[test]
        fn graph_withdrawal_publication_does_not_claim_held_invocation_physical_return() {
            use super::super::super::super::tests::fixture::{GatePoint, HeldProcess};
            use nara_tooling::EditorSourceToolWithdrawalMode;

            for point in [GatePoint::BeforeSpawn, GatePoint::BeforeIoReturn] {
                let project = Project::new();
                let (desired, root) = installed_selection(&project);
                project.run(|editor| {
                    loaded(editor);
                    let attempt = ready(editor, &project, desired.clone(), root);
                    wait_for_outcome(editor, Lane::Operation, &attempt, Outcome::Enabled);
                    let source = project.persisted();
                    let activation = editor.source_tool_view().active.unwrap().id().clone();
                    let held = HeldProcess::arm(editor, point);
                    let document = editor.workspace().active_document().unwrap();
                    let basis = editor.document_query_basis(document).unwrap();
                    let request = editor
                        .request_source_tool(
                            &activation,
                            &basis,
                            EditorSourceToolIntent::Attach {
                                entity: nara_scene::SceneEntityId::new("target").unwrap(),
                            },
                            BTreeMap::new(),
                        )
                        .unwrap();
                    let returned = editor
                        .source_tools
                        .invocation
                        .as_ref()
                        .unwrap()
                        .task
                        .return_observer();
                    held.wait_until_entered();
                    let selection = editor.source_tool_selection_view();
                    let prepared = held
                        .poll(|| {
                            editor.prepare_source_tool_withdrawal(
                                &selection.basis,
                                &desired.installed.installation,
                                EditorSourceToolWithdrawalMode::Cascade,
                            )
                        })
                        .unwrap();
                    let attempt = held
                        .poll(|| editor.withdraw_source_tool_installations(prepared))
                        .unwrap();
                    wait_for_outcome(editor, Lane::Operation, &attempt, Outcome::Withdrawn);
                    held.assert_pending(&returned);
                    let view = editor.source_tool_view();
                    assert!(view.active.is_none() && view.review.is_none() && view.retiring);
                    assert!(view.selection.as_ref().unwrap().installed.is_empty());
                    assert_eq!(project.persisted(), source);
                    let progress = held.poll(|| editor.advance_close());
                    assert!(matches!(
                        progress,
                        EditorProjectCloseProgress::Incomplete { .. }
                    ));
                    assert!(!progress.task_shutdown().is_complete());
                    held.assert_pending(&returned);
                    held.release();
                    wait(editor, |editor| !editor.source_tool_view().retiring);
                    assert_eq!(returned.state(), TaskReturnState::Returned);
                    assert!(editor.accept_source_tool_review(&request).is_err());
                    assert_eq!(project.persisted(), source);
                    assert!(
                        project
                            .root
                            .join("tools/finite-source-payload/Tool.dll")
                            .exists()
                    );
                });
            }
        }

        fn ready(
            editor: &mut EditorProjectSession,
            project: &Project,
            desired: EditorSourceToolDesiredSelection,
            approved_project: DirectoryCapability,
        ) -> EditorSourceToolAttemptId {
            // Enter at the private post-probe boundary. No managed Host or approval is fabricated;
            // only code-free installation verification and subsequent owner IO are under test.
            let definition = project.tool();
            assert!(definition.command.approved.is_none());
            let attempt = EditorSourceToolAttemptId::issue();
            let fence = EditorSourceToolSelectionBasis::issue();
            let predecessor = editor
                .source_tools
                .active
                .as_ref()
                .map(|active| active.active.id().clone());
            editor.source_tools.attempt = Some(attempt.clone());
            let selection = &mut editor.source_tools.selection;
            selection.basis = fence.clone();
            selection.phase = Phase::Probing;
            selection.operation = Some(EditorSourceToolSelectionAttempt {
                attempt: attempt.clone(),
                outcome: None,
            });
            selection.error = None;
            editor.source_tool_ready(ReadyTool {
                active: EditorSourceToolActive::admit(
                    EditorSourceToolActivationId::issue(),
                    definition.owner.clone(),
                    definition.component.clone(),
                    None,
                )
                .unwrap(),
                definition,
                attempt: attempt.clone(),
                selection: Some(EnableSelection {
                    graph: Graph::default().select(desired.clone()).unwrap(),
                    closure: Graph::default().select(desired.clone()).unwrap().entries,
                    approved_project,
                    fence,
                    predecessor,
                }),
            });
            attempt
        }

        #[test]
        fn deferred_disable_waits_for_enabled_publication_and_keeps_both_outcomes() {
            for phase in [IoPhase::BeforeWork, IoPhase::AfterWork] {
                let project = Project::new();
                let (desired, root) = installed_selection(&project);
                project.run(|editor| {
                    loaded(editor);
                    let held = hold(editor, phase);
                    let attempt = ready(editor, &project, desired.clone(), root);
                    let returned = returned(editor);
                    held.wait_until_entered();
                    let publishing = editor.source_tool_selection_view();
                    assert_eq!(publishing.phase, Phase::Publishing);
                    assert_attempt(editor, Lane::Operation, &attempt, None);
                    assert!(editor.source_tools.selection.ready.is_some());
                    assert_eq!(
                        held.poll(|| editor.cancel_source_tool_attempt(&attempt)),
                        Err(EditorSourceToolError::StaleAttempt)
                    );
                    assert_eq!(editor.source_tool_selection_view(), publishing);
                    assert!(editor.source_tools.selection.ready.is_some());
                    if phase == IoPhase::BeforeWork {
                        assert!(!project.root.join(SELECTION_PATH).exists());
                    } else {
                        assert_eq!(
                            store::decode(&fs::read(project.root.join(SELECTION_PATH)).unwrap())
                                .unwrap()
                                .desired,
                            Some(desired.clone())
                        );
                    }
                    let disable = held
                        .poll(|| editor.disable_installed_source_tool(&publishing.basis))
                        .unwrap();
                    assert_attempt(editor, Lane::Operation, &attempt, None);
                    assert_attempt(editor, Lane::Disable, &disable, None);
                    assert!(editor.source_tools.selection.ready.is_none());
                    let deferred = editor.source_tool_selection_view();
                    assert_eq!(
                        held.poll(|| editor.disable_installed_source_tool(&deferred.basis)),
                        Err(Error::Busy)
                    );
                    assert_eq!(editor.source_tool_selection_view(), deferred);
                    held.assert_pending(&returned);
                    // A separate gate belongs to the next IO, not to the held enable write.
                    let disabling = hold(editor, IoPhase::BeforeWork);
                    held.release();
                    wait(editor, |editor| {
                        editor
                            .source_tools
                            .selection
                            .pending
                            .as_ref()
                            .is_some_and(|pending| pending.attempt == disable)
                    });
                    disabling.wait_until_entered();
                    assert_eq!(returned.state(), TaskReturnState::Returned);
                    assert_attempt(
                        editor,
                        Lane::Operation,
                        &attempt,
                        Some(Outcome::PublishedWithoutActivation),
                    );
                    assert_attempt(editor, Lane::Disable, &disable, None);
                    assert_eq!(
                        store::decode(&fs::read(project.root.join(SELECTION_PATH)).unwrap())
                            .unwrap()
                            .desired,
                        Some(desired.clone())
                    );
                    assert!(editor.source_tool_view().active.is_none());
                    disabling.release();
                    wait_for_outcome(editor, Lane::Disable, &disable, Outcome::Disabled);
                    assert_attempt(
                        editor,
                        Lane::Operation,
                        &attempt,
                        Some(Outcome::PublishedWithoutActivation),
                    );
                    let mut disabled = desired.clone();
                    disabled.enabled = false;
                    assert_eq!(
                        editor.source_tool_selection_view().desired,
                        Some(disabled.clone())
                    );
                    assert_eq!(
                        store::decode(&fs::read(project.root.join(SELECTION_PATH)).unwrap())
                            .unwrap()
                            .desired,
                        Some(disabled)
                    );
                    assert!(editor.source_tool_view().active.is_none());
                });
            }
        }

        #[test]
        fn close_retains_main_publication_and_deferred_disable_failure_without_late_promotion() {
            let project = Project::new();
            let (desired, root) = installed_selection(&project);
            project.run(|editor| {
                loaded(editor);
                let held = hold(editor, IoPhase::AfterWork);
                let attempt = ready(editor, &project, desired.clone(), root);
                let returned = returned(editor);
                held.wait_until_entered();
                let basis = editor.source_tool_selection_view().basis;
                let disable = editor.disable_installed_source_tool(&basis).unwrap();
                for _ in 0..3 {
                    let close = held.poll(|| editor.advance_close());
                    assert!(!matches!(close, EditorProjectCloseProgress::Complete(_)));
                    assert!(!close.task_shutdown().is_complete());
                    held.assert_pending(&returned);
                    assert_attempt(editor, Lane::Operation, &attempt, None);
                    assert_attempt(editor, Lane::Disable, &disable, None);
                    assert!(editor.source_tool_view().active.is_none());
                }
                held.release();
                let deadline = Instant::now() + WAIT;
                loop {
                    match editor.advance_close() {
                        EditorProjectCloseProgress::Complete(receipt) => {
                            assert!(receipt.task_shutdown().is_complete());
                            assert!(!receipt.has_cleanup_failure());
                            break;
                        }
                        progress => assert!(
                            Instant::now() < deadline,
                            "held publication Close did not settle: {progress:?}"
                        ),
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                assert_eq!(returned.state(), TaskReturnState::Returned);
                for _ in 0..3 {
                    editor.advance_close();
                    assert_attempt(
                        editor,
                        Lane::Operation,
                        &attempt,
                        Some(Outcome::PublishedWithoutActivation),
                    );
                    assert_attempt(
                        editor,
                        Lane::Disable,
                        &disable,
                        Some(Outcome::Failed(Error::Closing)),
                    );
                    assert_eq!(
                        editor.source_tool_selection_view().desired,
                        Some(desired.clone())
                    );
                    assert!(editor.source_tool_view().active.is_none());
                    assert!(editor.source_tool_selection_view().active_origin.is_none());
                }
                assert_eq!(
                    store::decode(&fs::read(project.root.join(SELECTION_PATH)).unwrap())
                        .unwrap()
                        .desired,
                    Some(desired)
                );
            });
        }

        struct CleanupGuard {
            control: CleanupTestControl,
            retained_before: usize,
        }

        impl CleanupGuard {
            fn new(control: CleanupTestControl) -> Self {
                Self {
                    control,
                    retained_before: runtime_quarantine_status().current_thread_retained(),
                }
            }

            fn drain(&self) -> bool {
                let deadline = Instant::now() + WAIT;
                while runtime_quarantine_status().current_thread_retained() > self.retained_before {
                    let _ = drive_runtime_quarantine();
                    if Instant::now() >= deadline {
                        return false;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                true
            }
        }

        impl Drop for CleanupGuard {
            fn drop(&mut self) {
                self.control.release();
                let _ = self.drain();
            }
        }

        fn entries(project: &Project) -> Vec<PathBuf> {
            let mut paths = fs::read_dir(project.root.join(".nara/source-tools"))
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>();
            paths.sort();
            paths
        }

        #[test]
        fn failed_write_cleanup_survives_close_and_abnormal_drop_until_explicit_release() {
            for close_before_drop in [false, true] {
                let project = Project::new();
                let original = seed(&project);
                // Unwinding drops the Session before the guard releases and drains quarantine.
                let cleanup;
                let mut editor = project.open();
                let loaded = loaded(&mut editor);
                let host = editor.source_tools.selection.host.as_ref().unwrap().clone();
                let before = entries(&project);
                cleanup =
                    CleanupGuard::new(host.try_with(|host| host.test_fail_after_write()).unwrap());
                let attempt = editor.disable_installed_source_tool(&loaded.basis).unwrap();
                wait_for_outcome(
                    &mut editor,
                    Lane::Disable,
                    &attempt,
                    Outcome::Failed(Error::Write),
                );
                assert_eq!(
                    editor.source_tool_selection_view().operation,
                    loaded.operation
                );
                assert!(host.try_with(|host| host.has_cleanup()).unwrap());
                let pending = entries(&project);
                assert_eq!(pending.len(), before.len() + 1);
                let temporary = pending.iter().find(|path| !before.contains(path)).unwrap();
                assert_eq!(
                    store::decode(&fs::read(temporary).unwrap())
                        .unwrap()
                        .desired,
                    Some(desired(false))
                );
                assert_eq!(
                    fs::read(project.root.join(SELECTION_PATH)).unwrap(),
                    original
                );
                if close_before_drop {
                    let deadline = Instant::now() + WAIT;
                    loop {
                        let close = editor.advance_close();
                        assert!(matches!(
                            close,
                            EditorProjectCloseProgress::Incomplete {
                                file_cleanup_pending: true,
                                ..
                            }
                        ));
                        if close.task_shutdown().is_complete() {
                            break;
                        }
                        assert!(
                            Instant::now() < deadline,
                            "failed-write tasks did not return: {close:?}"
                        );
                        thread::sleep(Duration::from_millis(1));
                    }
                    assert!(host.try_with(|host| host.has_cleanup()).unwrap());
                }
                drop(editor);
                assert_eq!(
                    runtime_quarantine_status().current_thread_retained(),
                    cleanup.retained_before + 1
                );
                for _ in 0..3 {
                    assert_eq!(
                        drive_runtime_quarantine().current_thread_retained(),
                        cleanup.retained_before + 1
                    );
                    assert!(host.try_with(|host| host.has_cleanup()).unwrap());
                    assert_eq!(entries(&project), pending);
                    assert_eq!(
                        fs::read(project.root.join(SELECTION_PATH)).unwrap(),
                        original
                    );
                }
                cleanup.control.release();
                assert!(
                    cleanup.drain(),
                    "selection cleanup did not retire after release"
                );
                assert_eq!(
                    runtime_quarantine_status().current_thread_retained(),
                    cleanup.retained_before
                );
                assert!(
                    host.try_with(|_| ()).is_none(),
                    "actual publication Host outlived cleanup retirement"
                );
                assert!(!temporary.exists());
                assert_eq!(entries(&project), before);
                assert_eq!(
                    fs::read(project.root.join(SELECTION_PATH)).unwrap(),
                    original
                );
            }
        }
    }
}
