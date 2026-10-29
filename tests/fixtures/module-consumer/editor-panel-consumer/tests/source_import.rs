use nara_editor::EditorNativeComposition;
use nara_editor_panel_module_consumer::text_import;

#[path = "support/text_project.rs"]
mod text_project;

use nara::asset::AssetMetaIdentityCandidate;
use nara_asset::{AssetImportError, AssetImportLimits, AssetImportSource, SourceAssetImporter};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorProjectBrowserLocalAction,
    EditorSourceAssetImportAction, EditorSourceAssetImportAttempt, EditorSourceAssetMetadataView,
};
use std::{
    fs,
    time::{Duration, Instant},
};

fn drive(application: &mut nara_editor::EditorApplication) {
    if let Some(frame) = application.current_frame() {
        let update = application
            .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO);
        if let Some(after) = application.current_frame()
            && (frame.source_asset_import() != after.source_asset_import()
                || frame.source_asset_import_work_pending()
                    != after.source_asset_import_work_pending()
                || frame.image_import_work_pending() != after.image_import_work_pending())
            && !update.update().actions_observed()
        {
            close(application);
            panic!("a source import or metadata discovery change must request presentation");
        }
    } else {
        application.advance_project_open();
    }
    std::thread::sleep(Duration::from_millis(1));
}

fn wait(
    application: &mut nara_editor::EditorApplication,
    predicate: impl Fn(&nara_tooling::EditorFrameModel) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if application
            .current_frame()
            .is_some_and(|frame| predicate(&frame))
        {
            return;
        }
        if Instant::now() >= deadline {
            let frame = application.current_frame();
            close(application);
            panic!("Editor did not reach the expected import state: {frame:?}");
        }
        drive(application);
    }
}

fn submit(application: &mut nara_editor::EditorApplication, maximum: usize) {
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(frame.revision(), [action(maximum)]),
        Duration::ZERO,
    );
}

fn action(maximum: usize) -> EditorAction {
    use nara::project_host::EditorSourceAssetDomain;

    EditorAction::SourceAssetImport(
        text_import::TextImportDomain::import_action(
            "assets/caption.txt",
            &nara_asset_domain_consumer::TextSettings {
                max_characters: maximum,
            },
        )
        .unwrap(),
    )
}

fn close(application: &mut nara_editor::EditorApplication) {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut ready = application.request_exit().exit_ready();
    while !ready {
        assert!(
            Instant::now() < deadline,
            "Editor retained incomplete close"
        );
        ready = if let Some(frame) = application.current_frame() {
            application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                .update()
                .exit_ready()
        } else {
            application.advance_project_open().exit_ready()
        };
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn the_editor_selects_the_text_domain_explicitly_and_rejects_duplicate_owners() {
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition).unwrap();
    assert!(text_import::install(&mut composition).is_err());
}

#[test]
fn project_replacement_keeps_the_compiled_domain_and_resets_operation_state() {
    check_project_replacement(false);
}

#[test]
fn trusted_compiled_construction_and_replacement_keep_the_selected_domain() {
    check_project_replacement(true);
}

#[test]
fn late_panel_installation_rejects_host_facets_without_mutating_the_workspace() {
    let mut application =
        nara_editor::EditorApplication::new(nara_editor::EditorLaunchOptions::project_manager());
    let before = application.panel_workspace().layout().to_json().unwrap();
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition).unwrap();
    text_import::install_panel(&mut composition).unwrap();
    assert!(matches!(
        composition.install_panels(&mut application),
        Err(nara_editor::EditorCompositionError::HostContributionsRequireBuild)
    ));
    assert_eq!(
        application.panel_workspace().layout().to_json().unwrap(),
        before
    );
    assert!(
        application
            .panel_workspace()
            .registry()
            .descriptor("consumer.text-import")
            .is_none()
    );
}

