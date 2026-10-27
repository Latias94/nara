use std::collections::BTreeMap;

use nara::{
    asset::AssetRef,
    render_wgpu::__private::WgpuSubmittedImageReceipt,
    scene::{
        PrefabDeclarationLimits, PrefabDocument, PrefabInstance, PrefabInstancePath, SceneDocument,
        SceneEntityId, SceneEntityRecord,
    },
    sprite_render::__private::{SpriteInstance, SpriteMaterialKey},
    tooling::{
        EditorDocumentId, EditorPrefabConversionPhase, EditorPrefabConversionResult,
        EditorPrefabConversionView,
    },
};

use super::*;
use crate::frontend::imgui::{UiProject, control_clip, control_visible};

const ANCHOR: &str = "a-convert";
const CHILD: &str = "ordinary-child";
const OTHER: &str = "z-other";
const SCENE: &str = "scenes/startup.scene.json";
const PREFAB: &str = "prefabs/ordinary.prefab.json";
const TRANSFORM: &str = "nara.transform.Transform2d";

#[test]
#[ignore = "requires an interactive Windows desktop and a working DX12 adapter"]
fn native_complete_prefab_conversion_reviews_commits_and_round_trips_scene_and_prefab() {
    let project = UiProject::new();
    configure(&project.0).unwrap();
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    let event_loop = builder.build().unwrap();
    let mut application = ConversionApplication {
        project: project.0.clone(),
        result: None,
    };
    event_loop.run_app(&mut application).unwrap();
    // Assertions that can panic run after the real graphics and Session owners have retired.
    let (documents, presents) = application
        .result
        .expect("the complete-conversion native workflow must execute")
        .unwrap();
    assert_eq!(
        documents,
        [EditorDocumentKind::Scene, EditorDocumentKind::Prefab]
    );
    assert!(presents > 0);
}

struct ConversionApplication {
    project: PathBuf,
    result: Option<Result<(Vec<EditorDocumentKind>, u64), String>>,
}

