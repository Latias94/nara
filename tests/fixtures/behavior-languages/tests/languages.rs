//! Schema-driven private feasibility adapters, not a production language SDK.

#[allow(dead_code)]
#[path = "../../module-consumer/tests/support/behavior_trial.rs"]
mod trial;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use mlua::LuaSerdeExt;
use nara_reflect::{
    ComponentFieldId, ComponentFieldPath, ComponentPropertyError, ComponentPropertyPath,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    EntityReference, resolve_component_property,
};
use nara_scene::{
    Name, PrefabDocument, PrefabInstance, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use trial::{Definition, PreparedRuntime, Runtime, StorageKind, id, mover, sensor};

#[derive(Clone, Serialize)]
struct NativeTarget {
    id: String,
    name: String,
}

#[derive(Clone, Serialize)]
struct Snapshot {
    schema: String,
    actor: String,
    fields: BTreeMap<String, ComponentValue>,
    targets: Vec<NativeTarget>,
    fault: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameCommand {
    target: String,
    value: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: String,
    version: u32,
}

trait Language {
    fn describe(&self) -> Result<Vec<Binding>, String>;
    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String>;
}

struct Invocation {
    snapshot: Snapshot,
    targets: BTreeMap<String, trial::Target>,
}

fn admit_bindings(definition: &Definition, bindings: &[Binding]) -> Result<(), &'static str> {
    let mut admitted = BTreeMap::new();
    for binding in bindings {
        if admitted
            .insert(binding.schema.as_str(), binding.version)
            .is_some()
        {
            return Err("duplicate callback binding");
        }
        let schema = definition
            .schemas
            .get(&ComponentTypeId::new(&binding.schema))
            .ok_or("binding has no admitted Schema")?;
        if schema.version().get() != binding.version {
            return Err("callback version does not match Schema");
        }
    }
    if admitted.len() != definition.schemas.len() {
        return Err("missing callback binding");
    }
    Ok(())
}

fn prepare(
    definition: &Arc<Definition>,
    bindings: &[Binding],
    source: &SceneDocument,
    prefab: Option<&PrefabDocument>,
) -> Result<Runtime, String> {
    admit_bindings(definition, bindings).map_err(str::to_owned)?;
    PreparedRuntime::prepare(
        StorageKind::Envelope,
        Arc::clone(definition),
        source,
        prefab,
    )
    .and_then(|prepared| prepared.activate(definition))
    .map_err(|error| format!("{error:?}"))
}

fn capture(
    runtime: &Runtime,
    definition: &Definition,
    actor: &SceneEntityId,
    component: &ComponentTypeId,
) -> Result<Invocation, String> {
    let target = runtime.target(actor);
    let mut authority = BTreeMap::from([(actor.as_str().to_owned(), target.clone())]);
    let record = runtime
        .read(&target, component)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("absent Behavior")?;
    let schema = &definition.schemas[component];
    let mut fields = BTreeMap::new();
    for field in schema.fields() {
        match resolve_component_property(
            schema,
            record.version,
            &record.value,
            &ComponentPropertyPath::new(field.id.clone()),
        ) {
            Ok(property) => {
                fields.insert(field.id.as_str().to_owned(), property.value.clone());
            }
            Err(ComponentPropertyError::ValueUnavailable(_)) if !field.required => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    let references = nara_reflect::__private::plan_declared_entity_references::<()>(
        schema,
        &record.value,
        Default::default(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let mut targets = Vec::new();
    references
        .visit(&record.value, |_, reference| {
            let EntityReference::SceneLocal { entity } = reference else {
                return Err(());
            };
            let target = runtime.target(entity);
            runtime.entity(&target).map_err(|_| ())?;
            authority.insert(entity.as_str().to_owned(), target.clone());
            targets.push(NativeTarget {
                id: entity.as_str().to_owned(),
                name: runtime.native_name(&target).to_owned(),
            });
            Ok(())
        })
        .map_err(|_| "unresolved native target")?;
    Ok(Invocation {
        snapshot: Snapshot {
            schema: component.as_str().to_owned(),
            actor: actor.as_str().to_owned(),
            fields,
            targets,
            fault: "",
        },
        targets: authority,
    })
}

fn apply(
    runtime: &mut Runtime,
    invocation: &Invocation,
    commands: Vec<NameCommand>,
) -> Result<(), &'static str> {
    if commands.len() > invocation.snapshot.targets.len() + 1 {
        return Err("command count exceeds invocation scope");
    }
    let mut prepared = Vec::new();
    for command in commands {
        let target = invocation
            .targets
            .get(&command.target)
            .ok_or("foreign target")?;
        let entity = runtime.entity(target).map_err(|_| "stale target")?;
        if runtime.world.get::<Name>(entity).is_none() || command.value.len() > 1024 {
            return Err("invalid native Name change");
        }
        prepared.push((entity, command.value));
    }
    // No language code or borrowed World state remains in the exclusive publication tail.
    for (entity, value) in prepared {
        runtime.world.get_mut::<Name>(entity).unwrap().0 = value;
    }
    Ok(())
}

fn invoke(
    runtime: &mut Runtime,
    definition: &Definition,
    program: &dyn Language,
    actor: &SceneEntityId,
    component: &ComponentTypeId,
) {
    let input = capture(runtime, definition, actor, component).unwrap();
    apply(runtime, &input, program.invoke(&input.snapshot).unwrap()).unwrap();
}

fn catalog() -> ComponentSchemaCatalog {
    ComponentSchemaCatalog::from_json_bytes(
        &std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("data/behaviors.catalog.json"))
            .unwrap(),
    )
    .unwrap()
}

fn source() -> SceneDocument {
    let candidate = SceneDocumentCandidate::decode_json_bytes(
        &std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("data/game.scene.json")).unwrap(),
    )
    .unwrap();
    // Source admission is performed by the real authoring session under the loaded catalog.
    let definition = Definition::from_catalog(catalog(), true).unwrap();
    candidate
        .admit_authoring_with_basis(
            nara_scene::AuthoringDocumentSource::new(
                nara_asset::AssetRef::path("game.scene.json").unwrap(),
            ),
            &definition.basis,
        )
        .unwrap()
        .publish()
        .document()
        .clone()
}

fn assert_export(
    runtime: &mut Runtime,
    definition: &Definition,
    actors: &[SceneEntityId],
    amount: f64,
) {
    // Independent native markers encode the expected original pair, not exported Sensor data.
    for actor in actors {
        let sensor = capture(runtime, definition, actor, &sensor()).unwrap();
        let actor_entity = runtime.entity(&runtime.target(actor)).unwrap();
        let friend = runtime
            .entity(&runtime.target(&id(&sensor.snapshot.targets[0].id)))
            .unwrap();
        assert_eq!(
            runtime.world.get::<Name>(friend).unwrap().as_str(),
            "Friend signalled"
        );
        runtime.world.get_mut::<Name>(actor_entity).unwrap().0 = format!("Export actor {actor}");
        runtime.world.get_mut::<Name>(friend).unwrap().0 = format!("Export friend {actor}");
    }
    let exported = runtime.export().unwrap();
    assert!(trial::compile(&exported, definition, None).is_ok());
    assert_eq!(
        exported
            .entities
            .iter()
            .filter(|entity| entity.components.contains_key(&mover()))
            .count(),
        actors.len()
    );
    for actor in actors {
        let named = |name: String| {
            exported
                .entities
                .iter()
                .find(|entity| {
                    entity
                        .components
                        .get(&ComponentTypeId::new("nara.scene.Name"))
                        .is_some_and(|record| record.value == ComponentValue::String(name.clone()))
                })
                .unwrap()
        };
        let exported_actor = named(format!("Export actor {actor}"));
        let friend = named(format!("Export friend {actor}"));
        assert_eq!(
            exported_actor.components[&mover()]
                .value
                .get("amount")
                .unwrap()
                .as_f64(),
            Some(amount)
        );
        assert_eq!(
            exported_actor.components[&sensor()]
                .value
                .get("targets")
                .unwrap(),
            &ComponentValue::List(vec![
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: friend.id.clone()
                }),
                ComponentValue::Null,
            ])
        );
    }
    assert_eq!(
        exported
            .entities
            .iter()
            .filter(|entity| entity
                .components
                .contains_key(&ComponentTypeId::new("trial.behavior.Label")))
            .count(),
        actors.len()
    );
}

