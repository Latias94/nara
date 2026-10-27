use std::{cell::Cell, rc::Rc};

use mlua::LuaSerdeExt;

use super::{Binding, Language, NameCommand, Snapshot};

pub(super) struct LuaProgram {
    pub(super) lua: mlua::Lua,
    bindings: Vec<Binding>,
    callback: mlua::Function,
}

struct CleanupProbe(Rc<Cell<bool>>);

impl mlua::UserData for CleanupProbe {}

impl Drop for CleanupProbe {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

pub(super) struct CallbackLease {
    // mlua Function is weak with respect to the VM. This lease must own the VM explicitly.
    lua: mlua::Lua,
    callback: mlua::Function,
}

impl CallbackLease {
    pub(super) fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        invoke(&self.lua, &self.callback, input)
    }
}

impl LuaProgram {
    pub(super) fn compile(source: &str) -> mlua::Result<Self> {
        let lua = mlua::Lua::new();
        let module: mlua::Table = lua.load(source).set_name("schema_behaviors.lua").eval()?;
        // Readiness captures the actual callable and metadata, not a promise to look them up later.
        let callback = module.get("invoke")?;
        let bindings = lua.from_value(module.get("bindings")?)?;
        Ok(Self {
            lua,
            bindings,
            callback,
        })
    }

    pub(super) fn load(path: &std::path::Path) -> Result<Self, String> {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        Self::compile(&source).map_err(|error| error.to_string())
    }

    pub(super) fn observe_cleanup(&self) -> (mlua::WeakLua, Rc<Cell<bool>>) {
        let cleaned = Rc::new(Cell::new(false));
        // Keep the actual userdata in the registry, outside authored global/module tables.
        self.lua
            .set_named_registry_value(
                "trial.cleanup",
                self.lua
                    .create_userdata(CleanupProbe(Rc::clone(&cleaned)))
                    .unwrap(),
            )
            .unwrap();
        (self.lua.weak(), cleaned)
    }

    pub(super) fn retain_callback(&self) -> CallbackLease {
        CallbackLease {
            lua: self.lua.clone(),
            callback: self.callback.clone(),
        }
    }
}

fn invoke(
    lua: &mlua::Lua,
    callback: &mlua::Function,
    input: &Snapshot,
) -> Result<Vec<NameCommand>, String> {
    let input = lua.to_value(input).map_err(|error| error.to_string())?;
    let output = callback
        .call::<mlua::Value>(input)
        .map_err(|error| error.to_string())?;
    lua.from_value(output).map_err(|error| error.to_string())
}

impl Language for LuaProgram {
    fn describe(&self) -> Result<Vec<Binding>, String> {
        Ok(self.bindings.clone())
    }

    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        invoke(&self.lua, &self.callback, input)
    }
}

#[test]
fn a_bare_mlua_function_does_not_keep_its_vm_alive() {
    let program = LuaProgram::compile(include_str!("../../lua/schema_behaviors.lua")).unwrap();
    let (weak, cleaned) = program.observe_cleanup();
    let bare = program.callback.clone();
    drop(program);
    assert!(weak.try_upgrade().is_none());
    assert!(cleaned.get());
    // Do not call a value after its weak VM has expired; the safe lease owns Lua explicitly.
    drop(bare);
}