impl ApplicationHandler for ConversionApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run_native_workflow(event_loop, &self.project, |workflow| {
                workflow.run_prefab_conversion(&self.project)
            }));
        }
        event_loop.exit();
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn id(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn linked(entity: &str, source: &str) -> SceneEntityRecord {
    let mut record = SceneEntityRecord::new(id(entity));
    record.prefab = Some(PrefabInstance {
        source: AssetRef::path(source).unwrap(),
        overrides: None,
    });
    record
}

fn configure(project: &Path) -> Result<(), String> {
    let startup = read_source(&project.join(SCENE))?;
    let mut sprite: SceneEntityRecord =
        serde_json::from_value(record(&startup, "image-source")?.clone())
            .map_err(|error| error.to_string())?;
    sprite.id = id("source-sprite");
    sprite.children = vec![id("source-child")];
    let mut leaf = sprite.clone();
    leaf.id = id("leaf-sprite");
    leaf.children.clear();
    for (record, x) in [(&mut sprite, -32.0), (&mut leaf, 32.0)] {
        let transform = record
            .components
            .get_mut(&nara::reflect::ComponentTypeId::new(TRANSFORM))
            .ok_or("the existing image fixture has no Transform")?;
        transform.value = ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(x).unwrap()),
                    ("y", ComponentValue::f64(0.0).unwrap()),
                ]),
            ),
            ("rotation", ComponentValue::f64(0.0).unwrap()),
            (
                "scale",
                ComponentValue::map([
                    ("x", ComponentValue::f64(1.0).unwrap()),
                    ("y", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ]);
    }
    let transform_type = nara::reflect::ComponentTypeId::new(TRANSFORM);
    let preserved_transform = sprite.components[&transform_type].clone();
    let assembly = PrefabDocument::new(
        [id("source-sprite"), id("nested-source")],
        [
            sprite,
            SceneEntityRecord::new(id("source-child")),
            linked("nested-source", "leaf.prefab.json"),
        ],
    );
    let anchor = linked(ANCHOR, "assembly.prefab.json")
        .with_children([id(CHILD)])
        .with_component(transform_type.clone(), preserved_transform.clone());
    let local =
        SceneEntityRecord::new(id(CHILD)).with_component(transform_type, preserved_transform);
    let prefab = PrefabDocument::new(
        [id(ANCHOR), id(OTHER)],
        [
            anchor.clone(),
            local.clone(),
            linked(OTHER, "assembly.prefab.json"),
        ],
    );
    let scene = SceneDocument::new(
        [id(ANCHOR), id(OTHER)],
        [anchor, local, linked(OTHER, "ordinary.prefab.json")],
    );
    for (path, text) in [
        (SCENE, scene.to_json_string()),
        (PREFAB, prefab.to_json_string()),
        ("prefabs/assembly.prefab.json", assembly.to_json_string()),
        (
            "prefabs/leaf.prefab.json",
            PrefabDocument::from_ordered_roots([leaf]).to_json_string(),
        ),
    ] {
        fs::write(project.join(path), text.map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn record<'a>(
    document: &'a serde_json::Value,
    entity: &str,
) -> Result<&'a serde_json::Value, String> {
    document["payload"]["entities"]
        .as_array()
        .and_then(|entities| {
            entities
                .iter()
                .find(|record| record["id"].as_str() == Some(entity))
        })
        .ok_or_else(|| format!("source has no record {entity:?}"))
}

#[derive(Debug, PartialEq)]
struct RenderedSpriteInput {
    instance: SpriteInstance,
    material: SpriteMaterialKey,
    pixels: Vec<u8>,
}

type Hierarchy = Vec<(String, Option<String>, bool)>;

impl AuthoringWorkflow<'_> {
    fn conversion_control(&mut self, label: &str) -> Result<(), String> {
        // Use the actual recorded draw clip and owned Context wheel input, as in Canvas tests.
        for _ in 0..32 {
            let rect = control_rect(label)
                .ok_or_else(|| format!("conversion widget {label:?} was not painted"))?;
            let clip = control_clip(label)
                .ok_or_else(|| format!("conversion widget {label:?} has no draw clip"))?;
            let point = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            if point[0] < clip.min[0] || point[0] >= clip.max[0] {
                return Err(format!(
                    "conversion widget {label:?} is horizontally clipped"
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
            self.paint(|_| {})?;
            self.paint(|_| {})?;
        }
        Err(format!("conversion widget {label:?} remained clipped"))
    }

    fn conversion_hierarchy(&self) -> Result<Hierarchy, String> {
        let frame = self.frame()?;
        let scene = frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the conversion source has no hierarchy")?;
        Ok(scene
            .editor
            .inspector
            .entities
            .iter()
            .map(|row| {
                (
                    row.id.as_str().to_owned(),
                    row.parent.as_ref().map(|parent| parent.as_str().to_owned()),
                    row.has_prefab,
                )
            })
            .collect())
    }

    fn wait_conversion(
        &mut self,
        phase: EditorPrefabConversionPhase,
    ) -> Result<EditorPrefabConversionView, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.paint(|_| {})?;
            let frame = self.frame()?;
            let view = frame.prefab_conversion();
            if view.phase == phase {
                return Ok(view.clone());
            }
            if view.result.is_some() || Instant::now() >= deadline {
                return Err(format!("expected conversion {phase:?}, observed {view:?}"));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn conversion_presented(&mut self) -> Result<WgpuSubmittedImageReceipt, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.paint(|_| {})?;
            let frame = self.frame()?;
            if self.native.scene_presenter.is_presented(
                &self.native.scene,
                &frame,
                self.native.scene_extent,
                self.application
                    .current_scene_preview_content_stamp()
                    .as_ref(),
            ) && let Some(receipt) = self.native.scene.submitted
            {
                return Ok(receipt);
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "conversion has no current Scene composite: {:?}",
                    frame.diagnostics()
                ));
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn conversion_render_inputs(&mut self) -> Result<Vec<RenderedSpriteInput>, String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.application
                .drive_editor_frame(Duration::ZERO, false, false);
            if let Some(candidate) = self.application.take_scene_preview_candidate() {
                let content = candidate.content_stamp();
                let frame = self.frame()?;
                if frame.scene_preview_document() != Some(candidate.document())
                    || !candidate.preview_omissions().is_empty()
                {
                    return Err("conversion observed another source or omitted preview data".into());
                }
                let (_, _, _, _, batches, _) = candidate.into_parts();
                let mut inputs = Vec::new();
                for batch in batches.as_slice() {
                    let image = batch
                        .image
                        .as_ref()
                        .ok_or("the conversion fixture lost its actual prepared image")?;
                    if image.resource().pixels() != [24, 120, 220, 255] {
                        return Err("the actual image fixture changed its blue texel".into());
                    }
                    inputs.extend(batch.instances.iter().map(|instance| RenderedSpriteInput {
                        instance: *instance,
                        material: batch.material,
                        pixels: image.resource().pixels().to_vec(),
                    }));
                }
                if inputs.len() != 6 {
                    return Err(format!(
                        "expected six rendered Sprites, observed {}",
                        inputs.len()
                    ));
                }
                inputs.sort_by(|left, right| {
                    left.instance
                        .center
                        .x
                        .total_cmp(&right.instance.center.x)
                        .then(left.instance.center.y.total_cmp(&right.instance.center.y))
                });
                // Observe a real owner-produced payload, then let the next ordinary native drive
                // produce and present it. No candidate, terminal result or source state is injected.
                self.conversion_presented()?;
                if self
                    .application
                    .current_scene_preview_content_stamp()
                    .as_ref()
                    != Some(&content)
                {
                    if Instant::now() >= deadline {
                        return Err("render-input observation never settled on the presented source content".into());
                    }
                    continue;
                }
                return Ok(inputs);
            }
            if Instant::now() >= deadline {
                return Err("conversion produced no ordinary render payload".into());
            }
            self.paint(|_| {})?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn expect_conversion_link(
        &self,
        document: EditorDocumentId,
        expected: Option<&PrefabInstance>,
    ) -> Result<(), String> {
        let basis = self
            .application
            .document_query_basis(document)
            .map_err(|error| format!("conversion basis: {error:?}"))?;
        let observed = self
            .application
            .observe_prefab_instance(
                &basis,
                &PrefabInstancePath::new(id(ANCHOR)),
                PrefabDeclarationLimits::default(),
            )
            .map_err(|error| format!("conversion declaration: {error:?}"))?;
        if observed.instance.as_ref() != expected {
            return Err(format!(
                "unexpected stored anchor declaration: {:?}",
                observed.instance
            ));
        }
        Ok(())
    }

    fn conversion_document(
        &mut self,
        project: &Path,
        kind: EditorDocumentKind,
    ) -> Result<(), String> {
        let source = project.join(match kind {
            EditorDocumentKind::Scene => SCENE,
            EditorDocumentKind::Prefab => PREFAB,
            EditorDocumentKind::Extension(_) => unreachable!("native fixture source kind"),
        });
        let original = read_source(&source)?;
        let original_link: PrefabInstance =
            serde_json::from_value(record(&original, ANCHOR)?["prefab"].clone())
                .map_err(|error| error.to_string())?;
        let before_frame = self.frame()?;
        let active = before_frame
            .workspace()
            .active_scene
            .as_ref()
            .ok_or("the conversion source is not active")?;
        if active.kind != kind || self.dirty()? || self.history()? != 0 {
            return Err(format!(
                "the {kind:?} conversion needs a clean source baseline"
            ));
        }
        let document = active.document;
        self.conversion_control(&format!("entity:{ANCHOR}"))?;
        let before_tree = self.conversion_hierarchy()?;
        let before_pixels = self.conversion_render_inputs()?;
        self.conversion_control("Inspector tab: Prefab Intent")?;
        self.conversion_control("Prefab Intent: Refresh declarations")?;
        self.conversion_control("Prefab Intent: Convert Entire Instance to Local")?;
        let review_view = self.wait_conversion(EditorPrefabConversionPhase::Review)?;
        let operation = review_view
            .operation
            .ok_or("review has no operation identity")?;
        let review = review_view
            .review
            .as_ref()
            .ok_or("conversion has no reviewed mapping")?;
        if review_view.document != Some(document)
            || review_view.anchor.as_deref() != Some(ANCHOR)
            || review.total_entities != 4
            || review.rows.len() != 4
            || review.next_offset.is_some()
            || review.source_count != 2
            || review.working_copy_count != 0
        {
            return Err(format!(
                "unexpected native conversion review: {review_view:?}"
            ));
        }
        for row in &review.rows {
            self.conversion_control(&format!(
                "Prefab Intent conversion mapping: {} -> {}",
                row.origin, row.entity,
            ))?;
        }
        let displayed = self.conversion_presented()?;
        for _ in 0..4 {
            self.paint(|_| {})?;
            let frame = self.frame()?;
            if frame.prefab_conversion() != &review_view
                || self.conversion_hierarchy()? != before_tree
                || self.history()? != 0
                || self.dirty()?
                || self.native.scene.submitted != Some(displayed)
            {
                return Err(
                    "repainting Review changed the candidate, source or rendered receipt".into(),
                );
            }
        }
        if read_source(&source)? != original {
            return Err("preparation wrote the source file before Commit".into());
        }
        self.conversion_control("Prefab Intent: Commit reviewed conversion")?;
        let terminal = self.wait_conversion(EditorPrefabConversionPhase::Terminal)?;
        let Some(EditorPrefabConversionResult::Committed {
            operation: completed,
            receipt,
        }) = terminal.result.as_ref()
        else {
            return Err(format!("native conversion did not commit: {terminal:?}"));
        };
        if *completed != operation
            || terminal.operation != Some(operation)
            || receipt.entities().len() != review.rows.len()
            || receipt
                .entities()
                .iter()
                .zip(&review.rows)
                .any(|(actual, reviewed)| {
                    actual.entity() != &reviewed.entity
                        || expected_review_origin(actual.provenance()).as_deref()
                            != Some(reviewed.origin.as_ref())
                })
            || control_rect("Prefab Intent conversion result: Committed").is_none()
        {
            return Err("terminal conversion did not report the exact painted review".into());
        }
        let mut mapping = BTreeMap::new();
        for row in receipt.entities() {
            let path = row
                .provenance()
                .projection_path()
                .ok_or("a converted entity lost its projected origin")?;
            if path.root_anchor().as_str() != ANCHOR
                || mapping
                    .insert(
                        row.provenance().local_entity_id().as_str().to_owned(),
                        row.entity().as_str().to_owned(),
                    )
                    .is_some()
            {
                return Err(
                    "conversion mapping has an incorrect owner or duplicate source ID".into(),
                );
            }
        }
        let local = |name: &str| {
            mapping
                .get(name)
                .cloned()
                .ok_or_else(|| format!("review has no localized {name}"))
        };
        let sprite = local("source-sprite")?;
        let child = local("source-child")?;
        let nested = local("nested-source")?;
        let leaf = local("leaf-sprite")?;
        let expected_tree = vec![
            (ANCHOR.to_owned(), None, false),
            (sprite.clone(), Some(ANCHOR.to_owned()), false),
            (child, Some(sprite.clone()), false),
            (nested.clone(), Some(ANCHOR.to_owned()), false),
            (leaf, Some(nested.clone()), false),
            (CHILD.to_owned(), Some(ANCHOR.to_owned()), false),
            (OTHER.to_owned(), None, true),
        ];
        if self.conversion_hierarchy()? != expected_tree
            || self.history()? != 1
            || !self.dirty()?
            || read_source(&source)? != original
            || self
                .frame()?
                .workspace()
                .active_scene
                .as_ref()
                .and_then(|scene| scene.selection.top_entity())
                .map(SceneEntityId::as_str)
                != Some(ANCHOR)
        {
            return Err(
                "complete conversion changed hierarchy, history or disk outside its scope".into(),
            );
        }
        self.expect_conversion_link(document, None)?;
        let committed_display = self.conversion_presented()?;
        if committed_display == displayed || self.conversion_render_inputs()? != before_pixels {
            return Err(
                "conversion retained a stale receipt or changed actual sprite/image inputs".into(),
            );
        }
        self.conversion_control("Prefab Intent: Acknowledge conversion result")?;
        if self.frame()?.prefab_conversion().operation.is_some() {
            return Err("the exact conversion result was not acknowledged".into());
        }
        self.conversion_control("Undo")?;
        self.expect_conversion_link(document, Some(&original_link))?;
        if self.conversion_hierarchy()? != before_tree
            || self.history()? != 0
            || self.dirty()?
            || self.conversion_render_inputs()? != before_pixels
        {
            return Err("Undo did not restore the exact linked source and rendered inputs".into());
        }
        self.conversion_control("Redo")?;
        self.expect_conversion_link(document, None)?;
        if self.conversion_hierarchy()? != expected_tree
            || self.history()? != 1
            || !self.dirty()?
            || self.conversion_render_inputs()? != before_pixels
        {
            return Err(
                "Redo did not restore the reviewed IDs, hierarchy and rendered inputs".into(),
            );
        }
        self.conversion_control("Save")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Saved { document: actual, .. }) if actual == document
        ) || self.dirty()?
        {
            return Err("the actual converted source did not Save cleanly".into());
        }
        let saved = read_source(&source)?;
        if saved["payload"]["roots"] != original["payload"]["roots"]
            || record(&saved, CHILD)? != record(&original, CHILD)?
            || record(&saved, OTHER)? != record(&original, OTHER)?
            || record(&saved, ANCHOR)?["components"] != record(&original, ANCHOR)?["components"]
            || record(&saved, ANCHOR)?["children"] != serde_json::json!([sprite, nested, CHILD])
            || record(&saved, ANCHOR)?.get("prefab").is_some()
        {
            return Err(
                "Save changed preserved local records or serialized an incomplete conversion"
                    .into(),
            );
        }
        for entity in mapping.values() {
            if record(&saved, entity)?.get("prefab").is_some() {
                return Err("complete conversion persisted a nested Prefab link".into());
            }
        }
        self.conversion_control("Dismiss persistence result")?;
        self.conversion_control("Reopen")?;
        self.settle_persistence()?;
        if !matches!(
            self.frame()?.persistence().result(),
            Some(EditorPersistenceResult::Opened { .. })
        ) || self.conversion_hierarchy()? != expected_tree
            || self.history()? != 0
            || self.dirty()?
            || read_source(&source)? != saved
            || self.conversion_render_inputs()? != before_pixels
        {
            return Err(
                "actual Reopen did not retain clean converted source, hierarchy and rendering"
                    .into(),
            );
        }
        self.conversion_control("Dismiss persistence result")?;
        Ok(())
    }

    fn run_prefab_conversion(
        mut self,
        project: &Path,
    ) -> Result<(Vec<EditorDocumentKind>, u64), String> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.paint(|_| {})?;
            if self.application.current_frame().is_some() {
                let game = self
                    .application
                    .panel_workspace()
                    .layout()
                    .instances()
                    .iter()
                    .find(|panel| panel.kind() == "nara.game-view" && panel.is_visible())
                    .map(|panel| panel.id());
                if let Some(game) = game {
                    self.application
                        .panel_workspace_mut()
                        .layout_mut()
                        .close(game);
                    self.native.frontend.reset_layout();
                } else {
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err("the actual native project Open did not finish".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        for _ in 0..4 {
            self.paint(|_| {})?;
        }
        self.conversion_document(project, EditorDocumentKind::Scene)?;
        self.conversion_control("Inspector tab: Document")?;
        self.conversion_control(&format!("entity:{OTHER}"))?;
        self.conversion_control("Open Prefab Source")?;
        self.conversion_document(project, EditorDocumentKind::Prefab)?;
        Ok((
            vec![EditorDocumentKind::Scene, EditorDocumentKind::Prefab],
            self.presents,
        ))
    }
}

fn expected_review_origin(provenance: &nara::scene::SceneEntityProvenance) -> Option<String> {
    let path = provenance.projection_path()?;
    let mut segments = vec![path.root_anchor().to_string()];
    for step in path.steps() {
        match step {
            nara::scene::PrefabProjectionStep::SourceEntity {
                canonical_source,
                entity,
                ..
            } => {
                let nara::scene::SourceDocumentOrigin::ProjectLocation { path, .. } =
                    canonical_source.origin()
                else {
                    return None;
                };
                segments.push(format!("{path}:{entity}"));
            }
            nara::scene::PrefabProjectionStep::InstanceAddition { entity } => {
                segments.push(format!("added:{entity}"));
            }
        }
    }
    Some(segments.join(" / "))
}
