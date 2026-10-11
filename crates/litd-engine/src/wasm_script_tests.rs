use crate::{
    math::Fixed,
    wasm_script::{
        WasmFixedState, WasmScriptArtifact, WasmScriptBinding, WasmScriptError, WasmScriptRuntime,
    },
};

const ADD_THREE_WAT: &str = r#"
(module
  (import "litd" "add_fixed" (func $add_fixed (param i32 i64) (result i64)))
  (func (export "run")
    i32.const 0
    i64.const 12884901888
    call $add_fixed
    drop))
"#;

const INCREMENT_MEMORY_WAT: &str = r#"
(module
  (memory (export "memory") 1 1)
  (func (export "run")
    i32.const 0
    i32.const 0
    i32.load8_u
    i32.const 1
    i32.add
    i32.store8))
"#;

fn binding(module_id: &str, wat: &str, fuel_budget: u64) -> WasmScriptBinding {
    WasmScriptBinding {
        artifact: WasmScriptArtifact::from_wat(module_id, wat).unwrap(),
        entry_point: "run".to_string(),
        fuel_budget,
    }
}

fn initial_state() -> WasmFixedState {
    let mut state = WasmFixedState::default();
    state.set(0, Fixed::from_i32(2)).unwrap();
    state
}

#[test]
fn independent_runtimes_mutate_fixed_state_identically() {
    let binding = binding("add_three", ADD_THREE_WAT, 10_000);
    let mut first = WasmScriptRuntime::prepare(binding.clone(), initial_state()).unwrap();
    let mut second = WasmScriptRuntime::prepare(binding.clone(), initial_state()).unwrap();

    let first_stats = first.invoke().unwrap();
    let second_stats = second.invoke().unwrap();

    assert_eq!(first.state().get(0).unwrap(), Fixed::from_i32(5));
    assert_eq!(first.state(), second.state());
    assert_eq!(first_stats, second_stats);
    assert!(first_stats.fuel_consumed > 0);
    assert_eq!(binding.replay_identity(), first.binding().replay_identity());
}

#[test]
fn float_operator_is_rejected_at_validation() {
    let error = WasmScriptArtifact::from_wat(
        "float_denied",
        r#"(module (func (export "run") f32.const 1.0 drop))"#,
    )
    .unwrap_err();
    assert!(matches!(error, WasmScriptError::ValidationFailed { .. }));
    assert!(error.to_string().contains("no-float"));
}

#[test]
fn ambient_clock_import_is_rejected_loud() {
    let error = WasmScriptArtifact::from_wat(
        "clock_denied",
        r#"
        (module
          (import "wasi_snapshot_preview1" "clock_time_get"
            (func $clock (param i32 i64 i32) (result i32)))
          (func (export "run")))
        "#,
    )
    .unwrap_err();
    assert!(matches!(error, WasmScriptError::DeniedImport { .. }));
    assert!(
        error
            .to_string()
            .contains("wasi_snapshot_preview1.clock_time_get")
    );
}

#[test]
fn fuel_exhaustion_is_loud_and_transactional() {
    let binding = binding(
        "fuel_denied",
        r#"(module (func (export "run") (loop $forever br $forever)))"#,
        100,
    );
    let initial = initial_state();
    let mut runtime = WasmScriptRuntime::prepare(binding, initial).unwrap();
    let error = runtime.invoke().unwrap_err();

    assert!(matches!(error, WasmScriptError::FuelExhausted { .. }));
    assert!(error.to_string().contains("GATE_FAILED: step=script_fuel"));
    assert_eq!(runtime.state(), &initial);
}

#[test]
fn host_state_violation_traps_and_rolls_back() {
    let binding = binding(
        "slot_denied",
        r#"
        (module
          (import "litd" "set_fixed" (func $set_fixed (param i32 i64)))
          (func (export "run")
            i32.const 64
            i64.const 1
            call $set_fixed))
        "#,
        1_000,
    );
    let initial = initial_state();
    let mut runtime = WasmScriptRuntime::prepare(binding, initial).unwrap();
    let error = runtime.invoke().unwrap_err();

    assert!(matches!(error, WasmScriptError::ExecutionTrapped { .. }));
    assert!(
        error.to_string().contains("slot 64 is out of range"),
        "unexpected trap: {error:?}"
    );
    assert_eq!(runtime.state(), &initial);
}