fn check_project_replacement(trusted: bool) {
    let first = text_project::TextProject::new();
    let second = text_project::TextProject::new();
    fs::write(
        second.root.join("assets/replacement.marker"),
        b"replacement",
    )
    .unwrap();
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition).unwrap();
    let options = nara_editor::EditorLaunchOptions::new(&first.root);
    let (mut application, _frontend) = if trusted {
        composition
            .build_trusted_compiled(
                options,
                nara::project_host::TrustedCompiledEditorProjectIntent::new()
                    .with_recipe(text_project::recipe()),
            )
            .unwrap()
    } else {
        composition.build(options).unwrap()
    };
    application.native_shell_ready();
    let original = if trusted {
        let frame = application.current_frame().unwrap();
        assert!(frame.source_asset_import().is_none());
        assert_eq!(
            frame.project_browser().failure().unwrap().code(),
            "project.source-index.authoring-project-required"
        );
        None
    } else {
        wait(&mut application, |frame| {
            frame
                .project_browser()
                .entry("assets/caption.txt")
                .is_some()
                && !frame.image_import_work_pending()
        });
        submit(&mut application, 64);
        wait(&mut application, |frame| {
            matches!(
                frame.source_asset_import().map(|view| &view.attempt),
                Some(EditorSourceAssetImportAttempt::MetadataPublished { .. })
            )
        });
        Some(fs::read(first.root.join("assets/caption.txt.meta")).unwrap())
    };
    assert!(application.request_project_replacement(&second.root, None));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !application.current_frame().is_some_and(|frame| {
        frame
            .project_browser()
            .entry("assets/replacement.marker")
            .is_some()
            && !frame.image_import_work_pending()
    }) {
        assert!(
            Instant::now() < deadline,
            "replacement project did not open"
        );
        application.advance_project_open();
        drive(&mut application);
    }
    assert!(application.apply_project_browser_local_action(
        EditorProjectBrowserLocalAction::Select {
            relative_path: Some("assets/caption.txt".into())
        }
    ));
    wait(&mut application, |frame| {
        frame.source_asset_import().is_some()
    });
    let frame = application.current_frame().unwrap();
    let view = frame.source_asset_import().unwrap();
    assert_eq!(view.attempt, EditorSourceAssetImportAttempt::Idle);
    assert_eq!(view.metadata, EditorSourceAssetMetadataView::Missing);
    assert!(!second.root.join("assets/caption.txt.meta").exists());
    submit(&mut application, 128);
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.attempt),
            Some(EditorSourceAssetImportAttempt::MetadataPublished { .. })
        )
    });
    let changed = fs::read(second.root.join("assets/caption.txt.meta")).unwrap();
    assert_eq!(
        AssetMetaIdentityCandidate::decode_json_bytes(&changed)
            .unwrap()
            .admit::<nara_asset_domain_consumer::TextSettings>()
            .unwrap()
            .settings()
            .max_characters,
        128
    );
    assert_eq!(
        fs::read(first.root.join("assets/caption.txt.meta")).ok(),
        original
    );
    close(&mut application);
}

#[test]
fn a_compiled_editor_imports_reimports_and_preserves_metadata_before_check_and_play() {
    let project = text_project::TextProject::new();
    let metadata_path = project.root.join("assets/caption.txt.meta");
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition).unwrap();
    let (mut application, _frontend) = composition
        .build(nara_editor::EditorLaunchOptions::new(&project.root))
        .unwrap();
    application.native_shell_ready();
    wait(&mut application, |frame| {
        frame
            .project_browser()
            .entry("assets/caption.txt")
            .is_some()
            && !frame.image_import_work_pending()
    });
    assert!(application.apply_project_browser_local_action(
        EditorProjectBrowserLocalAction::Select {
            relative_path: Some("assets/caption.txt".into())
        }
    ));
    wait(&mut application, |frame| {
        frame.source_asset_import().is_some()
    });
    assert!(matches!(
        application
            .current_frame()
            .unwrap()
            .source_asset_import()
            .unwrap()
            .metadata,
        EditorSourceAssetMetadataView::Missing
    ));
    assert!(!metadata_path.exists(), "Open must not import implicitly");

    submit(&mut application, 64);
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.attempt),
            Some(EditorSourceAssetImportAttempt::MetadataPublished { .. })
        )
    });
    let initial = fs::read(&metadata_path).unwrap();
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes(&initial).unwrap();
    let stable_id = candidate.identity().stable_id();
    assert_eq!(
        candidate
            .admit::<nara_asset_domain_consumer::TextSettings>()
            .unwrap()
            .settings()
            .max_characters,
        64
    );
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.metadata),
            Some(EditorSourceAssetMetadataView::Ready { .. })
        )
    });

    fs::write(project.root.join("assets/caption.txt"), b"\xff").unwrap();
    submit(&mut application, 32);
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.attempt),
            Some(EditorSourceAssetImportAttempt::Failed { .. })
        )
    });
    assert_eq!(fs::read(&metadata_path).unwrap(), initial);
    fs::write(
        project.root.join("assets/caption.txt"),
        b"suffix.txt\nHello",
    )
    .unwrap();

    let stale = application.current_frame().unwrap();
    drive(&mut application);
    application.submit_and_drive(
        EditorActionBatch::new(stale.revision(), [action(128)]),
        Duration::ZERO,
    );
    assert_eq!(fs::read(&metadata_path).unwrap(), initial);
    assert!(
        !application
            .current_frame()
            .unwrap()
            .source_asset_import_work_pending()
    );

    submit(&mut application, 128);
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.attempt),
            Some(EditorSourceAssetImportAttempt::MetadataPublished { .. })
        )
    });
    let changed = fs::read(&metadata_path).unwrap();
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes(&changed).unwrap();
    assert_eq!(candidate.identity().stable_id(), stable_id);
    assert_eq!(
        candidate
            .admit::<nara_asset_domain_consumer::TextSettings>()
            .unwrap()
            .settings()
            .max_characters,
        128
    );
    for _ in 0..3 {
        let frame = application.current_frame().unwrap();
        assert!(
            !application
                .submit_and_drive(EditorActionBatch::new(frame.revision(), []), Duration::ZERO)
                .update()
                .actions_observed(),
            "unchanged source import frames must remain quiet"
        );
    }
    close(&mut application);
    drop(application);
    project.assert_check_and_play();
}

