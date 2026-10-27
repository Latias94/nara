use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use super::*;

struct Route<P> {
    program: P,
    publication: Rc<()>,
}
struct PreparedResult {
    publication: Rc<()>,
    invocation: Invocation,
    commands: Vec<NameCommand>,
}

impl<P: Language> Route<P> {
    fn admit(program: P, definition: &Definition) -> Result<Self, String> {
        admit_bindings(definition, &program.describe()?).map_err(str::to_owned)?;
        Ok(Self {
            program,
            publication: Rc::new(()),
        })
    }

    fn prepare(
        &self,
        runtime: &Runtime,
        definition: &Definition,
        actor: &SceneEntityId,
    ) -> PreparedResult {
        let invocation = capture(runtime, definition, actor, &mover()).unwrap();
        let commands = self.program.invoke(&invocation.snapshot).unwrap();
        PreparedResult {
            publication: Rc::clone(&self.publication),
            invocation,
            commands,
        }
    }

    fn publish(&self, runtime: &mut Runtime, result: PreparedResult) -> Result<(), &'static str> {
        if !Rc::ptr_eq(&self.publication, &result.publication) {
            return Err("stale execution publication");
        }
        apply(runtime, &result.invocation, result.commands)
    }

    fn execute(&self, runtime: &mut Runtime, definition: &Definition, actor: &SceneEntityId) {
        let result = self.prepare(runtime, definition, actor);
        self.publish(runtime, result).unwrap();
    }
}

fn runtime(definition: &Arc<Definition>, program: &dyn Language) -> Runtime {
    prepare(definition, &program.describe().unwrap(), &source(), None).unwrap()
}

#[test]
fn lua_body_iteration_preserves_world_rejects_old_results_and_leases_actual_vm() {
    let definition = Definition::from_catalog(catalog(), true).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schema_behaviors.lua");
    let initial = include_str!("../../lua/schema_behaviors.lua");
    std::fs::write(&path, initial).unwrap();
    let mut active = Route::admit(LuaProgram::load(&path).unwrap(), &definition).unwrap();
    let mut runtime = runtime(&definition, &active.program);
    let world = runtime.world.id();
    let actor = id("actor");
    let target = runtime.target(&actor);
    active.execute(&mut runtime, &definition, &actor);
    assert_eq!(runtime.native_name(&target), "Moved 2");
    std::fs::write(&path, "return function(").unwrap();
    assert!(LuaProgram::load(&path).is_err());
    active.execute(&mut runtime, &definition, &actor);
    for missing_entry in ["nil", "42"] {
        let incomplete =
            format!("local m = (function() {initial} end)(); m.invoke = {missing_entry}; return m");
        std::fs::write(&path, incomplete).unwrap();
        assert!(LuaProgram::load(&path).is_err());
        active.execute(&mut runtime, &definition, &actor);
        assert_eq!(runtime.native_name(&target), "Moved 2");
    }
    let invalid = format!(
        "local m = (function() {initial} end)(); m.bindings[#m.bindings].version = 2; return m"
    );
    std::fs::write(&path, invalid).unwrap();
    assert!(Route::admit(LuaProgram::load(&path).unwrap(), &definition).is_err());
    assert_eq!(runtime.native_name(&target), "Moved 2");
    let old_result = active.prepare(&runtime, &definition, &actor);
    let old_input = old_result.invocation.snapshot.clone();
    let (weak, cleaned) = active.program.observe_cleanup();
    let lease = active.program.retain_callback();
    let successor = initial.replace("amount * gain)", "amount * gain * 2)");
    assert_ne!(initial, successor);
    std::fs::write(&path, successor).unwrap();
    let candidate = Route::admit(LuaProgram::load(&path).unwrap(), &definition).unwrap();
    let previous = std::mem::replace(&mut active, candidate);
    drop(previous);
    assert!(weak.try_upgrade().is_some());
    assert!(!cleaned.get());
    assert_eq!(lease.invoke(&old_input).unwrap()[0].value, "Moved 2");
    active.execute(&mut runtime, &definition, &actor);
    assert_eq!(runtime.world.id(), world);
    assert_eq!(runtime.native_name(&target), "Moved 4");
    let delayed = PreparedResult {
        publication: Rc::clone(&old_result.publication),
        invocation: capture(&runtime, &definition, &actor, &mover()).unwrap(),
        commands: lease.invoke(&old_input).unwrap(),
    };
    assert_eq!(delayed.commands[0].value, "Moved 2");
    assert_eq!(
        active.publish(&mut runtime, delayed),
        Err("stale execution publication")
    );
    assert_eq!(
        active.publish(&mut runtime, old_result),
        Err("stale execution publication")
    );
    assert_eq!(runtime.native_name(&target), "Moved 4");
    drop(lease);
    assert!(weak.try_upgrade().is_none());
    assert!(cleaned.get());
    runtime.retire().unwrap();
}

