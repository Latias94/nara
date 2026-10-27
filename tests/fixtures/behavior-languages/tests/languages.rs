//! Private in-process feasibility transports, not a public Behavior or language API.

#[allow(dead_code)]
#[path = "../../module-consumer/tests/support/behavior_trial.rs"]
mod trial;

use std::{collections::BTreeMap, sync::Arc};

use mlua::LuaSerdeExt;
use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentValue, EntityReference};
use nara_scene::{Name, SceneEntityId, ScenePatchDocument, ScenePatchOperation};
use serde::{Deserialize, Serialize};
use trial::{Definition, PreparedRuntime, Runtime, StorageKind, id, mover, record, sensor};

#[derive(Clone, Serialize)]
struct NativeTarget {
    id: String,
    name: String,
}

#[derive(Clone, Serialize)]
struct Snapshot {
    event: &'static str,
    actor: String,
    amount: f64,
    targets: Vec<NativeTarget>,
    fault: &'static str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NameCommand {
    target: String,
    value: String,
}

#[derive(Clone)]
struct Invocation {
    snapshot: Snapshot,
    targets: BTreeMap<String, trial::Target>,
}

fn capture(runtime: &Runtime, definition: &Definition, actor: &SceneEntityId) -> Invocation {
    let target = runtime.target(actor);
    let mut authority = BTreeMap::from([(actor.as_str().to_owned(), target.clone())]);
    let amount = runtime
        .read(&target, &mover())
        .unwrap()
        .unwrap()
        .value
        .get("amount")
        .unwrap()
        .as_f64()
        .unwrap();
    let sensor_record = runtime.read(&target, &sensor()).unwrap().unwrap();
    let references = nara_reflect::__private::plan_declared_entity_references::<()>(
        &definition.schemas[&sensor()],
        &sensor_record.value,
        Default::default(),
    )
    .unwrap();
    let mut targets = Vec::new();
    references
        .visit(&sensor_record.value, |_, reference| {
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
        .unwrap();
    Invocation {
        snapshot: Snapshot {
            event: "tick",
            actor: actor.as_str().to_owned(),
            amount,
            targets,
            fault: "",
        },
        targets: authority,
    }
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
    // The callback has returned; every command was checked before this exclusive native tail.
    for (entity, value) in prepared {
        runtime.world.get_mut::<Name>(entity).unwrap().0 = value;
    }
    Ok(())
}

fn workflow(invoke: impl Fn(&Snapshot) -> Result<Vec<NameCommand>, String>) {
    let definition = Definition::new(1, true);
    let mut session = trial::session(&trial::source(), &definition);
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
    let edited = session.document();
    let (linked, _) = trial::prefab_source();
    let prefab = nara_scene::PrefabDocument::new(edited.roots.clone(), edited.entities.clone());
    for (source, prefab) in [(edited, None), (&linked, Some(&prefab))] {
        let report = trial::compile(source, &definition, prefab).unwrap();
        let candidate = report.candidate.unwrap();
        let actor = candidate
            .document()
            .entities
            .iter()
            .find(|entity| entity.components.contains_key(&mover()))
            .unwrap()
            .id
            .clone();
        let mut runtime = PreparedRuntime::prepare(
            StorageKind::Envelope,
            Arc::clone(&definition),
            source,
            prefab,
        )
        .unwrap()
        .activate(&definition)
        .unwrap();
        let target = runtime.target(&actor);
        let input = capture(&runtime, &definition, &actor);
        apply(&mut runtime, &input, invoke(&input.snapshot).unwrap()).unwrap();
        assert_eq!(runtime.native_name(&target), "Moved 7");
        let mut signal = capture(&runtime, &definition, &actor);
        signal.snapshot.event = "signal";
        apply(&mut runtime, &signal, invoke(&signal.snapshot).unwrap()).unwrap();
        let friend = runtime.target(&id(&signal.snapshot.targets[0].id));
        assert_eq!(runtime.native_name(&friend), "Friend signalled");
        runtime
            .set(
                &target,
                &mover(),
                record(
                    ComponentValue::map([("amount", ComponentValue::f64(9.0).unwrap())]),
                    1,
                ),
            )
            .unwrap();
        let input = capture(&runtime, &definition, &actor);
        apply(&mut runtime, &input, invoke(&input.snapshot).unwrap()).unwrap();
        assert_eq!(runtime.native_name(&target), "Moved 9");

        let mut failed = input.clone();
        failed.snapshot.amount = 123.0;
        failed.snapshot.fault = "throw";
        assert!(invoke(&failed.snapshot).is_err());
        assert_eq!(runtime.native_name(&target), "Moved 9");
        failed.snapshot.fault = "bad_target";
        let commands = invoke(&failed.snapshot).unwrap();
        assert_eq!(commands.len(), 2);
        assert!(apply(&mut runtime, &failed, commands).is_err());
        assert_eq!(runtime.native_name(&target), "Moved 9");
        let mut foreign = PreparedRuntime::prepare(
            StorageKind::Envelope,
            Arc::clone(&definition),
            source,
            prefab,
        )
        .unwrap()
        .activate(&definition)
        .unwrap();
        assert!(apply(&mut foreign, &input, invoke(&input.snapshot).unwrap()).is_err());
        assert_eq!(foreign.native_name(&foreign.target(&actor)), "Actor");
        foreign.retire().unwrap();
        let exported = runtime.export().unwrap();
        assert!(trial::compile(&exported, &definition, None).is_ok());
        let exported_actor = exported
            .entities
            .iter()
            .find(|entity| entity.components.contains_key(&mover()))
            .unwrap();
        assert_eq!(
            exported_actor.components[&mover()]
                .value
                .get("amount")
                .unwrap()
                .as_f64(),
            Some(9.0),
        );
        assert!(exported_actor.components.contains_key(&sensor()));
        let friend = exported
            .entities
            .iter()
            .find(|entity| {
                entity
                    .components
                    .get(&nara_reflect::ComponentTypeId::new("nara.scene.Name"))
                    .is_some_and(|record| {
                        record.value == ComponentValue::String("Friend signalled".into())
                    })
            })
            .unwrap();
        // Locate the target through its native effect, independently of Sensor's exported value.
        assert_eq!(
            exported_actor.components[&sensor()]
                .value
                .get("targets")
                .unwrap(),
            &ComponentValue::List(vec![
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: friend.id.clone(),
                }),
                ComponentValue::Null,
            ]),
        );
        runtime.retire().unwrap();
        assert_eq!(runtime.retained_rows(), 0);
    }
}