fn workflow(program: &dyn Language) {
    let definition = Definition::from_catalog(catalog(), true).unwrap();
    let bindings = program.describe().unwrap();
    let original = source();
    let mut session = trial::session(&original, &definition);
    let patch = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("actor"),
        component: mover(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("amount"),
        value: ComponentValue::f64(7.0).unwrap(),
    }]);
    let edit = session
        .prepare_authoring_edit(session.revision(), &patch)
        .unwrap();
    assert!(session.commit_authoring_edit(edit).applied);
    assert!(session.undo_authoring().applied);
    assert!(session.redo_authoring().applied);
    let label = ComponentTypeId::new("trial.behavior.Label");
    let label_record = original
        .entities
        .iter()
        .find(|entity| entity.id == id("label"))
        .unwrap()
        .components[&label]
        .clone();
    let remove = ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
        entity: id("label"),
        component: label.clone(),
    }]);
    let edit = session
        .prepare_authoring_edit(session.revision(), &remove)
        .unwrap();
    assert!(session.commit_authoring_edit(edit).applied);
    assert!(
        !session
            .document()
            .entities
            .iter()
            .find(|entity| entity.id == id("label"))
            .unwrap()
            .components
            .contains_key(&label)
    );
    assert!(session.undo_authoring().applied);
    assert!(session.redo_authoring().applied);
    let attach = ScenePatchDocument::new([ScenePatchOperation::AddComponent {
        entity: id("label"),
        component: label.clone(),
        value: label_record.clone(),
    }]);
    let edit = session
        .prepare_authoring_edit(session.revision(), &attach)
        .unwrap();
    assert!(session.commit_authoring_edit(edit).applied);
    assert!(session.undo_authoring().applied);
    assert!(session.redo_authoring().applied);
    let label_edit = ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: id("label"),
        component: label.clone(),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("text"),
        value: ComponentValue::String("Edited label".into()),
    }]);
    let edit = session
        .prepare_authoring_edit(session.revision(), &label_edit)
        .unwrap();
    assert!(session.commit_authoring_edit(edit).applied);
    assert!(session.undo_authoring().applied);
    assert!(session.redo_authoring().applied);
    let directory = tempfile::tempdir().unwrap();
    for json in [true, false] {
        let path = directory.path().join(if json {
            "saved.scene.json"
        } else {
            "saved.scene.ron"
        });
        std::fs::write(
            &path,
            if json {
                session.document().to_json_string().unwrap()
            } else {
                session.document().to_ron_string().unwrap()
            },
        )
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let candidate = if json {
            SceneDocumentCandidate::decode_json_bytes(&bytes)
        } else {
            SceneDocumentCandidate::decode_ron_bytes(&bytes)
        }
        .unwrap();
        let reopened = session.try_reopen_authoring_candidate(candidate).unwrap();
        assert_eq!(reopened.document(), session.document());
        assert_eq!(reopened.history_usage().records, 0);
    }
    let edited = session.document();
    let prefab = PrefabDocument::new(edited.roots.clone(), edited.entities.clone());
    let linked = SceneDocument::from_ordered_roots(["first", "second"].map(|anchor| {
        let mut record = SceneEntityRecord::new(id(anchor));
        record.prefab = Some(PrefabInstance {
            source: nara_asset::AssetRef::path("base.prefab.json").unwrap(),
            overrides: None,
        });
        record
    }));
    for (source, prefab, count) in [(edited, None, 1), (&linked, Some(&prefab), 2)] {
        let candidate = trial::compile(source, &definition, prefab)
            .unwrap()
            .candidate
            .unwrap();
        let mut runtime = prepare(&definition, &bindings, source, prefab).unwrap();
        // Every present Behavior is dispatched by its admitted ID, not a native type/field switch.
        for entity in &candidate.document().entities {
            let target = runtime.target(&entity.id);
            for component in runtime.components(&target).unwrap() {
                invoke(&mut runtime, &definition, program, &entity.id, &component);
            }
        }
        for entity in &candidate.document().entities {
            let target = runtime.target(&entity.id);
            if entity.components.contains_key(&mover()) {
                assert_eq!(runtime.native_name(&target), "Moved 7");
            }
            if entity
                .components
                .contains_key(&ComponentTypeId::new("trial.behavior.Label"))
            {
                assert_eq!(runtime.native_name(&target), "Edited label");
            }
        }
        let actors = candidate
            .document()
            .entities
            .iter()
            .filter(|entity| entity.components.contains_key(&mover()))
            .map(|entity| entity.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(actors.len(), count);
        let actor = &candidate
            .document()
            .entities
            .iter()
            .find(|entity| entity.components.contains_key(&mover()))
            .unwrap()
            .id;
        let target = runtime.target(actor);
        let input = capture(&runtime, &definition, actor, &mover()).unwrap();
        for fault in ["throw", "bad_target"] {
            let fault_schema = if fault == "bad_target" {
                sensor()
            } else {
                mover()
            };
            let mut failed = capture(&runtime, &definition, actor, &fault_schema).unwrap();
            failed
                .snapshot
                .fields
                .insert("amount".into(), ComponentValue::f64(123.0).unwrap());
            failed.snapshot.fault = fault;
            let output = program.invoke(&failed.snapshot);
            if fault == "throw" {
                assert!(output.is_err());
            } else {
                let output = output.unwrap();
                assert_eq!(output.len(), 2);
                assert_eq!(apply(&mut runtime, &failed, output), Err("foreign target"));
                let friend = runtime.target(&id(&failed.snapshot.targets[0].id));
                assert_eq!(runtime.native_name(&friend), "Friend signalled");
            }
            assert_eq!(runtime.native_name(&target), "Moved 7");
        }
        let mut foreign = prepare(&definition, &bindings, source, prefab).unwrap();
        assert!(
            apply(
                &mut foreign,
                &input,
                program.invoke(&input.snapshot).unwrap()
            )
            .is_err()
        );
        assert_eq!(foreign.native_name(&foreign.target(actor)), "Actor");
        foreign.retire().unwrap();
        for mut invalid in [
            bindings[1..].to_vec(),
            bindings
                .iter()
                .cloned()
                .chain([bindings[0].clone()])
                .collect(),
            bindings.clone(),
        ] {
            if invalid.len() == bindings.len() {
                invalid[0].version += 1;
            }
            assert!(prepare(&definition, &invalid, source, prefab).is_err());
            invoke(&mut runtime, &definition, program, actor, &mover());
            assert_eq!(runtime.native_name(&target), "Moved 7");
        }
        assert_export(&mut runtime, &definition, &actors, 7.0);
        runtime.retire().unwrap();
        assert_eq!(runtime.retained_rows(), 0);
        assert_eq!(
            apply(
                &mut runtime,
                &input,
                program.invoke(&input.snapshot).unwrap()
            ),
            Err("stale target"),
        );
    }

    // A field declared only in the external catalog needs no native extraction branch.
    let mut updated = original.clone();
    updated
        .entities
        .iter_mut()
        .find(|entity| entity.id == id("actor"))
        .unwrap()
        .components
        .get_mut(&mover())
        .unwrap()
        .value
        .set_path(
            &ComponentFieldPath::from_fields(["wire_gain"]),
            ComponentValue::f64(3.0).unwrap(),
        )
        .unwrap();
    let mut runtime = prepare(&definition, &bindings, &updated, None).unwrap();
    invoke(&mut runtime, &definition, program, &id("actor"), &mover());
    assert_eq!(
        runtime.native_name(&runtime.target(&id("actor"))),
        "Moved 6"
    );
    runtime.retire().unwrap();
    let mut no_gain = prepare(&definition, &bindings, &original, None).unwrap();
    invoke(&mut no_gain, &definition, program, &id("actor"), &mover());
    assert_eq!(
        no_gain.native_name(&no_gain.target(&id("actor"))),
        "Moved 2"
    );
    let label_input = capture(&no_gain, &definition, &id("label"), &label).unwrap();
    assert_eq!(
        label_input
            .snapshot
            .fields
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["text"]
    );
    let mut wrong_schema = label_input.snapshot.clone();
    wrong_schema.schema = "trial.behavior.Missing".into();
    assert!(program.invoke(&wrong_schema).is_err());
    no_gain.retire().unwrap();
}

