use serde::{Deserialize, Serialize};
use wasmparser::{
    Encoding, ExternalKind, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures,
};

use crate::wasm_script::{WASM_MAX_MEMORY_BYTES, WASM_MAX_MODULE_BYTES, WasmScriptError};

const WASM_PAGE_BYTES: u64 = 64 * 1024;
const ALLOWED_IMPORTS: &[&str] = &["add_fixed", "get_fixed", "set_fixed"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmValidationReport {
    pub module_bytes: usize,
    pub import_count: usize,
    pub memory_count: usize,
    pub maximum_memory_bytes: usize,
    pub floats_allowed: bool,
    pub memory_growth_allowed: bool,
    pub mutable_globals_allowed: bool,
    pub threads_allowed: bool,
    pub wasi_allowed: bool,
}

pub(crate) fn strict_features() -> WasmFeatures {
    let mut features = WasmFeatures::MVP;
    features.remove(WasmFeatures::FLOATS);
    features
}

pub(crate) fn validate_module(
    module_id: &str,
    wasm_bytes: &[u8],
) -> Result<WasmValidationReport, WasmScriptError> {
    if wasm_bytes.is_empty() || wasm_bytes.len() > WASM_MAX_MODULE_BYTES {
        return validation_error(
            module_id,
            format!(
                "module bytes must be in 1..={WASM_MAX_MODULE_BYTES}, actual {}",
                wasm_bytes.len()
            ),
        );
    }

    Validator::new_with_features(strict_features())
        .validate_all(wasm_bytes)
        .map_err(|error| WasmScriptError::ValidationFailed {
            module_id: module_id.to_string(),
            reason: format!("strict MVP/no-float validation: {error}"),
        })?;

    let mut saw_core_module = false;
    let mut import_count = 0;
    let mut memory_count = 0;
    let mut maximum_memory_bytes = 0;
    let mut exported_memory = false;
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload.map_err(|error| WasmScriptError::ValidationFailed {
            module_id: module_id.to_string(),
            reason: format!("parse validated module: {error}"),
        })?;
        match payload {
            Payload::Version { encoding, .. } => {
                if encoding != Encoding::Module {
                    return validation_error(module_id, "components are not script modules");
                }
                saw_core_module = true;
            }
            Payload::ImportSection(groups) => {
                for group in groups {
                    let group = group.map_err(|error| WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read import group: {error}"),
                    })?;
                    for import in group {
                        let (_, import) =
                            import.map_err(|error| WasmScriptError::ValidationFailed {
                                module_id: module_id.to_string(),
                                reason: format!("read import: {error}"),
                            })?;
                        import_count += 1;
                        if import.module != "litd"
                            || !ALLOWED_IMPORTS.contains(&import.name)
                            || !matches!(import.ty, TypeRef::Func(_))
                        {
                            return Err(WasmScriptError::DeniedImport {
                                module_id: module_id.to_string(),
                                namespace: import.module.to_string(),
                                name: import.name.to_string(),
                            });
                        }
                    }
                }
            }
            Payload::MemorySection(memories) => {
                for memory in memories {
                    let memory = memory.map_err(|error| WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read memory declaration: {error}"),
                    })?;
                    memory_count += 1;
                    if memory_count > 1 {
                        return validation_error(module_id, "at most one linear memory is allowed");
                    }
                    if memory.memory64 || memory.shared || memory.page_size_log2.is_some() {
                        return validation_error(
                            module_id,
                            "memory64, shared memory, and custom page sizes are denied",
                        );
                    }
                    let Some(maximum) = memory.maximum else {
                        return validation_error(
                            module_id,
                            "linear memory must declare a maximum of one 64KiB page",
                        );
                    };
                    if memory.initial > 1 || maximum > 1 {
                        return validation_error(
                            module_id,
                            format!(
                                "linear memory exceeds {} byte cap (initial={} maximum={maximum})",
                                WASM_MAX_MEMORY_BYTES, memory.initial
                            ),
                        );
                    }
                    maximum_memory_bytes = (maximum * WASM_PAGE_BYTES) as usize;
                }
            }
            Payload::GlobalSection(globals) => {
                for global in globals {
                    let global = global.map_err(|error| WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read global declaration: {error}"),
                    })?;
                    if global.ty.mutable {
                        return validation_error(
                            module_id,
                            "mutable guest globals are denied; persistent script state must use the fixed-point host API",
                        );
                    }
                }
            }
            Payload::ExportSection(exports) => {
                for export in exports {
                    let export = export.map_err(|error| WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read export declaration: {error}"),
                    })?;
                    if export.kind == ExternalKind::Memory {
                        if exported_memory || export.name != "memory" || export.index != 0 {
                            return validation_error(
                                module_id,
                                "linear memory must have exactly one export named memory at index 0",
                            );
                        }
                        exported_memory = true;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let operators = body.get_operators_reader().map_err(|error| {
                    WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read function operators: {error}"),
                    }
                })?;
                for operator in operators {
                    let operator = operator.map_err(|error| WasmScriptError::ValidationFailed {
                        module_id: module_id.to_string(),
                        reason: format!("read function operator: {error}"),
                    })?;
                    if matches!(operator, Operator::MemoryGrow { .. }) {
                        return validation_error(
                            module_id,
                            "memory.grow is denied; linear memory size must remain fixed after preparation",
                        );
                    }
                }
            }
            Payload::TableSection(tables) if tables.count() != 0 => {
                return validation_error(module_id, "tables and indirect calls are denied");
            }
            Payload::StartSection { .. } => {
                return validation_error(
                    module_id,
                    "start functions are denied; invoke an explicit metered entry point",
                );
            }
            _ => {}
        }
    }
    if !saw_core_module {
        return validation_error(module_id, "missing core WebAssembly module header");
    }
    if memory_count != 0 && !exported_memory {
        return validation_error(
            module_id,
            "linear memory must be exported as memory so replay hashing and trap rollback cover every byte",
        );
    }
    Ok(WasmValidationReport {
        module_bytes: wasm_bytes.len(),
        import_count,
        memory_count,
        maximum_memory_bytes,
        floats_allowed: false,
        memory_growth_allowed: false,
        mutable_globals_allowed: false,
        threads_allowed: false,
        wasi_allowed: false,
    })
}

pub(crate) fn custom_section<'a>(
    wasm_bytes: &'a [u8],
    name: &str,
) -> Result<Option<&'a [u8]>, WasmScriptError> {
    let mut found = None;
    for payload in Parser::new(0).parse_all(wasm_bytes) {
        let payload = payload.map_err(|error| WasmScriptError::ValidationFailed {
            module_id: "custom_section_readback".to_string(),
            reason: error.to_string(),
        })?;
        if let Payload::CustomSection(section) = payload
            && section.name() == name
        {
            if found.is_some() {
                return Err(WasmScriptError::ArtifactMismatch {
                    module_id: "custom_section_readback".to_string(),
                    reason: format!("duplicate custom section {name}"),
                });
            }
            found = Some(section.data());
        }
    }
    Ok(found)
}

fn validation_error<T>(module_id: &str, reason: impl Into<String>) -> Result<T, WasmScriptError> {
    Err(WasmScriptError::ValidationFailed {
        module_id: module_id.to_string(),
        reason: reason.into(),
    })
}
