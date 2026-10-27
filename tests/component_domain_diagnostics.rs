#![cfg(all(
    feature = "project-workflow",
    feature = "runtime-2d",
    feature = "serde",
    feature = "tooling"
))]

#[path = "support/editor_action_driver.rs"]
mod editor_action_driver;
#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::fs;

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::Component,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    diagnostic::{DiagnosticCode, DiagnosticReport, DiagnosticValueRef, SafeSummary},
    project_host::{EditorProjectSession, TrustedCompiledEditorProjectIntent},
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        AuthoringSchemaBundle, AuthoringSchemaBundleOwner, ComponentCapability,
        ComponentDomainError, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerId,
        ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
        ComponentValueShape, PersistentComponentSchemaOwnerId,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityRecord, ScenePatchDocument,
        ScenePatchOperation,
    },
    tooling::{EditorAction, EditorActionOutcome, EditorDocumentMutation},
};
use project_content_fixture::{TestProject, scene_id};

const CANARY: &str = "qvM7k2p8Nx4J";
const COMPONENT: &str = "demo.DomainCooldown";

#[derive(Component)]
struct DomainCooldown(ComponentValue);

#[derive(Default)]
struct DomainPlugin;

impl Plugin for DomainPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("demo.domain-cooldown"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PluginSchemaProviderId::new("demo.domain-cooldown.schema")]);
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Domain cooldown",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("duration"),
            "Duration",
            ComponentFieldPath::from_fields(["duration"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
        ComponentFieldSchema::required(
            ComponentFieldId::new("payload"),
            "Payload",
            ComponentFieldPath::from_fields(["payload"]),
            ComponentValueShape::map(ComponentValueKind::String.into()),
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING),
    ])
}

fn recipe() -> ProductRecipe {
    let provider = ComponentSchemaProviderDefinition::new(
        ComponentSchemaOwnerId::new("demo.domain-cooldown"),
        PluginSchemaProviderId::new("demo.domain-cooldown.schema"),
        ComponentSchemaProviderBindingId::new("demo.domain-cooldown.native", 1),
        || {
            Ok(ComponentSchemaCatalog {
                components: vec![schema()],
                ..Default::default()
            })
        },
        |registry| {
            registry
                .register_persistent_component_with_codec::<DomainCooldown, _, _>(
                    schema(),
                    |value| {
                        let code = match value.field_i64("duration")? {
                            -1 => Some("demo.cooldown.negative"),
                            -2 => Some("demo.cooldown.disabled"),
                            _ => None,
                        };
                        if let Some(code) = code {
                            return Err(ComponentDomainError::new(
                                DiagnosticCode::new(code).unwrap(),
                                SafeSummary::new("Cooldown cannot be used").unwrap(),
                            )
                            .with_field(ComponentFieldId::new("duration"))
                            .into());
                        }
                        Ok(DomainCooldown(value.clone()))
                    },
                    |value| Ok(value.0.clone()),
                )
                .map(|_| ())
        },
    );
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<DomainPlugin>::for_default([provider]).unwrap())
        .unwrap()
}

fn document(duration: i64) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(scene_id("cooldown"))
        .with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([
                    ("duration", ComponentValue::I64(duration)),
                    (
                        "payload",
                        ComponentValue::map([(CANARY, ComponentValue::String(CANARY.to_owned()))]),
                    ),
                ]),
            ),
        )])
}

fn project() -> TestProject {
    let project = TestProject::with_prefab_startup();
    let path = project.path().join("schema/authoring.bundle.json");
    let mut owners = AuthoringSchemaBundle::from_json_bytes(&fs::read(&path).unwrap())
        .unwrap()
        .owners()
        .iter()
        .map(AuthoringSchemaBundleOwner::from)
        .collect::<Vec<_>>();
    owners.push(AuthoringSchemaBundleOwner::new(
        PersistentComponentSchemaOwnerId::try_new("demo.domain-cooldown").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema()],
            ..Default::default()
        },
        None,
    ));
    fs::write(
        path,
        AuthoringSchemaBundle::new(owners)
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    project
}

fn assert_domain(report: &DiagnosticReport, code: &str) {
    let diagnostic = report
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == code)
        .unwrap();
    assert_eq!(diagnostic.summary().as_str(), "Cooldown cannot be used");
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "component-field-id"
                && field.value() == DiagnosticValueRef::Identifier("duration"))
    );
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "component-id"
                && field.value() == DiagnosticValueRef::Identifier(COMPONENT))
    );
    assert!(
        diagnostic
            .fields()
            .iter()
            .any(|field| field.key().as_str() == "schema-owner"
                && field.value() == DiagnosticValueRef::Identifier("demo.domain-cooldown"))
    );
    assert!(!format!("{report:?}").contains(CANARY));
}

#[test]
fn strict_check_and_editor_actions_preserve_native_domain_failures_without_payload_text() {
    let project = project();
    for (duration, code) in [
        (-1, "demo.cooldown.negative"),
        (-2, "demo.cooldown.disabled"),
    ] {
        project.write_scene_source(&document(duration));
        let rejection = check_project(ProjectCheckRequest::new(
            project.root_capability(),
            recipe(),
        ))
        .unwrap_err();
        assert_domain(rejection.content_error().unwrap().diagnostics(), code);
    }
    project.write_scene_source(&document(0));
    let mut editor = EditorProjectSession::open_trusted_compiled(
        project.writable_root_capability(),
        TrustedCompiledEditorProjectIntent::new().with_recipe(recipe()),
    )
    .unwrap();
    let frame = editor.current_frame();
    let document = frame.workspace().active_document.unwrap();
    let base_revision = frame.workspace().scene(document).unwrap().revision;
    for (duration, code) in [
        (-1, "demo.cooldown.negative"),
        (-2, "demo.cooldown.disabled"),
    ] {
        let result = editor_action_driver::dispatch_one(
            &mut editor,
            EditorAction::Mutate(EditorDocumentMutation::ApplyScenePatch {
                document,
                base_revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::SetField {
                    entity: scene_id("cooldown"),
                    component: ComponentTypeId::new(COMPONENT),
                    component_version: ComponentSchemaVersion::ONE,
                    field: ComponentFieldId::new("duration"),
                    value: ComponentValue::I64(duration),
                }]),
            }),
        );
        let EditorActionOutcome::Workspace(report) = result else {
            panic!("expected an Editor workspace result");
        };
        assert!(!report.applied);
        assert_domain(&report.diagnostics, code);
        assert_eq!(
            editor
                .current_frame()
                .workspace()
                .scene(document)
                .unwrap()
                .revision,
            base_revision
        );
    }
}