struct LuaProgram {
    lua: mlua::Lua,
    callback: mlua::Function,
}

impl LuaProgram {
    fn compile(source: &str) -> mlua::Result<Self> {
        let lua = mlua::Lua::new();
        let callback = lua.load(source).set_name("behaviors.lua").eval()?;
        Ok(Self { lua, callback })
    }

    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        let input = self
            .lua
            .to_value(input)
            .map_err(|error| error.to_string())?;
        let output = self
            .callback
            .call::<mlua::Value>(input)
            .map_err(|error| error.to_string())?;
        self.lua
            .from_value(output)
            .map_err(|error| error.to_string())
    }
}

#[test]
fn lua_executes_source_and_prefab_data_through_bounded_native_commands() {
    let program = LuaProgram::compile(include_str!("../lua/behaviors.lua")).unwrap();
    assert_eq!(
        program.lua.globals().get::<String>("_VERSION").unwrap(),
        "Lua 5.4"
    );
    assert!(LuaProgram::compile("return function(").is_err());
    workflow(|input| program.invoke(input));
}

type ManagedEntry = unsafe extern "system" fn(*const u8, i32, *mut u8, i32) -> i32;

struct DotnetProgram {
    callback: netcorehost::hostfxr::ManagedFunction<ManagedEntry>,
    _loader: netcorehost::hostfxr::AssemblyDelegateLoader,
    _context:
        netcorehost::hostfxr::HostfxrContext<netcorehost::hostfxr::InitializedForRuntimeConfig>,
}

impl DotnetProgram {
    fn load() -> Self {
        use netcorehost::{hostfxr::Hostfxr, pdcstr, pdcstring::PdCString};
        let directory =
            std::path::PathBuf::from(std::env::var_os("NARA_BEHAVIOR_MANAGED_DIR").expect(
                "Run the complete comparison with python tests/fixtures/behavior-languages/run.py",
            ));
        let hostfxr = Hostfxr::load_from_path(
            std::env::var_os("NARA_BEHAVIOR_HOSTFXR")
                .expect("The comparison runner selects hostfxr"),
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
        Self {
            callback,
            _loader: loader,
            _context: context,
        }
    }

    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        let input = serde_json::to_vec(input).map_err(|error| error.to_string())?;
        let length = i32::try_from(input.len()).map_err(|error| error.to_string())?;
        let mut output = vec![0_u8; 64 * 1024];
        // SAFETY: Bridge.Invoke has this exact platform ABI and integer/pointer signature.
        // Buffers stay live for the synchronous call; the bridge copies the input, bounds
        // its output and catches managed exceptions. No World or borrowed component escapes.
        let count = unsafe {
            (self.callback)(
                input.as_ptr(),
                length,
                output.as_mut_ptr(),
                output.len() as i32,
            )
        };
        let count =
            usize::try_from(count).map_err(|_| format!("Managed callback failed: {count}"))?;
        if count > output.len() {
            return Err("Managed callback returned an invalid buffer length".into());
        }
        serde_json::from_slice(&output[..count]).map_err(|error| error.to_string())
    }
}

#[test]
fn csharp_executes_source_and_prefab_data_through_bounded_native_commands() {
    let program = DotnetProgram::load();
    workflow(|input| program.invoke(input));
}
