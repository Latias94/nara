//! Experimental language execution over the public Scene data and source-authoring APIs.

#[path = "support/authoring_project.rs"]
mod authoring_project;
#[path = "support/scene_runtime.rs"]
mod scene_runtime;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use nara_reflect::{
    ComponentFieldPath, ComponentPropertyError, ComponentPropertyPath, ComponentSchemaCatalog,
    ComponentTypeId, ComponentValue, EntityReference, resolve_component_property,
    rewrite_declared_entity_references,
};
use nara_scene::{
    Name, PrefabDocument, PrefabInstance, SceneDocument, SceneDocumentCandidate, SceneEntityId,
    SceneEntityRecord,
};
use scene_runtime::{Definition, Runtime, id, mover, sensor};
use serde::{Deserialize, Serialize};

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
    targets: BTreeMap<String, scene_runtime::Target>,
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
            .owner
            .schema(&ComponentTypeId::new(&binding.schema))
            .ok_or("binding has no admitted Schema")?;
        if schema.version().get() != binding.version {
            return Err("callback version does not match Schema");
        }
    }
    if admitted.len() != definition.owner.current().components().len() {
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
    Runtime::materialize(Arc::clone(definition), source, prefab)
}

fn capture(
    runtime: &Runtime,
    definition: &Definition,
    actor: &SceneEntityId,
    component: &ComponentTypeId,
) -> Result<Invocation, String> {
    if !runtime.matches_definition(definition) {
        return Err("language observation uses another Scene definition admission".into());
    }
    let target = runtime.target(actor);
    let mut authority = BTreeMap::from([(actor.as_str().to_owned(), target.clone())]);
    let record = runtime
        .read(&target, component)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("absent Behavior")?;
    let schema = definition
        .owner
        .schema(component)
        .ok_or("unselected data Schema")?;
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
    let mut targets = Vec::new();
    // Observe through the public bounded reference traversal; keep the rewritten value local.
    rewrite_declared_entity_references(
        schema,
        &record.value,
        Default::default(),
        |_, reference| {
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
            Ok(reference.clone())
        },
    )
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
    candidate.into_composition_source().unwrap()
}

fn assert_export(
    runtime: &mut Runtime,
    definition: &Definition,
    actors: &[SceneEntityId],
    amount: f64,
    expected: &SceneDocument,
) -> SceneDocument {
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
    assert!(scene_runtime::compile(&exported, definition, None).is_ok());
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
            &exported_actor.components[&mover()],
            expected
                .entities
                .iter()
                .find_map(|entity| entity.components.get(&mover()))
                .unwrap(),
        );
        assert_eq!(
            exported_actor.components[&mover()]
                .value
                .get("amount")
                .unwrap()
                .as_f64(),
            Some(amount)
        );
        let mut expected_sensor = expected
            .entities
            .iter()
            .find_map(|entity| entity.components.get(&sensor()))
            .unwrap()
            .clone();
        expected_sensor
            .value
            .set_path(
                &ComponentFieldPath::from_fields(["targets"]),
                ComponentValue::List(vec![
                    ComponentValue::EntityReference(EntityReference::SceneLocal {
                        entity: friend.id.clone(),
                    }),
                    ComponentValue::Null,
                ]),
            )
            .unwrap();
        assert_eq!(exported_actor.components[&sensor()], expected_sensor);
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
    let label = ComponentTypeId::new("trial.behavior.Label");
    let expected_label = expected
        .entities
        .iter()
        .find_map(|entity| entity.components.get(&label))
        .unwrap();
    for entity in &exported.entities {
        if let Some(record) = entity.components.get(&label) {
            assert_eq!(record, expected_label);
        }
    }
    exported
}

