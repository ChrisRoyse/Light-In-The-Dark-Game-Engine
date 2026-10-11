//! Unit coverage for the #1455 startup content loader.
//!
//! The full author -> freeze -> parity success path (which needs the real measurement panel) is
//! proven end-to-end by `examples/issue1455_elf_runtime_slice_fsv.rs` against the CANONICAL vault.
//! These unit tests pin the guard/error contracts: an empty request and a missing row must both
//! error loudly with a structured code, never degrade to an empty-ok table.

use crate::test_temp::TestTempDir;

use super::{CONTENT_RUNTIME_EMPTY_REQUEST, FrozenContentTable, load_frozen_content_for_sim};
use crate::calyx_content::{EngineCalyxContentError, EngineCalyxContentStore, EngineContentKind};
use crate::calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig};

fn empty_vault_store(label: &str) -> EngineCalyxContentStore {
    // A vault directory that does not exist yet: opening validates config only, and a read against
    // a non-existent vault resolves to a missing row (not an error at open time). No measurement
    // panel / ONNX is touched because these tests never write.
    let vault_dir = TestTempDir::new(&format!("issue1455-runtime-{label}"));
    let database =
        EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir)).unwrap();
    EngineCalyxContentStore::new(database)
}

#[test]
fn freeze_empty_request_errors_with_structured_code() {
    let store = empty_vault_store("empty");
    let error = FrozenContentTable::freeze(&store, &[]).unwrap_err();
    match error {
        EngineCalyxContentError::InvalidContent { field, reason } => {
            assert_eq!(field, "ids");
            assert!(
                reason.contains(CONTENT_RUNTIME_EMPTY_REQUEST),
                "reason must carry the structured code, got {reason}"
            );
        }
        other => panic!("expected InvalidContent, got {other:?}"),
    }
}

#[test]
fn freeze_missing_row_errors_loud_not_empty_ok() {
    let store = empty_vault_store("missing");
    let ids = [(EngineContentKind::Unit, "race.elf".to_string())];
    let error = FrozenContentTable::freeze(&store, &ids).unwrap_err();
    // The row genuinely does not exist -> the loader must surface the miss, never hand back an
    // empty-but-ok table that a sim would silently treat as "no Elf".
    assert!(
        matches!(error, EngineCalyxContentError::Persistence(_)),
        "missing row must be a loud persistence error, got {error:?}"
    );
}

#[test]
fn load_frozen_content_for_sim_propagates_missing_row() {
    let store = empty_vault_store("propagate");
    let ids = [(EngineContentKind::Unit, "race.elf".to_string())];
    assert!(load_frozen_content_for_sim(&store, &ids).is_err());
}