struct LuaProgram {
    lua: mlua::Lua,
    module: mlua::Table,
}

impl LuaProgram {
    fn compile(source: &str) -> mlua::Result<Self> {
        let lua = mlua::Lua::new();
        let module = lua.load(source).set_name("schema_behaviors.lua").eval()?;
        Ok(Self { lua, module })
    }
}

impl Language for LuaProgram {
    fn describe(&self) -> Result<Vec<Binding>, String> {
        self.module
            .get::<mlua::Value>("bindings")
            .and_then(|value| self.lua.from_value(value))
            .map_err(|error| error.to_string())
    }
    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        let input = self
            .lua
            .to_value(input)
            .map_err(|error| error.to_string())?;
        let callback: mlua::Function = self
            .module
            .get("invoke")
            .map_err(|error| error.to_string())?;
        let output = callback
            .call::<mlua::Value>(input)
            .map_err(|error| error.to_string())?;
        self.lua
            .from_value(output)
            .map_err(|error| error.to_string())
    }
}

type ManagedEntry = unsafe extern "system" fn(*const u8, i32, *mut u8, i32) -> i32;
struct DotnetProgram {
    callback: netcorehost::hostfxr::ManagedFunction<ManagedEntry>,
    describe: netcorehost::hostfxr::ManagedFunction<ManagedEntry>,
    _loader: netcorehost::hostfxr::AssemblyDelegateLoader,
    _context:
        netcorehost::hostfxr::HostfxrContext<netcorehost::hostfxr::InitializedForRuntimeConfig>,
}