static PREPARATION_GATE: std::sync::Mutex<
    Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
> = std::sync::Mutex::new(None);

struct DelayedTextDomain;
impl nara_asset::AssetDomain for DelayedTextDomain {
    type Product = nara_asset_domain_consumer::TextResource;
}

impl SourceAssetImporter for DelayedTextDomain {
    type Settings = nara_asset_domain_consumer::TextSettings;
    const SOURCE_KIND: &'static str = "consumer.delayed-text";
    fn limits() -> AssetImportLimits {
        nara_asset_domain_consumer::TextDomain::limits()
    }
    fn prepare<E>(
        source: &mut AssetImportSource<'_, E>,
    ) -> Result<Self::Product, AssetImportError<E>> {
        let product = nara_asset_domain_consumer::TextDomain::prepare(source)?;
        let (started, release) = PREPARATION_GATE.lock().unwrap().take().unwrap();
        started.send(()).unwrap();
        release.recv().unwrap();
        Ok(product)
    }
}
impl nara::project_host::EditorSourceAssetDomain for DelayedTextDomain {
    type Domain = Self;
    fn default_settings() -> nara_asset_domain_consumer::TextSettings {
        nara_asset_domain_consumer::TextSettings { max_characters: 64 }
    }
}

struct Release(std::sync::mpsc::Sender<()>);
impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn closing_during_domain_preparation_waits_for_physical_retirement_without_publishing() {
    let project = text_project::TextProject::new();
    let mut composition = EditorNativeComposition::new();
    composition
        .register_source_asset_domain::<DelayedTextDomain>()
        .unwrap();
    let (mut application, _frontend) = composition
        .build(nara_editor::EditorLaunchOptions::new(&project.root))
        .unwrap();
    application.native_shell_ready();
    wait(&mut application, |frame| {
        frame
            .project_browser()
            .entry("assets/caption.txt")
            .is_some()
            && !frame.image_import_work_pending()
    });
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release = Release(release_tx);
    *PREPARATION_GATE.lock().unwrap() = Some((started_tx, release_rx));
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::SourceAssetImport(
                EditorSourceAssetImportAction {
                    relative_path: "assets/caption.txt".into(),
                    source_kind: "consumer.delayed-text".into(),
                    settings: "{\"max_characters\":64}".into(),
                },
            )],
        ),
        Duration::ZERO,
    );
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(
        !application.request_exit().exit_ready(),
        "a live parser must prevent physical close"
    );
    wait(&mut application, |frame| {
        matches!(
            frame.workspace_intent().result(),
            Some(nara_tooling::EditorWorkspaceIntentResult::Applied {
                intent: nara_tooling::EditorWorkspaceIntent::Exit,
            })
        )
    });
    let closing = application.advance(Duration::ZERO);
    assert!(
        !closing.exit_ready(),
        "cancellation must retain the held parser through physical return"
    );
    assert!(
        application
            .current_frame()
            .unwrap()
            .source_asset_import_work_pending()
    );
    assert!(!project.root.join("assets/caption.txt.meta").exists());
    drop(release);
    close(&mut application);
    assert!(!project.root.join("assets/caption.txt.meta").exists());
}

