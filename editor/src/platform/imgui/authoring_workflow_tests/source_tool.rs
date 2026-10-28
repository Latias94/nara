//! Application metadata adoption followed by installed C# proposals through native widgets.

use std::os::windows::fs::OpenOptionsExt;

use nara::{
    fs::{
        CapabilityRights, ContentDigest, DirectoryCapability, HostCapabilityOptions,
        RelativeComponent, TrustMode,
    },
    project_host::{
        EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionOutcome,
        EditorSchemaOwnerAdoptionPhase, EditorSchemaOwnerAdoptionView,
        EditorSourceToolInstallationOutcome, EditorSourceToolInstallationPlan,
        EditorSourceToolManagedHost, EditorSourceToolManagedHostDigests,
        PreparedEditorSourceToolRelease, advanced::built_in_schema_providers,
        open_explicit_product_project_root,
    },
    reflect::{
        AuthoringSchemaBundle, ComponentRegistry, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponentSchemaOwnerId,
    },
    scene::{
        InMemoryPrefabSourceResolver, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
        SceneEntityRecord, SceneOccurrenceKey, ScenePatchOperation, SourceDocumentKey,
        SourceDocumentKind, retire_and_despawn_scene_instance,
    },
    tooling::{
        EditorSourceToolDesiredSelection, EditorSourceToolIntent, EditorSourceToolRequestId,
        EditorSourceToolSelectionOutcome, EditorSourceToolSelectionPhase, EditorSourceToolView,
        SceneInspectorReadOnlyReason,
    },
};

use super::*;
use crate::frontend::imgui::{control_clip, control_visible};
use crate::workspace::{EditorPanelInstanceId, EditorPanelPlacement, EditorPanelTarget};

const STATS: &str = "trial.enemy.EnemyStats";
const NAME: &str = "nara.scene.Name";
const SCENE: &str = "worlds/entry.scene.json";
const TOOL_TIMEOUT: Duration = Duration::from_secs(30);

// Keep the actual widget identities in one place for the independently owned frontend.
const BONUS_INPUT: &str = "source-tool:integer:parameter";
const PROPERTY_PREVIEW: &str = "source-tool:integer:prepare";
const PROPERTY_ACCEPT: &str = "source-tool:integer:accept";
const PANEL_ATTACH: &str = "source-tool:attach-or-edit";
const PANEL_ACCEPT: &str = "source-tool:accept";
const PANEL_DISABLE: &str = "source-tool:disable";
const PANEL_DISABLE_SAVED: &str = "source-tool:disable-saved";
const INSTALLATION: &str = "installed-tool";
const SELECTION: &str = ".nara/source-tools/selection.json";

struct SourceToolFixture {
    project: InteractionProject,
    owner: ComponentSchemaOwnerSnapshot,
    installed: InstalledSourceTool,
    original: SceneDocument,
}

struct InstalledSourceTool {
    root: DirectoryCapability,
    receipt: ContentDigest,
    host: EditorSourceToolManagedHost,
}