#[test]
fn unbounded_or_oversized_memory_is_rejected() {
    for (module_id, wat) in [
        (
            "unbounded_memory",
            r#"(module (memory 1) (func (export "run")))"#,
        ),
        (
            "oversized_memory",
            r#"(module (memory 1 2) (func (export "run")))"#,
        ),
    ] {
        let error = WasmScriptArtifact::from_wat(module_id, wat).unwrap_err();
        assert!(matches!(error, WasmScriptError::ValidationFailed { .. }));
    }
}

#[test]
fn artifact_tamper_and_wrong_entry_fail_closed() {
    let mut artifact = WasmScriptArtifact::from_wat("tamper", ADD_THREE_WAT).unwrap();
    artifact.wasm_bytes.push(0);
    let error = artifact.validate().unwrap_err();
    assert!(matches!(error, WasmScriptError::ArtifactMismatch { .. }));

    let wrong_entry = WasmScriptBinding {
        artifact: WasmScriptArtifact::from_wat("wrong_entry", ADD_THREE_WAT).unwrap(),
        entry_point: "missing".to_string(),
        fuel_budget: 10_000,
    };
    let error = WasmScriptRuntime::prepare(wrong_entry, initial_state())
        .err()
        .expect("missing entry must fail preparation");
    assert!(matches!(error, WasmScriptError::PreparationFailed { .. }));
}

#[test]
fn custom_section_round_trips_as_artifact_content() {
    let metadata = br#"{"schema":"litd.gui.v1","ops":1}"#;
    let artifact = WasmScriptArtifact::from_wat_with_custom_section(
        "metadata",
        ADD_THREE_WAT,
        "litd.gui.v1",
        metadata,
    )
    .unwrap();
    assert_eq!(
        artifact.custom_section("litd.gui.v1").unwrap(),
        Some(metadata.as_slice())
    );
    assert_eq!(artifact.validate().unwrap().import_count, 1);
}

#[test]
fn fixed_guest_memory_is_hashed_and_deterministic() {
    let binding = binding("memory_counter", INCREMENT_MEMORY_WAT, 10_000);
    let mut first = WasmScriptRuntime::prepare(binding.clone(), WasmFixedState::default()).unwrap();
    let mut second = WasmScriptRuntime::prepare(binding, WasmFixedState::default()).unwrap();
    let initial_hash = first.replay_state_hash();

    let first_stats = first.invoke().unwrap();
    let second_stats = second.invoke().unwrap();

    assert_eq!(first_stats, second_stats);
    assert_ne!(first_stats.replay_state_hash, initial_hash);
    assert_eq!(
        first_stats.fixed_state_hash,
        WasmFixedState::default().state_hash()
    );
    assert_ne!(
        first.invoke().unwrap().replay_state_hash,
        first_stats.replay_state_hash
    );
}

#[test]
fn fuel_trap_rolls_back_guest_memory() {
    let binding = binding(
        "memory_rollback",
        r#"
        (module
          (memory (export "memory") 1 1)
          (func (export "run")
            i32.const 0
            i32.const 42
            i32.store8
            (loop $forever br $forever)))
        "#,
        100,
    );
    let mut runtime = WasmScriptRuntime::prepare(binding, WasmFixedState::default()).unwrap();
    let before = runtime.replay_state_hash();

    assert!(matches!(
        runtime.invoke().unwrap_err(),
        WasmScriptError::FuelExhausted { .. }
    ));
    assert_eq!(runtime.replay_state_hash(), before);
}

#[test]
fn untracked_guest_state_is_rejected() {
    for (module_id, wat, expected) in [
        (
            "mutable_global",
            r#"(module (global (mut i32) (i32.const 0)) (func (export "run")))"#,
            "mutable guest globals are denied",
        ),
        (
            "growing_memory",
            r#"(module (memory (export "memory") 0 1) (func (export "run") i32.const 1 memory.grow drop))"#,
            "memory.grow is denied",
        ),
        (
            "hidden_memory",
            r#"(module (memory 1 1) (func (export "run")))"#,
            "must be exported as memory",
        ),
    ] {
        let error = WasmScriptArtifact::from_wat(module_id, wat)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(expected),
            "unexpected {module_id} error: {error}"
        );
    }
}