#[test]
fn an_auxiliary_change_after_preparation_preserves_the_accepted_metadata() {
    use nara::asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId};

    let project = text_project::TextProject::new();
    let target = project.root.join("assets/caption.txt.meta");
    let before = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::try_random().unwrap(),
            AssetPath::new("caption.txt").unwrap(),
            AssetSourceKind::Other("consumer.delayed-text".into()),
        ),
        nara_asset_domain_consumer::TextSettings { max_characters: 64 },
    )
    .to_json_string()
    .unwrap()
    .into_bytes();
    fs::write(&target, &before).unwrap();
    let mut composition = EditorNativeComposition::new();
    composition
        .register_source_asset_domain::<DelayedTextDomain>()
        .unwrap();
    let (mut application, _frontend) = composition
        .build(nara_editor::EditorLaunchOptions::new(&project.root))
        .unwrap();
    application.native_shell_ready();
    wait(&mut application, |frame| {
        frame
            .project_browser()
            .entry("assets/caption.txt")
            .is_some()
            && !frame.image_import_work_pending()
    });
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release = Release(release_tx);
    *PREPARATION_GATE.lock().unwrap() = Some((started_tx, release_rx));
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::SourceAssetImport(
                EditorSourceAssetImportAction {
                    relative_path: "assets/caption.txt".into(),
                    source_kind: "consumer.delayed-text".into(),
                    settings: "{\"max_characters\":128}".into(),
                },
            )],
        ),
        Duration::ZERO,
    );
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    fs::write(project.root.join("assets/suffix.txt"), b" earth").unwrap();
    drop(release);
    wait(&mut application, |frame| {
        !frame.source_asset_import_work_pending()
    });
    let frame = application.current_frame().unwrap();
    let Some(EditorSourceAssetImportAttempt::Failed { diagnostics }) =
        frame.source_asset_import().map(|view| &view.attempt)
    else {
        panic!(
            "changed observed input was not rejected: {:?}",
            frame.source_asset_import()
        );
    };
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str()
                == "project.editor.source-import-source-changed")
    );
    assert_eq!(fs::read(target).unwrap(), before);
    close(&mut application);
}

#[test]
fn conflicting_metadata_prevents_source_import_and_preserves_the_exact_target() {
    use nara::asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetSourceKind};
    let project = text_project::TextProject::new();
    let mut composition = EditorNativeComposition::new();
    text_import::install(&mut composition).unwrap();
    let (mut application, _frontend) = composition
        .build(nara_editor::EditorLaunchOptions::new(&project.root))
        .unwrap();
    application.native_shell_ready();
    wait(&mut application, |frame| {
        frame
            .project_browser()
            .entry("assets/caption.txt")
            .is_some()
            && !frame.image_import_work_pending()
    });
    submit(&mut application, 64);
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.metadata),
            Some(EditorSourceAssetMetadataView::Ready { .. })
        )
    });
    let target = project.root.join("assets/caption.txt.meta");
    let before = fs::read(&target).unwrap();
    let stable_id = AssetMetaIdentityCandidate::decode_json_bytes(&before)
        .unwrap()
        .identity()
        .stable_id();
    fs::write(project.root.join("assets/copy.txt"), b"suffix.txt\nCopy").unwrap();
    fs::write(
        project.root.join("assets/copy.txt.meta"),
        AssetMeta::new(
            AssetMetaIdentity::new(
                stable_id,
                AssetPath::new("copy.txt").unwrap(),
                AssetSourceKind::Other("consumer.text".into()),
            ),
            nara_asset_domain_consumer::TextSettings { max_characters: 64 },
        )
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let frame = application.current_frame().unwrap();
    application.submit_and_drive(
        EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(
                nara_tooling::EditorProjectBrowserAction::Refresh,
            )],
        ),
        Duration::ZERO,
    );
    wait(&mut application, |frame| {
        matches!(
            frame.source_asset_import().map(|view| &view.metadata),
            Some(EditorSourceAssetMetadataView::IdentityConflict { .. })
        )
    });
    submit(&mut application, 128);
    assert!(
        !application
            .current_frame()
            .unwrap()
            .source_asset_import_work_pending()
    );
    assert_eq!(fs::read(target).unwrap(), before);
    close(&mut application);
}