fn module_path(variable: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(variable).expect("Runner builds authored revisions"))
}

fn await_collection(retirement: &super::dotnet::Retirement) {
    for _ in 0..16 {
        if retirement.complete() {
            return;
        }
    }
    panic!("Collectible context is still alive; retirement remains pending");
}

#[test]
fn csharp_body_iteration_preserves_world_rejects_old_results_and_roots_real_code() {
    let definition = Definition::from_catalog(catalog(), true).unwrap();
    let mut active = Route::admit(
        DotnetProgram::load(&module_path("NARA_BEHAVIOR_MODULE_LIVE")).unwrap(),
        &definition,
    )
    .unwrap();
    let mut runtime = runtime(&definition, &active.program);
    let world = runtime.world.id();
    let actor = id("actor");
    let target = runtime.target(&actor);
    active.execute(&mut runtime, &definition, &actor);
    assert_eq!(runtime.native_name(&target), "Moved 2");
    assert!(
        active
            .program
            .candidate(Path::new("missing-authored-module.dll"))
            .is_err()
    );
    let mut invalid = active
        .program
        .candidate(&module_path("NARA_BEHAVIOR_MODULE_INVALID"))
        .unwrap();
    assert!(admit_bindings(&definition, &invalid.describe().unwrap()).is_err());
    let rejected_retirement = invalid.retire();
    drop(invalid);
    await_collection(&rejected_retirement);
    active.execute(&mut runtime, &definition, &actor);
    let old_result = active.prepare(&runtime, &definition, &actor);
    let old_input = old_result.invocation.snapshot.clone();
    let lease = active.program.retain_callback();
    let project = module_path("NARA_BEHAVIOR_MODULE_PROJECT");
    let artifacts = module_path("NARA_BEHAVIOR_LIVE_ARTIFACTS");
    let started = std::time::Instant::now();
    let build = std::process::Command::new("dotnet")
        .arg("build")
        .arg(&project)
        .args(["--configuration", "Debug", "--artifacts-path"])
        .arg(&artifacts)
        .args([
            "--disable-build-servers",
            "-m:1",
            "-p:UseSharedCompilation=false",
            "-p:TrialRevision=2",
        ])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    eprintln!(
        "Managed body rebuild while predecessor is loaded: {:?}",
        started.elapsed()
    );
    let candidate = Route::admit(
        active
            .program
            .candidate(&module_path("NARA_BEHAVIOR_MODULE_LIVE"))
            .unwrap(),
        &definition,
    )
    .unwrap();
    let mut previous = std::mem::replace(&mut active, candidate);
    let retirement = previous.program.retire();
    assert!(retirement.invocation_refused(&old_input));
    drop(previous);
    assert!(!retirement.complete());
    assert_eq!(lease.invoke(&old_input).unwrap()[0].value, "Moved 2");
    active.execute(&mut runtime, &definition, &actor);
    assert_eq!(runtime.world.id(), world);
    assert_eq!(runtime.native_name(&target), "Moved 4");
    let delayed = PreparedResult {
        publication: Rc::clone(&old_result.publication),
        invocation: capture(&runtime, &definition, &actor, &mover()).unwrap(),
        commands: lease.invoke(&old_input).unwrap(),
    };
    assert_eq!(delayed.commands[0].value, "Moved 2");
    assert_eq!(
        active.publish(&mut runtime, delayed),
        Err("stale execution publication")
    );
    assert_eq!(
        active.publish(&mut runtime, old_result),
        Err("stale execution publication")
    );
    assert_eq!(runtime.native_name(&target), "Moved 4");
    drop(lease);
    await_collection(&retirement);
    runtime.retire().unwrap();
    let retirement = active.program.retire();
    drop(active);
    await_collection(&retirement);
}