impl SourceToolFixture {
    fn new() -> Result<Self, String> {
        let directory = PathBuf::from(
            std::env::var_os("NARA_SOURCE_TOOL_V1_DIR")
                .ok_or("NARA_SOURCE_TOOL_V1_DIR must name the actual prebuilt C# tool")?,
        );
        let executable = PathBuf::from(
            std::env::var_os("NARA_SOURCE_TOOL_DOTNET")
                .ok_or("NARA_SOURCE_TOOL_DOTNET must name the resolved dotnet executable")?,
        );
        let catalog = ComponentSchemaCatalog::from_json_bytes(
            &fs::read(directory.join("enemy-stats.catalog.json"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let owner = ComponentSchemaOwnerSnapshot::admit(
            PersistentComponentSchemaOwnerId::try_new("trial.enemy-stats")
                .map_err(|error| error.to_string())?,
            catalog,
            None,
        )
        .map_err(|error| error.to_string())?;
        let bundle =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .map_err(|error| error.to_string())?;
        if bundle.owner(owner.owner_id()).is_some() {
            return Err("the initial native bundle already declares the installed owner".into());
        }
        let name_owner = bundle
            .owner_for_component(&ComponentTypeId::new(NAME))
            .ok_or("the trusted builtins do not declare Name")?;
        let named = |entity, name: &str| {
            SceneEntityRecord::new(SceneEntityId::new(entity).unwrap()).with_component(
                ComponentTypeId::new(NAME),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::String(name.into()),
                )
                .with_owner(name_owner.owner_id().clone()),
            )
        };
        let original = SceneDocument::from_ordered_roots([
            named("enemy", "Guard").with_component(
                ComponentTypeId::new(STATS),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, stats(100, 10))
                    .with_owner(owner.owner_id().clone()),
            ),
            named("recruit", "Recruit"),
        ]);
        let project = InteractionProject::new();
        let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound/media");
        for member in ["icon.png", "icon.png.meta"] {
            fs::copy(assets.join(member), project.root.join("media").join(member))
                .map_err(|error| error.to_string())?;
        }
        fs::write(
            project.root.join("catalogs/authoring.bundle.json"),
            bundle.to_json_string().map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            project.root.join(SCENE),
            original
                .to_json_string()
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let original_bundle = fs::read(project.root.join("catalogs/authoring.bundle.json"))
            .map_err(|error| error.to_string())?;
        let installed = installed_tool(&project, &directory, &executable)?;
        if fs::read(project.root.join("catalogs/authoring.bundle.json"))
            .map_err(|error| error.to_string())?
            != original_bundle
        {
            return Err("release installation changed the native-only bundle".into());
        }
        Ok(Self {
            project,
            owner,
            installed,
            original,
        })
    }
}

fn installed_tool(
    project: &InteractionProject,
    artifacts: &Path,
    executable: &Path,
) -> Result<InstalledSourceTool, String> {
    let source = project.root.join("tool-release-source");
    fs::create_dir(&source).map_err(|error| error.to_string())?;
    let mut files = Vec::new();
    for member in [
        "EnemyStatsTool.dll",
        "EnemyStatsTool.deps.json",
        "EnemyStatsTool.runtimeconfig.json",
        "enemy-stats.catalog.json",
    ] {
        let bytes = fs::read(artifacts.join(member)).map_err(|error| error.to_string())?;
        fs::write(source.join(member), &bytes).map_err(|error| error.to_string())?;
        let digest = ContentDigest::of_bytes(&bytes);
        let hash: String = digest
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        files.push(serde_json::json!({
            "path": member,
            "length": digest.length(),
            "blake3": hash,
            "kind": "tool",
        }));
    }
    let manifest = serde_json::to_vec(&serde_json::json!({
        "format_version": 1,
        "package": "trial.enemy-stats",
        "release": "1",
        "host_target": "x86_64-pc-windows-msvc",
        "authority": "trusted_process",
        "protocol_version": 1,
        "owner": "trial.enemy-stats",
        "component": STATS,
        "catalog": "enemy-stats.catalog.json",
        "entry": "EnemyStatsTool.dll",
        "requires": [],
        "files": files,
    }))
    .map_err(|error| error.to_string())?;
    fs::write(source.join("nara-tool.json"), &manifest).map_err(|error| error.to_string())?;
    let captured = PreparedEditorSourceToolRelease::capture(
        &open_explicit_product_project_root(&source).map_err(|error| error.to_string())?,
        ContentDigest::of_bytes(&manifest),
    )
    .map_err(|error| error.to_string())?;
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(&project.root)
        .map_err(|error| error.to_string())?;
    let parent = DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::Untrusted),
    )
    .map_err(|error| error.to_string())?;
    let plan = EditorSourceToolInstallationPlan::new(
        parent.share(),
        RelativeComponent::new(INSTALLATION).map_err(|error| error.to_string())?,
        captured,
    )
    .map_err(|error| error.to_string())?;
    let installation = match plan.publish() {
        EditorSourceToolInstallationOutcome::Published(installation) => installation,
        other => {
            return Err(format!(
                "actual source-tool release publication failed: {other:?}"
            ));
        }
    };
    let receipt = installation.receipt_digest();
    let bootstrap = PathBuf::from(
        std::env::var_os("NARA_SOURCE_TOOL_BOOTSTRAP_DIR")
            .ok_or("NARA_SOURCE_TOOL_BOOTSTRAP_DIR must name the actual prebuilt managed Host")?,
    );
    let framework = std::env::var("NARA_SOURCE_TOOL_FRAMEWORK").map_err(|error| {
        format!("NARA_SOURCE_TOOL_FRAMEWORK must select the exact framework: {error}")
    })?;
    let digest = |path: &Path| {
        fs::read(path)
            .map(|bytes| ContentDigest::of_bytes(&bytes))
            .map_err(|error| error.to_string())
    };
    let host = EditorSourceToolManagedHost::new(
        open_explicit_product_project_root(
            executable
                .parent()
                .ok_or("the selected dotnet executable has no parent")?,
        )
        .map_err(|error| error.to_string())?,
        open_explicit_product_project_root(&bootstrap).map_err(|error| error.to_string())?,
        EditorSourceToolManagedHostDigests {
            launcher: digest(executable)?,
            bootstrap: digest(&bootstrap.join("Nara.SourceToolHost.dll"))?,
            deps: digest(&bootstrap.join("Nara.SourceToolHost.deps.json"))?,
            runtimeconfig: digest(&bootstrap.join("Nara.SourceToolHost.runtimeconfig.json"))?,
        },
        &framework,
        TOOL_TIMEOUT,
    )
    .map_err(|error| error.to_string())?;
    drop(installation);
    Ok(InstalledSourceTool {
        root: parent,
        receipt,
        host,
    })
}

struct SourceToolObservation {
    presents: u64,
    history: [usize; 4],
    health: [i64; 4],
    saved: SceneDocument,
    desired: EditorSourceToolDesiredSelection,
    selection_bytes: Vec<u8>,
}

struct SourceToolApplication<'a> {
    fixture: &'a SourceToolFixture,
    result: Option<Result<(SourceToolObservation, u64), String>>,
}

