use litd_hash::Fnv1a64;
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Memory, Module, PoolingAllocationConfig, Store, Trap,
    TypedFunc, WasmFeatures,
};

use crate::wasm_script::{
    WASM_MAX_MEMORY_BYTES, WasmFixedState, WasmInvocationStats, WasmScriptBinding, WasmScriptError,
};

#[derive(Debug)]
struct HostState {
    fixed: WasmFixedState,
    violation: Option<WasmScriptError>,
}

pub struct WasmScriptRuntime {
    binding: WasmScriptBinding,
    _engine: Engine,
    _module: Module,
    _instance: Instance,
    store: Store<HostState>,
    entry: TypedFunc<(), ()>,
    memory: Option<Memory>,
    memory_rollback: Vec<u8>,
}

impl WasmScriptRuntime {
    pub fn prepare(
        binding: WasmScriptBinding,
        initial_state: WasmFixedState,
    ) -> Result<Self, WasmScriptError> {
        let validation = binding.validate()?;
        let module_id = binding.artifact.module_id.clone();
        let engine = deterministic_engine(&module_id)?;
        let module = Module::new(&engine, &binding.artifact.wasm_bytes).map_err(|error| {
            WasmScriptError::PreparationFailed {
                module_id: module_id.clone(),
                reason: format!("compile validated module: {error}"),
            }
        })?;
        let linker = fixed_point_linker(&engine, &module_id)?;
        let instance_pre = linker.instantiate_pre(&module).map_err(|error| {
            WasmScriptError::PreparationFailed {
                module_id: module_id.clone(),
                reason: format!("pre-link fixed-point imports: {error}"),
            }
        })?;
        let mut store = Store::new(
            &engine,
            HostState {
                fixed: initial_state,
                violation: None,
            },
        );
        let instance = instance_pre.instantiate(&mut store).map_err(|error| {
            WasmScriptError::PreparationFailed {
                module_id: module_id.clone(),
                reason: format!("instantiate pooled module: {error}"),
            }
        })?;
        let entry = instance
            .get_typed_func::<(), ()>(&mut store, &binding.entry_point)
            .map_err(|error| WasmScriptError::PreparationFailed {
                module_id: module_id.clone(),
                reason: format!(
                    "entry {} must exist with signature () -> (): {error}",
                    binding.entry_point
                ),
            })?;
        let memory = if validation.memory_count == 0 {
            None
        } else {
            Some(instance.get_memory(&mut store, "memory").ok_or_else(|| {
                WasmScriptError::PreparationFailed {
                    module_id: module_id.clone(),
                    reason: "validated linear memory export memory was not instantiated"
                        .to_string(),
                }
            })?)
        };
        let memory_rollback = memory
            .as_ref()
            .map_or_else(Vec::new, |memory| vec![0; memory.data_size(&store)]);
        Ok(Self {
            binding,
            _engine: engine,
            _module: module,
            _instance: instance,
            store,
            entry,
            memory,
            memory_rollback,
        })
    }

    pub fn invoke(&mut self) -> Result<WasmInvocationStats, WasmScriptError> {
        let before = self.store.data().fixed;
        self.snapshot_memory();
        self.store.data_mut().violation = None;
        self.store
            .set_fuel(self.binding.fuel_budget)
            .map_err(|error| self.execution_error(format!("set invocation fuel: {error}")))?;
        if let Err(error) = self.entry.call(&mut self.store, ()) {
            let host_violation = self.store.data_mut().violation.take();
            self.restore_state(before);
            if let Some(host_violation) = host_violation {
                return Err(self.execution_error(host_violation.to_string()));
            }
            if matches!(error.downcast_ref::<Trap>(), Some(Trap::OutOfFuel)) {
                return Err(WasmScriptError::FuelExhausted {
                    module_id: self.binding.artifact.module_id.clone(),
                    entry_point: self.binding.entry_point.clone(),
                    budget: self.binding.fuel_budget,
                });
            }
            return Err(self.execution_error(error.to_string()));
        }
        let fuel_remaining = match self.store.get_fuel() {
            Ok(fuel) => fuel,
            Err(error) => {
                self.restore_state(before);
                return Err(self.execution_error(format!("read remaining fuel: {error}")));
            }
        };
        Ok(WasmInvocationStats {
            fuel_budget: self.binding.fuel_budget,
            fuel_consumed: self.binding.fuel_budget.saturating_sub(fuel_remaining),
            fixed_state_hash: self.store.data().fixed.state_hash(),
            replay_state_hash: self.replay_state_hash(),
        })
    }

    pub fn state(&self) -> &WasmFixedState {
        &self.store.data().fixed
    }

