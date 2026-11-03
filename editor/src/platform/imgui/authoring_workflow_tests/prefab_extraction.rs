use nara::{
    asset::AssetRef,
    project_host::{EditorPrefabExtractionOutcome, EditorPrefabExtractionPhase},
    reflect::{ComponentSchemaVersion, ComponentTypeId, PersistentComponentSchemaOwnerId},
    scene::{
        PrefabDocument, PrefabEntityTarget, PrefabInstance, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePlacement,
        ScenePlacementParent,
    },
    tooling::{
        EditorAction, EditorActionBatch, EditorNavigationAction, EditorPrefabExtractionDisposition,
    },
};

use super::*;

const PREFAB: &str = "blueprints/ordinary.prefab.json";
const EXTRACTED: &str = "blueprints/extracted.prefab.json";

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 adapter"]
fn native_prefab_extraction_reviews_cancels_and_publishes_exact_outputs() {
    let project = InteractionProject::new();
    configure(&project.root);
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = ExtractionApplication {
        project: project.root.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    let presents = application
        .result
        .expect("the native extraction trajectory must execute")
        .unwrap();
    assert!(presents > 0);
}

fn entity(name: &str) -> SceneEntityRecord {
    SceneEntityRecord::new(SceneEntityId::new(name).unwrap())
}

fn configure(root: &Path) {
    let branch = entity("branch").with_component(
        ComponentTypeId::new(CUSTOM_COMPONENT),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            ComponentValue::map([("value", ComponentValue::I64(10))]),
        )
        .with_owner(
            PersistentComponentSchemaOwnerId::try_new("studio.interaction.components").unwrap(),
        ),
    );
    let prefab = PrefabDocument::new(
        [
            SceneEntityPlacement::root(SceneEntityId::new("branch").unwrap()),
            SceneEntityPlacement::new(
                PrefabEntityTarget::source(SceneEntityId::new("child").unwrap()),
                ScenePlacement::last(ScenePlacementParent::Entity(PrefabEntityTarget::source(
                    SceneEntityId::new("branch").unwrap(),
                ))),
            ),
            SceneEntityPlacement::root(SceneEntityId::new("outside").unwrap()),
        ],
        [branch, entity("child"), entity("outside")],
    );
    fs::write(root.join(PREFAB), prefab.to_json_string().unwrap()).unwrap();
    let instances = ["prefab-anchor", "repeated"].map(|name| {
        let mut instance = entity(name);
        instance.prefab = Some(PrefabInstance::forest(
            AssetRef::path("ordinary.prefab.json").unwrap(),
        ));
        instance
    });
    fs::write(
        root.join("worlds/entry.scene.json"),
        SceneDocument::from_ordered_roots(instances)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

struct ExtractionApplication {
    project: PathBuf,
    result: Option<Result<u64, String>>,
}

impl ApplicationHandler for ExtractionApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_prefab_extraction(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl AuthoringWorkflow<'_> {
    fn extraction_phase(&mut self, phase: EditorPrefabExtractionPhase) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            self.paint(|_| {})?;
            let view = self
                .application
                .prefab_extraction_view()
                .map_err(|error| error.to_string())?;
            if view.phase == phase {
                return Ok(());
            }
            if Instant::now() >= deadline
                || matches!(
                    view.phase,
                    EditorPrefabExtractionPhase::RequiresReconcile
                        | EditorPrefabExtractionPhase::AwaitingSelection
                        | EditorPrefabExtractionPhase::Terminal
                )
            {
                return Err(format!(
                    "native extraction did not reach {phase:?}: {view:?}; {:?}",
                    self.application.prefab_extraction_diagnostics(),
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn run_prefab_extraction(mut self, project: &Path) -> Result<u64, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.application.current_frame().is_none() {
            self.paint(|_| {})?;
            if Instant::now() >= deadline {
                return Err("the native extraction project did not open".into());
            }
        }
        for _ in 0..5 {
            self.paint(|_| {})?;
        }
        self.click("entity:prefab-anchor")?;
        self.click("Open Prefab Source")?;
        loop {
            self.paint(|_| {})?;
            if self
                .frame()?
                .workspace()
                .active_scene
                .as_ref()
                .is_some_and(|scene| scene.kind == EditorDocumentKind::Prefab)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("the native Open Prefab Source did not finish".into());
            }
        }
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.click("entity:branch")?;
        let original = fs::read(project.join(PREFAB)).map_err(|error| error.to_string())?;
        let initial = self.frame()?;
        let source_document = initial
            .workspace()
            .active_document
            .ok_or("missing Prefab tab")?;
        let scene_document = initial
            .workspace()
            .documents
            .iter()
            .find(|document| document.kind == EditorDocumentKind::Scene)
            .ok_or("missing Scene tab")?
            .document;
        for cancel in [true, false] {
            self.click("Extract Prefab Branch...")?;
            if cancel {
                let frame = self.frame()?;
                self.application.submit_and_drive(
                    EditorActionBatch::new(
                        frame.revision(),
                        [EditorAction::Navigate(
                            EditorNavigationAction::SetActiveDocument {
                                document: scene_document,
                            },
                        )],
                    ),
                    Duration::ZERO,
                );
                self.paint(|_| {})?;
            }
            self.replace_text("Extraction Prefab alias", "extracted.prefab.json")?;
            self.click("Prepare extraction review")?;
            self.extraction_phase(EditorPrefabExtractionPhase::Review)?;
            let operation = self
                .application
                .prefab_extraction_view()
                .unwrap()
                .operation
                .unwrap();
            let outputs = self
                .application
                .prefab_extraction_review(operation)
                .map_err(|error| error.to_string())?
                .outputs()
                .map(|output| (output.project_path().to_owned(), output.bytes().to_vec()))
                .collect::<Vec<_>>();
            if !self
                .application
                .prefab_extraction_review(operation)
                .map_err(|error| error.to_string())?
                .documents()
                .any(|(document, disposition)| {
                    document == source_document
                        && disposition == EditorPrefabExtractionDisposition::ReplaceSource
                })
            {
                return Err("changing tabs retargeted the original extraction draft".into());
            }
            for _ in 0..3 {
                self.paint(|_| {})?;
            }
            if fs::read(project.join(PREFAB)).map_err(|error| error.to_string())? != original
                || project.join(EXTRACTED).exists()
                || self.dirty()?
            {
                return Err("painting the extraction review published or edited source".into());
            }
            if cancel {
                self.click(&format!("Extraction save:{PREFAB}"))?;
                self.click("Update extraction save selection")?;
                self.extraction_phase(EditorPrefabExtractionPhase::AwaitingSelection)?;
                self.paint(|_| {})?;
                self.click(&format!("Extraction save:{PREFAB}"))?;
                self.click("Update extraction save selection")?;
                self.extraction_phase(EditorPrefabExtractionPhase::Review)?;
                self.paint(|_| {})?;
                self.click("Cancel extraction")?;
            } else {
                self.click("Confirm reviewed extraction")?;
            }
            self.extraction_phase(EditorPrefabExtractionPhase::Terminal)?;
            let outcome = self.application.prefab_extraction_view().unwrap().outcome;
            let expected = if cancel {
                EditorPrefabExtractionOutcome::Cancelled
            } else {
                EditorPrefabExtractionOutcome::Adopted
            };
            if outcome != Some(expected) {
                return Err(format!("unexpected native extraction result: {outcome:?}"));
            }
            self.paint(|_| {})?;
            self.click("Dismiss extraction result")?;
            if cancel {
                if fs::read(project.join(PREFAB)).map_err(|error| error.to_string())? != original
                    || project.join(EXTRACTED).exists()
                {
                    return Err("cancelled native extraction changed disk".into());
                }
                let frame = self.frame()?;
                self.application.submit_and_drive(
                    EditorActionBatch::new(
                        frame.revision(),
                        [EditorAction::Navigate(
                            EditorNavigationAction::SetActiveDocument {
                                document: source_document,
                            },
                        )],
                    ),
                    Duration::ZERO,
                );
                self.paint(|_| {})?;
            } else {
                if !outputs.iter().any(|(path, _)| path == EXTRACTED) || self.dirty()? {
                    return Err("adopted extraction did not save the original Workspace".into());
                }
                for (path, bytes) in outputs {
                    if fs::read(project.join(&path)).map_err(|error| error.to_string())? != bytes {
                        return Err(format!("native publication differs from Review: {path}"));
                    }
                }
                self.click("Reopen")?;
                self.settle_persistence()?;
                if !matches!(
                    self.frame()?.persistence().result(),
                    Some(EditorPersistenceResult::Opened { .. })
                ) {
                    return Err(
                        "native extraction output did not reopen through persistence".into(),
                    );
                }
            }
        }
        Ok(self.presents)
    }
}