fn workflow(program: &dyn Language) {
    let definition = Definition::from_catalog(catalog()).unwrap();
    let bindings = program.describe().unwrap();
    let original = source();
    let edited = authoring_project::edit_save_reopen(&definition.owner, &original);
    let label = ComponentTypeId::new("trial.behavior.Label");
    let prefab = PrefabDocument::new(edited.roots.clone(), edited.entities.clone());
    let linked = SceneDocument::from_ordered_roots(["first", "second"].map(|anchor| {
        let mut record = SceneEntityRecord::new(id(anchor));
        record.prefab = Some(PrefabInstance {
            source: nara_asset::AssetRef::path("base.prefab.json").unwrap(),
            overrides: None,
        });
        record
    }));
    for (source, prefab, count) in [(&edited, None, 1), (&linked, Some(&prefab), 2)] {
        let candidate = scene_runtime::compile(source, &definition, prefab).unwrap();
        let mut runtime = prepare(&definition, &bindings, source, prefab).unwrap();
        if count == 2 {
            let locate = |anchor: &str, local: &str| {
                candidate
                    .provenance()
                    .iter()
                    .find(|(_, source)| {
                        source.local_entity_id() == &id(local)
                            && source
                                .projection_path()
                                .is_some_and(|path| path.root_anchor() == &id(anchor))
                    })
                    .unwrap()
                    .0
                    .clone()
            };
            let actor = locate("first", "actor");
            let own_friend = runtime.target(&locate("first", "friend"));
            let other_friend_id = locate("second", "friend");
            let other_friend = runtime.target(&other_friend_id);
            let scoped = capture(&runtime, &definition, &actor, &sensor()).unwrap();
            let mut proposals = program.invoke(&scoped.snapshot).unwrap();
            assert_eq!(proposals.len(), 1);
            proposals.push(NameCommand {
                target: other_friend_id.as_str().into(),
                value: "Unauthorized other occurrence".into(),
            });
            assert_eq!(
                apply(&mut runtime, &scoped, proposals),
                Err("foreign target")
            );
            assert_eq!(runtime.native_name(&own_friend), "Friend");
            assert_eq!(runtime.native_name(&other_friend), "Friend");
            invoke(&mut runtime, &definition, program, &actor, &sensor());
            assert_eq!(runtime.native_name(&own_friend), "Friend signalled");
            assert_eq!(runtime.native_name(&other_friend), "Friend");
            // Restore the independent native observation before ordinary full dispatch.
            let entity = runtime.entity(&own_friend).unwrap();
            runtime.world.get_mut::<Name>(entity).unwrap().0 = "Friend".into();
        }
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
        let readmitted = Definition::from_catalog(catalog()).unwrap();
        assert_eq!(
            readmitted.bindings.composition_fingerprint(),
            definition.bindings.composition_fingerprint()
        );
        assert!(capture(&runtime, &readmitted, actor, &mover()).is_err());
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
        let exported = assert_export(&mut runtime, &definition, &actors, 7.0, &edited);
        let mut replay = prepare(&definition, &bindings, &exported, None).unwrap();
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
                .id
                .clone()
        };
        let replay_actor = named(format!("Export actor {}", actors[0]));
        invoke(&mut replay, &definition, program, &replay_actor, &sensor());
        for (index, actor) in actors.iter().enumerate() {
            let initial = format!("Export friend {actor}");
            let target = replay.target(&named(initial.clone()));
            assert_eq!(
                replay.native_name(&target),
                if index == 0 {
                    format!("{initial} signalled")
                } else {
                    initial
                }
            );
        }
        for entity in &exported.entities {
            if entity.components.contains_key(&label) {
                invoke(&mut replay, &definition, program, &entity.id, &label);
                assert_eq!(
                    replay.native_name(&replay.target(&entity.id)),
                    "Edited label"
                );
            }
        }
        replay.retire().unwrap();
        runtime.retire().unwrap();
        assert_eq!(
            runtime
                .world
                .query::<&nara_scene::SceneDataComponents>()
                .iter(&runtime.world)
                .count(),
            0
        );
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

#[path = "support/dotnet.rs"]
mod dotnet;
#[path = "support/lifetime.rs"]
mod lifetime;
#[path = "support/lua.rs"]
mod lua;
use dotnet::DotnetProgram;
use lua::LuaProgram;

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
    workflow(&DotnetProgram::load_default());
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
    assert!(Definition::from_catalog(collision).is_err());
}