    pub fn replace_state(&mut self, state: WasmFixedState) {
        self.store.data_mut().fixed = state;
    }

    pub fn binding(&self) -> &WasmScriptBinding {
        &self.binding
    }

    pub fn replay_state_hash(&self) -> u64 {
        let mut hash = Fnv1a64::new();
        hash.update(b"litd.wasm.replay-state.v1");
        hash.update(self.binding.artifact.module_id.as_bytes());
        hash.update(self.binding.artifact.wasm_sha256.as_bytes());
        hash.update_u64_le(u64::from(self.binding.artifact.abi_version));
        hash.update(self.binding.artifact.wasmtime_version.as_bytes());
        hash.update(self.binding.entry_point.as_bytes());
        hash.update_u64_le(self.binding.fuel_budget);
        hash.update_u64_le(self.store.data().fixed.state_hash());
        if let Some(memory) = &self.memory {
            let bytes = memory.data(&self.store);
            hash.update_u64_le(bytes.len() as u64);
            hash.update(bytes);
        } else {
            hash.update_u64_le(0);
        }
        hash.finish()
    }

    fn snapshot_memory(&mut self) {
        if let Some(memory) = &self.memory {
            self.memory_rollback
                .copy_from_slice(memory.data(&self.store));
        }
    }

    fn restore_state(&mut self, fixed: WasmFixedState) {
        self.store.data_mut().fixed = fixed;
        if let Some(memory) = &self.memory {
            memory
                .data_mut(&mut self.store)
                .copy_from_slice(&self.memory_rollback);
        }
    }

    fn execution_error(&self, reason: String) -> WasmScriptError {
        WasmScriptError::ExecutionTrapped {
            module_id: self.binding.artifact.module_id.clone(),
            entry_point: self.binding.entry_point.clone(),
            reason,
        }
    }
}

fn deterministic_engine(module_id: &str) -> Result<Engine, WasmScriptError> {
    Engine::tls_eager_initialize();
    let mut config = Config::new();
    config
        .consume_fuel(true)
        .epoch_interruption(false)
        .cranelift_nan_canonicalization(true)
        .wasm_features(WasmFeatures::all(), false)
        .wasm_features(WasmFeatures::FLOATS, false)
        .memory_reservation(WASM_MAX_MEMORY_BYTES as u64)
        .memory_reservation_for_growth(0)
        .memory_guard_size(0);

    let mut pool = PoolingAllocationConfig::new();
    pool.total_core_instances(1)
        .total_memories(1)
        .total_tables(0)
        .max_memories_per_module(1)
        .max_tables_per_module(0)
        .max_memory_size(WASM_MAX_MEMORY_BYTES)
        .max_unused_warm_slots(1);
    config.allocation_strategy(pool);
    Engine::new(&config).map_err(|error| WasmScriptError::PreparationFailed {
        module_id: module_id.to_string(),
        reason: format!("create deterministic Wasmtime engine: {error}"),
    })
}

fn fixed_point_linker(
    engine: &Engine,
    module_id: &str,
) -> Result<Linker<HostState>, WasmScriptError> {
    let mut linker = Linker::new(engine);
    linker
        .func_wrap(
            "litd",
            "get_fixed",
            |mut caller: Caller<'_, HostState>, slot: i32| -> wasmtime::Result<i64> {
                let result = caller.data().fixed.get_raw(slot);
                record_host_result(&mut caller, result)
            },
        )
        .and_then(|linker| {
            linker.func_wrap(
                "litd",
                "set_fixed",
                |mut caller: Caller<'_, HostState>, slot: i32, raw: i64| {
                    let result = caller.data_mut().fixed.set_raw(slot, raw);
                    record_host_result(&mut caller, result)
                },
            )
        })
        .and_then(|linker| {
            linker.func_wrap(
                "litd",
                "add_fixed",
                |mut caller: Caller<'_, HostState>, slot: i32, raw: i64| {
                    let result = caller.data_mut().fixed.add_raw(slot, raw);
                    record_host_result(&mut caller, result)
                },
            )
        })
        .map_err(|error| WasmScriptError::PreparationFailed {
            module_id: module_id.to_string(),
            reason: format!("define fixed-point host ABI: {error}"),
        })?;
    Ok(linker)
}

fn record_host_result<T>(
    caller: &mut Caller<'_, HostState>,
    result: Result<T, WasmScriptError>,
) -> wasmtime::Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            caller.data_mut().violation = Some(error);
            Err(wasmtime::format_err!(
                "litd fixed-point host import rejected"
            ))
        }
    }
}