#[test]
#[ignore = "requires Windows DX12 and actual installed EnemyStats images with managed Host approval"]
fn native_source_tool_inline_property_and_panel_share_review_history_and_persistence() {
    let fixture = SourceToolFixture::new().unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = SourceToolApplication {
        fixture: &fixture,
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Both native graphics scopes and their complete Application owners have retired here.
    let (observed, reopened_presents) = application
        .result
        .expect("the actual source-tool widget workflow must execute")
        .unwrap();
    assert!(observed.presents > 0 && reopened_presents > 0);
    assert_eq!(observed.history, [0, 1, 0, 1]);
    assert_eq!(observed.health, [100, 125, 100, 125]);
    let mut expected = fixture.original.clone();
    expected.entities[0]
        .components
        .get_mut(&ComponentTypeId::new(STATS))
        .unwrap()
        .value = stats(125, 10);
    expected.entities[1].components.insert(
        ComponentTypeId::new(STATS),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, stats(100, 10))
            .with_owner(fixture.owner.owner_id().clone()),
    );
    assert_eq!(observed.saved, expected);
    assert_saved_rust_data(&observed.saved, &fixture.owner).unwrap();
}

impl ApplicationHandler for SourceToolApplication<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some((|| {
                let observed =
                    run_native_workflow(event_loop, &self.fixture.project.root, |workflow| {
                        workflow.run_source_tool(self.fixture)
                    })?;
                // run_native_workflow finishes graphics, physically retires the Session and drops
                // the old Application before a completely fresh Open can acquire saved source.
                let presents =
                    run_native_workflow(event_loop, &self.fixture.project.root, |workflow| {
                        workflow.reopen_source_tool(self.fixture, &observed)
                    })?;
                Ok((observed, presents))
            })());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn settle_native_schema_owner(
        &mut self,
        phase: EditorSchemaOwnerAdoptionPhase,
    ) -> Result<EditorSchemaOwnerAdoptionView, String> {
        let deadline = Instant::now() + TOOL_TIMEOUT;
        loop {
            self.paint(|_| {})?;
            let view = self
                .application
                .schema_owner_adoption_view()
                .map_err(|error| error.to_string())?;
            if view.phase == phase {
                return Ok(view.clone());
            }
            if matches!(
                view.phase,
                EditorSchemaOwnerAdoptionPhase::Terminal
                    | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
            ) || Instant::now() >= deadline
            {
                return Err(format!(
                    "native Application metadata adoption did not reach {phase:?}: {view:?}"
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn adopt_installed_source_tool_owner(
        &mut self,
        fixture: &SourceToolFixture,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_document
            .ok_or("Open has no active document")?;
        let before = frame
            .workspace()
            .document(document)
            .ok_or("the initial source tab is missing")?
            .clone();
        let selection = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the initial Scene selection is missing")?
            .selection
            .clone();
        let old_basis = self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("{error:?}"))?;
        let unknown_stats = |frame: &EditorFrameModel| {
            frame
                .workspace()
                .active_scene
                .as_ref()
                .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
                .and_then(|entity| {
                    entity
                        .components
                        .iter()
                        .find(|component| component.component.as_str() == STATS)
                })
                .is_some_and(|component| {
                    component.access.read_only_reason()
                        == Some(SceneInspectorReadOnlyReason::UnknownSchema)
                })
        };
        if old_basis.schema.as_ref().is_none_or(|basis| {
            basis
                .component_schema_owner(&ComponentTypeId::new(STATS))
                .is_some()
        }) || !unknown_stats(&frame)
            || before.history.undo_depth != 0
            || before.history.redo_depth != 0
            || before.dirty
            || control_rect(&format!("field:{STATS}:health")).is_some()
        {
            return Err("ordinary native Open must retain clean unknown EnemyStats under the native-only bundle".into());
        }
        let source_path = fixture.project.root.join(SCENE);
        let bundle_path = fixture.project.root.join("catalogs/authoring.bundle.json");
        let original_source = fs::read(&source_path).map_err(|error| error.to_string())?;
        let original_bundle = fs::read(&bundle_path).map_err(|error| error.to_string())?;
        let native = AuthoringSchemaBundle::from_json_bytes(&original_bundle)
            .map_err(|error| error.to_string())?;
        if native.owner(fixture.owner.owner_id()).is_some()
            || read_scene(&source_path)? != fixture.original
        {
            return Err(
                "the missing-owner native fixture changed before metadata preparation".into(),
            );
        }
        let before_tool = self.frame()?.source_tool().clone();
        let deadline = Instant::now() + TOOL_TIMEOUT;
        let operation = loop {
            match self
                .application
                .prepare_installed_schema_owner(INSTALLATION, fixture.installed.receipt)
            {
                Ok(operation) => break operation,
                Err(EditorSchemaOwnerAdoptionError::Busy) if Instant::now() < deadline => {
                    self.paint(|_| {})?;
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => {
                    return Err(format!(
                        "native Application metadata preparation failed: {error:?}"
                    ));
                }
            }
        };
        let review = self.settle_native_schema_owner(EditorSchemaOwnerAdoptionPhase::Review)?;
        let reviewed = self.frame()?;
        if review.operation != Some(operation)
            || review.owner.as_deref() != Some(fixture.owner.owner_id().as_str())
            || review.documents != 1
            || review.outcome.is_some()
            || review.reconciliation
            || self
                .application
                .schema_owner_adoption_publication()
                .is_some()
            || self
                .application
                .document_query_basis(document)
                .map_err(|error| format!("{error:?}"))?
                != old_basis
            || reviewed.workspace().document(document) != Some(&before)
            || !unknown_stats(&reviewed)
            || reviewed.source_tool() != &before_tool
            || fs::read(&source_path).map_err(|error| error.to_string())? != original_source
            || fs::read(&bundle_path).map_err(|error| error.to_string())? != original_bundle
        {
            return Err(format!(
                "native Application metadata Review changed incumbent source, history or files: {review:?}"
            ));
        }

        // This calls the public Application boundary; no metadata-adoption widget is qualified.
        self.application
            .adopt_schema_owner(operation)
            .map_err(|error| error.to_string())?;
        let adopted = self.settle_native_schema_owner(EditorSchemaOwnerAdoptionPhase::Terminal)?;
        let basis = self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("{error:?}"))?;
        let frame = self.frame()?;
        let after = frame
            .workspace()
            .document(document)
            .ok_or("adoption lost the original tab")?;
        if adopted.operation != Some(operation)
            || adopted.outcome != Some(EditorSchemaOwnerAdoptionOutcome::Adopted)
            || basis.revision != old_basis.revision
            || basis.schema == old_basis.schema
            || basis
                .schema
                .as_ref()
                .and_then(|schema| schema.component_schema_owner(&ComponentTypeId::new(STATS)))
                != Some(fixture.owner.owner_id())
            || after.saved_revision != before.saved_revision
            || after.history != before.history
            || after.dirty != before.dirty
            || after.source_binding != before.source_binding
            || frame
                .workspace()
                .active_scene
                .as_ref()
                .is_none_or(|scene| scene.selection != selection)
            || self.selected_stats()? != (100, 10)
            || frame.source_tool() != &before_tool
            || fixture.project.root.join(SELECTION).exists()
            || fs::read(&source_path).map_err(|error| error.to_string())? != original_source
        {
            return Err(format!(
                "native Application metadata adoption did not preserve the original source state: {adopted:?}"
            ));
        }
        let published_bundle = fs::read(&bundle_path).map_err(|error| error.to_string())?;
        let expected = AuthoringSchemaBundle::from_owner_snapshots(
            native
                .owners()
                .iter()
                .cloned()
                .chain([fixture.owner.clone()]),
        )
        .map_err(|error| error.to_string())?;
        if AuthoringSchemaBundle::from_json_bytes(&published_bundle)
            .map_err(|error| error.to_string())?
            != expected
            || self
                .application
                .schema_owner_adoption_publication()
                .is_none_or(|receipt| {
                    receipt.verified_content_digest()
                        != Some(ContentDigest::of_bytes(&published_bundle))
                })
        {
            return Err(
                "native metadata adoption did not publish the exact additive bundle".into(),
            );
        }
        self.application
            .acknowledge_schema_owner_adoption(operation)
            .map_err(|error| error.to_string())?;
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        Ok(())
    }

    fn open_source_tool_document(&mut self) -> Result<(), String> {
        let backend = self
            .native
            .gpu
            .with_device(
                super::super::super::editor_window_packet(&self.native.window),
                |scope| scope.adapter().get_info().backend,
            )
            .map_err(|error| error.to_string())?;
        if backend != dear_imgui_wgpu::wgpu::Backend::Dx12 {
            return Err(format!(
                "the native source-tool qualification requires DX12, selected {backend:?}"
            ));
        }
        let deadline = Instant::now() + TOOL_TIMEOUT;
        loop {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "ordinary Open did not publish: {:?}",
                    self.application.state()
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        let view = self.settle_actual_source_tool()?;
        let selection = view
            .selection
            .as_ref()
            .ok_or("ordinary Open did not observe the installed selection")?;
        if view.active.is_some()
            || view.attempt.is_some()
            || view.request.is_some()
            || view.review.is_some()
            || selection.active_origin.is_some()
            || selection.phase != EditorSourceToolSelectionPhase::Idle
            || selection.error.is_some()
            || selection.operation.as_ref().is_none_or(|operation| {
                operation.outcome != Some(EditorSourceToolSelectionOutcome::Observed)
            })
        {
            return Err(format!(
                "ordinary Open executed or activated a source tool: {view:?}"
            ));
        }
        self.click("entity:enemy")
    }

    fn settle_actual_source_tool(&mut self) -> Result<EditorSourceToolView, String> {
        let deadline = Instant::now() + TOOL_TIMEOUT;
        loop {
            let presented = self.frame()?.source_tool().clone();
            self.paint(|_| {})?;
            let view = self.frame()?.source_tool().clone();
            // An after-paint owner drive can publish completion for the following redraw.
            // Require a paint which actually started from that terminal tool observation.
            if !view.has_pending_work()
                && view == presented
                && view.selection.as_ref().is_some_and(|selection| {
                    selection
                        .operation
                        .as_ref()
                        .is_some_and(|operation| operation.outcome.is_some())
                        && selection
                            .disable
                            .as_ref()
                            .is_none_or(|disable| disable.outcome.is_some())
                })
            {
                if view.attempt_error.is_some()
                    || view.request_error.is_some()
                    || view
                        .selection
                        .as_ref()
                        .is_some_and(|selection| selection.error.is_some())
                {
                    return Err(format!("the actual C# tool failed: {view:?}"));
                }
                return Ok(view);
            }
            if Instant::now() >= deadline {
                return Err(format!("the actual C# child did not return: {view:?}"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn enable_installed_source_tool(
        &mut self,
        fixture: &SourceToolFixture,
    ) -> Result<EditorSourceToolView, String> {
        let selection = self
            .application
            .source_tool_selection_view()
            .map_err(|error| error.to_string())?;
        let prepared = self
            .application
            .prepare_installed_source_tool_selection(
                fixture.installed.root.share(),
                INSTALLATION,
                fixture.installed.receipt,
                &fixture.installed.host,
            )
            .map_err(|error| error.to_string())?;
        let installed = prepared.selection().clone();
        if self
            .application
            .source_tool_selection_view()
            .map_err(|error| error.to_string())?
            != selection
        {
            return Err("Host preparation changed the installed selection".into());
        }
        let attempt = self
            .application
            .enable_installed_source_tool(&selection.basis, prepared)
            .map_err(|error| error.to_string())?;
        let view = self.settle_actual_source_tool()?;
        let selection = view
            .selection
            .as_ref()
            .ok_or("enabled selection is missing")?;
        if selection.phase != EditorSourceToolSelectionPhase::Idle
            || selection
                .desired
                .as_ref()
                .is_none_or(|desired| !desired.enabled || desired.installed != installed)
            || selection.active_origin.as_ref() != Some(&installed)
            || selection.operation.as_ref().is_none_or(|operation| {
                operation.attempt != attempt
                    || operation.outcome != Some(EditorSourceToolSelectionOutcome::Enabled)
            })
        {
            return Err(format!(
                "installed selection was not durably enabled: {view:?}"
            ));
        }
        Ok(view)
    }

    fn selected_stats(&self) -> Result<(i64, i64), String> {
        let frame = self.frame()?;
        let component = frame
            .workspace()
            .active_scene
            .as_ref()
            .and_then(|scene| scene.editor.inspector.selected_entity_view.as_ref())
            .and_then(|entity| {
                entity
                    .components
                    .iter()
                    .find(|component| component.component.as_str() == STATS)
            })
            .ok_or("the selected entity does not expose the admitted Stats component")?;
        let value = |id| {
            component
                .fields
                .iter()
                .find(|field| field.id.as_str() == id)
                .and_then(|field| field.value.as_ref())
                .and_then(ComponentValue::as_i64)
                .ok_or_else(|| format!("the selected Stats {id} field is unavailable"))
        };
        Ok((value("health")?, value("armor")?))
    }

    fn open_source_tool_panel(&mut self) -> Result<EditorPanelInstanceId, String> {
        let panel = self
            .application
            .panel_workspace_mut()
            .open("nara.source-tool", EditorPanelTarget::ActiveDocument)
            .map_err(|error| error.to_string())?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .place(panel, EditorPanelPlacement::Floating);
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        Ok(panel)
    }

    fn source_tool_control(&mut self, label: &str) -> Result<(), String> {
        // The small floating builtin panel may need ordinary wheel scrolling to expose review.
        // Use its actual draw clip and owned Context input, as in the conversion qualification.
        for _ in 0..32 {
            let rect = control_rect(label)
                .ok_or_else(|| format!("source-tool widget {label:?} was not painted"))?;
            let clip = control_clip(label)
                .ok_or_else(|| format!("source-tool widget {label:?} has no draw clip"))?;
            let point = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            if point[0] < clip.min[0] || point[0] >= clip.max[0] {
                return Err(format!(
                    "source-tool widget {label:?} is horizontally clipped"
                ));
            }
            if point[1] >= clip.min[1] && point[1] < clip.max[1] && control_visible(label) {
                return self.click(label);
            }
            self.pointer = [
                (clip.min[0] + clip.max[0]) * 0.5,
                (clip.min[1] + clip.max[1]) * 0.5,
            ];
            self.paint(|_| {})?;
            let direction = if point[1] < clip.min[1] { 3.0 } else { -3.0 };
            self.paint(|context| context.io_mut().add_mouse_wheel_event([0.0, direction]))?;
            for _ in 0..2 {
                self.paint(|_| {})?;
            }
        }
        Err(format!("source-tool widget {label:?} remained clipped"))
    }

    fn require_source_tool_review(
        &self,
        request: &EditorSourceToolRequestId,
        health: i64,
    ) -> Result<(), String> {
        let frame = self.frame()?;
        let summary = frame
            .source_tool()
            .review
            .as_ref()
            .ok_or("the source-tool frame has no shared review observation")?;
        let review = self
            .application
            .source_tool_review()
            .map_err(|error| error.to_string())?
            .ok_or("the Session has no retained source-tool candidate")?;
        let exact_writes = |operations: &[ScenePatchOperation], expected_health| {
            operations.len() == 2
                && [("health", expected_health), ("armor", 10)]
                    .into_iter()
                    .all(|(expected_field, expected_value)| {
                        operations.iter().any(|operation| {
                            matches!(operation, ScenePatchOperation::SetField {
                                entity, component, field, value, ..
                            } if entity.as_str() == "enemy" && component.as_str() == STATS
                                && field.as_str() == expected_field
                                && value == &ComponentValue::I64(expected_value))
                        })
                    })
        };
        if &summary.request != request
            || review.request() != request
            || summary.operations != review.patch().operations.len()
            || summary.title.as_ref() != review.title()
            || !exact_writes(&review.patch().operations, health)
            || !exact_writes(&review.inverse().operations, 100)
        {
            return Err(
                "Inspector and panel did not retain the exact fixed health candidate".into(),
            );
        }
        Ok(())
    }

    fn run_source_tool(
        mut self,
        fixture: &SourceToolFixture,
    ) -> Result<SourceToolObservation, String> {
        self.open_source_tool_document()?;
        self.adopt_installed_source_tool_owner(fixture)?;
        let source = fixture.project.root.join(SCENE);
        let original_bytes = fs::read(&source).map_err(|error| error.to_string())?;
        let project_code =
            fs::read(fixture.project.root.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let frame = self.frame()?;
        let document = frame
            .workspace()
            .active_document
            .ok_or("Open has no active document")?;
        let basis = self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("{error:?}"))?;
        if self.selected_stats()? != (100, 10) || self.history()? != 0 || self.dirty()? {
            return Err("ordinary Open did not preserve clean initial Stats(100, 10)".into());
        }
        if self
            .application
            .source_tool_selection_view()
            .map_err(|error| error.to_string())?
            .desired
            .is_some()
            || fixture.project.root.join(SELECTION).exists()
        {
            return Err("ordinary Open unexpectedly created an installed selection".into());
        }
        let ready = self.enable_installed_source_tool(fixture)?;
        let enabled = ready.selection.as_ref().unwrap().desired.clone().unwrap();
        let selection_bytes =
            fs::read(fixture.project.root.join(SELECTION)).map_err(|error| error.to_string())?;
        let active = ready
            .active
            .as_ref()
            .ok_or("explicit Probe did not activate the actual child")?;
        let activation = active.id().clone();
        if active.owner() != fixture.owner.owner_id()
            || active.component().as_str() != STATS
            || active
                .integer_property()
                .is_none_or(|facet| facet.label() != "Health bonus")
            || control_rect(BONUS_INPUT).is_none()
            || control_rect(&format!("field:{STATS}:health")).is_some()
            || control_rect(&format!("field:{STATS}:armor")).is_none()
        {
            return Err(format!(
                "the health proposal facet did not replace only its admitted generic row: active={active:?}, parameter={:?}, health={:?}, armor={:?}",
                control_rect(BONUS_INPUT),
                control_rect(&format!("field:{STATS}:health")),
                control_rect(&format!("field:{STATS}:armor")),
            ));
        }
        let initial_history = self.history()?;
        let initial_health = self.selected_stats()?.0;
        self.replace_text(BONUS_INPUT, "25")?;
        self.click(PROPERTY_PREVIEW)?;
        let view = self.settle_actual_source_tool()?;
        let request = view
            .request
            .ok_or("the real Inspector Preview submitted no request")?;
        self.require_source_tool_review(&request, 125)?;
        if control_rect(PROPERTY_ACCEPT).is_none() {
            return Err(
                "the actual Inspector did not paint the Session review acceptance control".into(),
            );
        }
        let (patch, inverse) = {
            let review = self
                .application
                .source_tool_review()
                .map_err(|error| error.to_string())?
                .ok_or("the inline proposal has no candidate")?;
            (review.patch().clone(), review.inverse().clone())
        };
        let panel = self.open_source_tool_panel()?;
        self.require_source_tool_review(&request, 125)?;
        if self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("{error:?}"))?
            != basis
            || self.selected_stats()? != (100, 10)
            || self.history()? != initial_history
            || fs::read(&source).map_err(|error| error.to_string())? != original_bytes
        {
            return Err("preview or the second review observer mutated source/history/disk".into());
        }
        {
            let review = self
                .application
                .source_tool_review()
                .map_err(|error| error.to_string())?
                .ok_or("opening the panel lost the Session candidate")?;
            if review.patch() != &patch || review.inverse() != &inverse {
                return Err("the panel regenerated the prepared candidate".into());
            }
        }
        self.source_tool_control(PANEL_ACCEPT)?;
        if self.frame()?.source_tool().review.is_some()
            || self
                .application
                .source_tool_review()
                .map_err(|error| error.to_string())?
                .is_some()
            || self.selected_stats()? != (125, 10)
        {
            return Err("panel acceptance did not consume the Inspector's exact proposal".into());
        }
        let accepted_history = self.history()?;
        let accepted_health = self.selected_stats()?.0;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(panel, false);
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        self.click("Undo")?;
        let undo_history = self.history()?;
        let undo_health = self.selected_stats()?.0;
        self.click("Redo")?;
        let redo_history = self.history()?;
        let redo_health = self.selected_stats()?.0;
        if [
            initial_history,
            accepted_history,
            undo_history,
            redo_history,
        ] != [0, 1, 0, 1]
            || [initial_health, accepted_health, undo_health, redo_health] != [100, 125, 100, 125]
            || fs::read(&source).map_err(|error| error.to_string())? != original_bytes
        {
            return Err(
                "the actual accepted proposal was not one unsaved Undo/Redo transaction".into(),
            );
        }

        // A useful panel action uses the same explicit request/review/acceptance pipeline.
        self.click("entity:recruit")?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(panel, true);
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.source_tool_control(PANEL_ATTACH)?;
        let attach = self.settle_actual_source_tool()?;
        let attach_request = attach
            .request
            .ok_or("the panel Attach submitted no actual child request")?;
        if attach.review.as_ref().is_none_or(|review| {
            review.request != attach_request
                || !matches!(&review.intent, EditorSourceToolIntent::Attach { entity }
                if entity.as_str() == "recruit")
        }) || self.history()? != 1
        {
            return Err(
                "the panel Attach bypassed shared review or mutated before acceptance".into(),
            );
        }
        self.source_tool_control(PANEL_ACCEPT)?;
        if self.history()? != 2 || self.selected_stats()? != (100, 10) {
            return Err("the panel did not accept its Attach candidate exactly once".into());
        }
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(panel, false);
        for _ in 0..3 {
            self.paint(|_| {})?;
        }

        self.click("entity:enemy")?;
        self.replace_text(BONUS_INPUT, "999")?;
        if self.frame()?.source_tool().request.as_ref() != Some(&attach_request) {
            return Err("typing an unsubmitted parameter draft executed a child".into());
        }
        // Probe the same prebuilt DLL again: a new activation cannot inherit this old draft.
        let replacement = self.enable_installed_source_tool(fixture)?;
        let replacement_id = replacement
            .active
            .as_ref()
            .ok_or("the same-command replacement Probe did not activate")?
            .id()
            .clone();
        if replacement_id == activation
            || replacement.request.is_some()
            || replacement.review.is_some()
        {
            return Err("the replacement inherited its predecessor's request or activation".into());
        }
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, true))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, false))?;
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        let replaced = self.frame()?.source_tool().clone();
        if replaced.request.is_some()
            || replaced.has_pending_work()
            || self.selected_stats()? != (125, 10)
            || self.history()? != 2
        {
            return Err("an old parameter draft submitted after actual tool replacement".into());
        }
        self.replace_text(BONUS_INPUT, "333")?;
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(panel, true);
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.source_tool_control(PANEL_DISABLE)?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, true))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, false))?;
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        let disabled = self.settle_actual_source_tool()?;
        let selection = disabled
            .selection
            .as_ref()
            .ok_or("disabled selection is missing")?;
        let mut desired_disabled = enabled.clone();
        desired_disabled.enabled = false;
        if selection.desired.as_ref() != Some(&desired_disabled)
            || selection.active_origin.is_some()
            || selection.disable.as_ref().is_none_or(|disable| {
                disable.outcome != Some(EditorSourceToolSelectionOutcome::Disabled)
            })
            || selection.operation != replaced.selection.as_ref().unwrap().operation
            || selection.operation.as_ref().is_none_or(|operation| {
                operation.outcome != Some(EditorSourceToolSelectionOutcome::Enabled)
            })
            || fs::read(fixture.project.root.join(SELECTION)).map_err(|error| error.to_string())?
                == selection_bytes
        {
            return Err(format!(
                "native DisableInstalled did not persist: {disabled:?}"
            ));
        }
        let disable_observation = selection.disable.clone();
        self.application
            .panel_workspace_mut()
            .layout_mut()
            .set_visible(panel, false);
        for _ in 0..3 {
            self.paint(|_| {})?;
        }
        let after_disable = self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("{error:?}"))?;
        if disabled.active.is_some()
            || disabled.has_pending_work()
            || disabled.request.is_some()
            || disabled.review.is_some()
            || control_rect(BONUS_INPUT).is_some()
            || control_rect(&format!("field:{STATS}:health")).is_none()
            || control_rect(&format!("field:{STATS}:armor")).is_none()
            || after_disable
                .schema
                .as_ref()
                .and_then(|schema| schema.component_schema_owner(&ComponentTypeId::new(STATS)))
                != Some(fixture.owner.owner_id())
            || self.selected_stats()? != (125, 10)
            || self.history()? != 2
        {
            return Err(
                "disable replayed an old draft or failed to restore the admitted generic editor"
                    .into(),
            );
        }
        self.click("Save")?;
        self.settle_persistence()?;
        if !matches!(self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document)
            || self.dirty()?
        {
            return Err("the actual Save widget did not persist the tool-authored source".into());
        }
        let saved = read_scene(&source)?;
        // Leave an enabled durable request for fresh Open to observe without running a child.
        let reenabled = self.enable_installed_source_tool(fixture)?;
        if reenabled
            .selection
            .as_ref()
            .and_then(|selection| selection.desired.as_ref())
            != Some(&enabled)
            || reenabled.selection.as_ref().unwrap().disable != disable_observation
            || fs::read(fixture.project.root.join(SELECTION)).map_err(|error| error.to_string())?
                != selection_bytes
        {
            return Err("explicit re-enable did not restore the same installed selection".into());
        }
        if project_code
            != fs::read(fixture.project.root.join("src/lib.rs"))
                .map_err(|error| error.to_string())?
            || !String::from_utf8_lossy(&project_code).contains("compile_error!")
        {
            return Err("the prebuilt Editor workflow changed or replaced project code".into());
        }
        Ok(SourceToolObservation {
            presents: self.presents,
            history: [
                initial_history,
                accepted_history,
                undo_history,
                redo_history,
            ],
            health: [initial_health, accepted_health, undo_health, redo_health],
            saved,
            desired: enabled,
            selection_bytes,
        })
    }

    fn reopen_source_tool(
        mut self,
        fixture: &SourceToolFixture,
        observed: &SourceToolObservation,
    ) -> Result<u64, String> {
        self.open_source_tool_document()?;
        if self.selected_stats()? != (125, 10)
            || self.history()? != 0
            || self.dirty()?
            || control_rect(&format!("field:{STATS}:health")).is_none()
            || read_scene(&fixture.project.root.join(SCENE))? != observed.saved
        {
            return Err(
                "fresh Application Open did not read saved owner-tagged Stats without activation"
                    .into(),
            );
        }
        self.click("entity:recruit")?;
        if self.selected_stats()? != (100, 10) || self.history()? != 0 {
            return Err("fresh Open lost the panel-authored attached Stats".into());
        }
        let selection = self
            .application
            .source_tool_selection_view()
            .map_err(|error| error.to_string())?;
        if selection.desired.as_ref() != Some(&observed.desired)
            || selection.active_origin.is_some()
            || fs::read(fixture.project.root.join(SELECTION)).map_err(|error| error.to_string())?
                != observed.selection_bytes
        {
            return Err(
                "fresh Open did not observe the saved enabled request without activation".into(),
            );
        }
        let observed_operation = selection.operation.clone();
        self.open_source_tool_panel()?;
        self.source_tool_control(PANEL_DISABLE_SAVED)?;
        let disabled = self.settle_actual_source_tool()?;
        let selection = disabled
            .selection
            .as_ref()
            .ok_or("saved selection disappeared")?;
        let mut desired_disabled = observed.desired.clone();
        desired_disabled.enabled = false;
        if disabled.active.is_some()
            || disabled.attempt.is_some()
            || disabled.request.is_some()
            || disabled.review.is_some()
            || selection.desired.as_ref() != Some(&desired_disabled)
            || selection.active_origin.is_some()
            || selection.disable.as_ref().is_none_or(|disable| {
                disable.outcome != Some(EditorSourceToolSelectionOutcome::Disabled)
            })
            || selection.operation != observed_operation
            || selection.operation.as_ref().is_none_or(|operation| {
                operation.outcome != Some(EditorSourceToolSelectionOutcome::Observed)
            })
            || fs::read(fixture.project.root.join(SELECTION)).map_err(|error| error.to_string())?
                == observed.selection_bytes
            || read_scene(&fixture.project.root.join(SCENE))? != observed.saved
            || self.history()? != 0
            || self.dirty()?
        {
            return Err(
                "native DisableInstalled did not persist the unapproved saved request".into(),
            );
        }
        if !fixture
            .project
            .root
            .join(INSTALLATION)
            .join("EnemyStatsTool.dll")
            .is_file()
        {
            return Err("durable selection unexpectedly removed the installed DLL".into());
        }
        Ok(self.presents)
    }
}

