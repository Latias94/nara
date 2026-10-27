//! Actual independently built C# source proposals through the native Editor widgets.

use nara::{
    project_host::{
        EditorSourceToolCommand, EditorSourceToolDefinition, advanced::built_in_schema_providers,
    },
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentRegistry,
        ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
        ComponentTypeId, PersistentComponentSchemaOwnerId,
    },
    scene::{
        InMemoryPrefabSourceResolver, SCENE_COMPONENTS_SCHEMA_PROVIDER, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneDocumentCandidate, SceneEntityId,
        SceneEntityRecord, SceneOccurrenceKey, ScenePatchOperation, SourceDocumentKey,
        SourceDocumentKind, retire_and_despawn_scene_instance,
    },
    tooling::{EditorSourceToolIntent, EditorSourceToolRequestId, EditorSourceToolView},
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

struct SourceToolFixture {
    project: InteractionProject,
    owner: ComponentSchemaOwnerSnapshot,
    definition: EditorSourceToolDefinition,
    original: SceneDocument,
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
        let builtins =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .map_err(|error| error.to_string())?;
        let bundle = AuthoringSchemaBundle::new(
            builtins
                .owners()
                .iter()
                .map(AuthoringSchemaBundleOwner::from)
                .chain([AuthoringSchemaBundleOwner::from(&owner)]),
        )
        .map_err(|error| error.to_string())?;
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
        let command = EditorSourceToolCommand::new(
            executable,
            vec![directory.join("EnemyStatsTool.dll").into_os_string()],
            directory,
            TOOL_TIMEOUT,
        )
        .map_err(|error| error.to_string())?;
        let definition =
            EditorSourceToolDefinition::new(command, owner.clone(), ComponentTypeId::new(STATS))
                .map_err(|error| error.to_string())?;
        Ok(Self {
            project,
            owner,
            definition,
            original,
        })
    }
}

struct SourceToolObservation {
    presents: u64,
    history: [usize; 4],
    health: [i64; 4],
    saved: SceneDocument,
}

struct SourceToolApplication<'a> {
    fixture: &'a SourceToolFixture,
    result: Option<Result<(SourceToolObservation, u64), String>>,
}

#[test]
#[ignore = "requires Windows DX12 and the actual prebuilt EnemyStats tool environment"]
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
                        workflow.reopen_source_tool(self.fixture, &observed.saved)
                    })?;
                Ok((observed, presents))
            })());
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
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
        let view = self.frame()?.source_tool().clone();
        if view != EditorSourceToolView::default() {
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
            if !view.has_pending_work() && view == presented {
                if view.attempt_error.is_some() || view.request_error.is_some() {
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
        self.application
            .enable_source_tool(fixture.definition.clone())
            .map_err(|error| error.to_string())?;
        let ready = self.settle_actual_source_tool()?;
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
        self.application
            .enable_source_tool(fixture.definition.clone())
            .map_err(|error| error.to_string())?;
        let replacement = self.settle_actual_source_tool()?;
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
            .disable_source_tool(&replacement_id)
            .map_err(|error| error.to_string())?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, true))?;
        self.paint(|context| context.io_mut().add_key_event(Key::Enter, false))?;
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        let disabled = self.frame()?.source_tool().clone();
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
        })
    }

    fn reopen_source_tool(
        mut self,
        fixture: &SourceToolFixture,
        saved: &SceneDocument,
    ) -> Result<u64, String> {
        self.open_source_tool_document()?;
        if self.selected_stats()? != (125, 10)
            || self.history()? != 0
            || self.dirty()?
            || control_rect(&format!("field:{STATS}:health")).is_none()
            || read_scene(&fixture.project.root.join(SCENE))? != *saved
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