impl DotnetProgram {
    fn load() -> Self {
        use netcorehost::{hostfxr::Hostfxr, pdcstr, pdcstring::PdCString};
        let directory = std::path::PathBuf::from(
            std::env::var_os("NARA_BEHAVIOR_MANAGED_DIR")
                .expect("Run python tests/fixtures/behavior-languages/run.py"),
        );
        let hostfxr = Hostfxr::load_from_path(
            std::env::var_os("NARA_BEHAVIOR_HOSTFXR").expect("Runner selects hostfxr"),
        )
        .unwrap();
        let config =
            PdCString::from_os_str(directory.join("Nara.BehaviorTrial.runtimeconfig.json"))
                .unwrap();
        let assembly = PdCString::from_os_str(directory.join("Nara.BehaviorTrial.dll")).unwrap();
        let context = hostfxr.initialize_for_runtime_config(&config).unwrap();
        let loader = context.get_delegate_loader_for_assembly(assembly).unwrap();
        let callback = loader
            .get_function_with_unmanaged_callers_only::<ManagedEntry>(
                pdcstr!("Nara.BehaviorTrial.Bridge, Nara.BehaviorTrial"),
                pdcstr!("Invoke"),
            )
            .unwrap();
        let describe = loader
            .get_function_with_unmanaged_callers_only::<ManagedEntry>(
                pdcstr!("Nara.BehaviorTrial.Bridge, Nara.BehaviorTrial"),
                pdcstr!("Describe"),
            )
            .unwrap();
        Self {
            callback,
            describe,
            _loader: loader,
            _context: context,
        }
    }