fn stats(health: i64, armor: i64) -> ComponentValue {
    ComponentValue::map([
        ("wire_health", ComponentValue::I64(health)),
        ("wire_armor", ComponentValue::I64(armor)),
    ])
}

fn read_scene(path: &Path) -> Result<SceneDocument, String> {
    SceneDocumentCandidate::decode_json_bytes(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?
        .into_composition_source()
        .map_err(|error| error.to_string())
}

fn assert_saved_rust_data(
    saved: &SceneDocument,
    owner: &ComponentSchemaOwnerSnapshot,
) -> Result<(), String> {
    let mut registry = ComponentRegistry::new();
    SCENE_COMPONENTS_SCHEMA_PROVIDER
        .register_or_validate_into(&mut registry)
        .map_err(|error| error.to_string())?;
    registry.freeze().map_err(|error| error.to_string())?;
    let bindings = SceneComponentBindings::new(
        registry.snapshot().map_err(|error| error.to_string())?,
        [owner.clone()],
    )
    .map_err(|error| error.to_string())?;
    let expanded = bindings.expand_prefabs(saved, &InMemoryPrefabSourceResolver::new());
    if expanded.diagnostics.has_errors() {
        return Err(format!("{:?}", expanded.diagnostics));
    }
    let source = SourceDocumentKey::named(
        "f1390a84-11df-49b0-a901-d7a045141039"
            .parse()
            .map_err(|error| format!("{error}"))?,
        "native-source-tool-saved",
        SourceDocumentKind::Scene,
    )
    .map_err(|error| error.to_string())?;
    let candidate = expanded
        .candidate
        .ok_or("saved source produced no expanded candidate")?
        .bind_source(source);
    let mut world = nara::ecs::World::new();
    let prepared = bindings
        .prepare_scene(
            &mut world,
            &candidate,
            &SceneOccurrenceKey::new("native-source-tool", "saved")
                .map_err(|error| error.to_string())?,
            None,
        )
        .map_err(|report| format!("{:?}", report.diagnostics))?;
    let report = prepared.commit(&mut world);
    if report.diagnostics.has_errors() {
        return Err(format!("{:?}", report.diagnostics));
    }
    let instance = report
        .instance
        .ok_or("saved source produced no Scene instance")?;
    let observed = (|| {
        for (id, health) in [("enemy", 125), ("recruit", 100)] {
            let entity = match instance.resolve(
                &world,
                &SceneEntityId::new(id).map_err(|error| error.to_string())?,
            ) {
                nara::identity::EntityLookup::Resolved(entity) => entity,
                _ => return Err(format!("saved Rust Scene has no live {id}")),
            };
            let record = bindings
                .data_component(&world, entity, &ComponentTypeId::new(STATS))
                .map_err(|error| error.to_string())?
                .ok_or("saved Rust Scene has no Stats")?;
            if record.owner() != Some(owner.owner_id()) || record.value != stats(health, 10) {
                return Err(format!(
                    "public Rust Scene bindings read unexpected {id} Stats: {record:?}"
                ));
            }
        }
        Ok(())
    })();
    let retired = retire_and_despawn_scene_instance(&mut world, &instance)
        .map(|_| ())
        .map_err(|diagnostics| format!("{diagnostics:?}"));
    observed.and(retired)
}
