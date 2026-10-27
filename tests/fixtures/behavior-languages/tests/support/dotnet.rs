use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use nara_reflect::{ComponentSchemaCatalog, ComponentValue};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use super::{Binding, Language, NameCommand, Snapshot};

type ManagedEntry = unsafe extern "system" fn(*const u8, i32, *mut u8, i32) -> i32;

struct Bridge {
    callback: netcorehost::hostfxr::ManagedFunction<ManagedEntry>,
    _loader: netcorehost::hostfxr::AssemblyDelegateLoader,
    _context:
        netcorehost::hostfxr::HostfxrContext<netcorehost::hostfxr::InitializedForRuntimeConfig>,
}

impl Bridge {
    fn load() -> Self {
        use netcorehost::{hostfxr::Hostfxr, pdcstr, pdcstring::PdCString};
        let directory = PathBuf::from(
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
                pdcstr!("Control"),
            )
            .unwrap();
        Self {
            callback,
            _loader: loader,
            _context: context,
        }
    }

    fn call<T: Serialize, R: DeserializeOwned>(&self, request: &T) -> Result<R, String> {
        let input = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let length = i32::try_from(input.len()).map_err(|error| error.to_string())?;
        let mut output = vec![0_u8; 64 * 1024];
        // SAFETY: The permanent Bridge.Control has this exact platform ABI/signature,
        // bounds copies and catches exceptions. Buffers and bridge owners span this call.
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
            return Err("Invalid managed output length".into());
        }
        serde_json::from_slice(&output[..count]).map_err(|error| error.to_string())
    }
}

#[derive(Deserialize)]
struct ModuleId {
    module: u64,
}
#[derive(Deserialize)]
struct LeaseId {
    lease: u64,
}
#[derive(Deserialize)]
struct Completion {
    complete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct EnemyStatsRequest {
    pub(super) operation: String,
    pub(super) name: String,
    pub(super) health: i64,
    pub(super) armor: i64,
    pub(super) bonus: i64,
    pub(super) fault: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnemyStatsProposal {
    pub(super) title: String,
    pub(super) name: String,
    pub(super) writes: Vec<EnemyStatsWrite>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnemyStatsWrite {
    pub(super) field: String,
    pub(super) value: ComponentValue,
}

pub(super) struct DotnetProgram {
    bridge: Rc<Bridge>,
    module: u64,
    retiring: bool,
}

pub(super) struct CallbackLease {
    bridge: Rc<Bridge>,
    lease: u64,
}
pub(super) struct Retirement {
    bridge: Rc<Bridge>,
    module: u64,
}

impl CallbackLease {
    pub(super) fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        self.bridge
            .call(&json!({"operation":"probe_lease", "lease":self.lease, "snapshot":input}))
    }
}

impl Drop for CallbackLease {
    fn drop(&mut self) {
        let _: serde_json::Value = self
            .bridge
            .call(&json!({"operation":"release", "lease":self.lease}))
            .expect("Release a live private module lease");
    }
}

impl Retirement {
    pub(super) fn complete(&self) -> bool {
        self.bridge
            .call::<_, Completion>(&json!({"operation":"poll", "module":self.module}))
            .unwrap()
            .complete
    }

    pub(super) fn invocation_refused(&self, input: &Snapshot) -> bool {
        self.bridge
            .call::<_, Vec<NameCommand>>(
                &json!({"operation":"invoke", "module":self.module, "snapshot":input}),
            )
            .is_err()
    }
}

impl DotnetProgram {
    pub(super) fn load_default() -> Self {
        let path = PathBuf::from(
            std::env::var_os("NARA_BEHAVIOR_MODULE_V1").expect("Runner builds the authored module"),
        );
        Self::load(&path).unwrap()
    }

    pub(super) fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_bridge(Rc::new(Bridge::load()), path)
    }

    fn load_with_bridge(bridge: Rc<Bridge>, path: &Path) -> Result<Self, String> {
        let loaded: ModuleId = bridge.call(&json!({"operation":"load", "path":path}))?;
        Ok(Self {
            bridge,
            module: loaded.module,
            retiring: false,
        })
    }

    pub(super) fn candidate(&self, path: &Path) -> Result<Self, String> {
        Self::load_with_bridge(Rc::clone(&self.bridge), path)
    }

    pub(super) fn enemy_stats_catalog(&self) -> Result<ComponentSchemaCatalog, String> {
        let catalog: serde_json::Value = self
            .bridge
            .call(&json!({"operation":"enemy_stats_catalog", "module":self.module}))?;
        let encoded = serde_json::to_vec(&catalog).map_err(|error| error.to_string())?;
        ComponentSchemaCatalog::from_json_bytes(&encoded).map_err(|error| error.to_string())
    }

    pub(super) fn propose_enemy_stats(
        &self,
        request: &EnemyStatsRequest,
    ) -> Result<EnemyStatsProposal, String> {
        self.bridge.call(&json!({
            "operation":"enemy_stats_propose", "module":self.module, "enemyStats":request,
        }))
    }

    pub(super) fn retain_callback(&self) -> CallbackLease {
        let retained: LeaseId = self
            .bridge
            .call(&json!({"operation":"retain", "module":self.module}))
            .unwrap();
        CallbackLease {
            bridge: Rc::clone(&self.bridge),
            lease: retained.lease,
        }
    }

    pub(super) fn retire(&mut self) -> Retirement {
        let _: Completion = self
            .bridge
            .call(&json!({"operation":"retire", "module":self.module}))
            .unwrap();
        self.retiring = true;
        Retirement {
            bridge: Rc::clone(&self.bridge),
            module: self.module,
        }
    }
}

impl Language for DotnetProgram {
    fn describe(&self) -> Result<Vec<Binding>, String> {
        self.bridge
            .call(&json!({"operation":"describe", "module":self.module}))
    }
    fn invoke(&self, input: &Snapshot) -> Result<Vec<NameCommand>, String> {
        self.bridge
            .call(&json!({"operation":"invoke", "module":self.module, "snapshot":input}))
    }
}

impl Drop for DotnetProgram {
    fn drop(&mut self) {
        if !self.retiring {
            let _ = self.retire();
        }
        // Collection is deliberately observable through Retirement, never implied by Drop.
    }
}