    fn call<T: Serialize, R: DeserializeOwned>(
        &self,
        callback: &netcorehost::hostfxr::ManagedFunction<ManagedEntry>,
        input: &T,
    ) -> Result<R, String> {
        let input = serde_json::to_vec(input).map_err(|error| error.to_string())?;
        let length = i32::try_from(input.len()).map_err(|error| error.to_string())?;
        let mut output = vec![0_u8; 64 * 1024];
        // SAFETY: Both Bridge entries use this exact platform ABI/signature, copy input,
        // bound output and catch managed exceptions. Buffers and owners span the call.
        let count = unsafe {
            callback(
                input.as_ptr(),
                length,
                output.as_mut_ptr(),
                output.len() as i32,
            )
        };
        let count =
            usize::try_from(count).map_err(|_| format!("Managed callback failed: {count}"))?;
        if count > output.len() {
            return Err("Invalid managed buffer length".into());
        }
        serde_json::from_slice(&output[..count]).map_err(|error| error.to_string())
    }
}

impl Language for DotnetProgram {
    fn describe(&self) -> Result<Vec<Binding>, String> {
        self.call(&self.describe, &())
    }
    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        self.call(&self.callback, input)
    }
}

#[test]
fn lua_executes_external_schemas_without_native_field_or_type_dispatch() {
    let program = LuaProgram::compile(include_str!("../lua/schema_behaviors.lua")).unwrap();
    assert_eq!(
        program.lua.globals().get::<String>("_VERSION").unwrap(),
        "Lua 5.4"
    );
    assert!(LuaProgram::compile("return function(").is_err());
    workflow(&program);
}

#[test]
fn csharp_executes_external_schemas_without_native_field_or_type_dispatch() {
    workflow(&DotnetProgram::load());
}

#[test]
fn invalid_catalog_and_native_schema_collision_refuse_definition() {
    let mut invalid: serde_json::Value =
        serde_json::from_slice(include_bytes!("../data/behaviors.catalog.json")).unwrap();
    invalid["payload"]["components"][0]["fields"][0]["shape"]["kind"] = "not-a-kind".into();
    assert!(
        ComponentSchemaCatalog::from_json_str(&serde_json::to_string(&invalid).unwrap()).is_err()
    );
    let mut collision = catalog();
    collision.components[0].id = ComponentTypeId::new("nara.scene.Name");
    assert!(Definition::from_catalog(collision, true).is_err());
}
