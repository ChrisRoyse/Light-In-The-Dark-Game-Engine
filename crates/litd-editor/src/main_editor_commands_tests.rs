use std::{fs, path::PathBuf};

use litd_editor::editor_commands::{
    COMMAND_MODE_TERRAIN, COMMAND_MODE_VFX, COMMAND_OPEN_PALETTE, EditorKeyChord,
};
use litd_editor::shell::build_editor_shell_plan;
use litd_engine::render::ui::UiRenderSettings;

use super::super::*;

fn state_path(runtime: &EditorShellRuntime) -> PathBuf {
    PathBuf::from(runtime.evidence().state_path)
}

fn read_state(runtime: &EditorShellRuntime) -> serde_json::Value {
    runtime.persisted_shell_state_json()
}

fn read_state_text(runtime: &EditorShellRuntime) -> String {
    String::from_utf8(runtime.persisted_shell_state_bytes())
        .expect("shell-state source of truth must be valid UTF-8")
}

fn active_mode(state: &serde_json::Value) -> &serde_json::Value {
    &state["persistence"]["active_mode"]
}

#[test]
fn palette_search_and_hotkey_invoke_same_command_operation() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let path = state_path(&runtime);
    let before = read_state(&runtime);
    println!(
        "issue1217 palette_hotkey before path={} active_mode={} save_count={}",
        path.display(),
        active_mode(&before),
        runtime.evidence().state_save_count
    );

    let palette = runtime
        .invoke_editor_command_search("vfx", "palette")
        .unwrap();
    let after_palette = read_state(&runtime);
    println!(
        "issue1217 palette_hotkey after_palette command={} active_mode={} source={}",
        palette.command_id,
        active_mode(&after_palette),
        runtime.last_command_source
    );

    runtime
        .invoke_editor_command_by_id(COMMAND_MODE_TERRAIN, "palette")
        .unwrap();
    let reset = read_state(&runtime);
    let hotkey = runtime
        .invoke_editor_keybinding(&EditorKeyChord::alt("5"))
        .unwrap();
    let after_hotkey = read_state(&runtime);
    println!(
        "issue1217 palette_hotkey after_hotkey command={} reset_mode={} active_mode={} source={}",
        hotkey.command_id,
        active_mode(&reset),
        active_mode(&after_hotkey),
        runtime.last_command_source
    );

    assert!(palette.known);
    assert!(hotkey.known);
    assert_eq!(palette.command_id, COMMAND_MODE_VFX);
    assert_eq!(hotkey.command_id, COMMAND_MODE_VFX);
    assert_eq!(active_mode(&after_palette), "Vfx");
    assert_eq!(active_mode(&reset), "Terrain");
    assert_eq!(active_mode(&after_hotkey), "Vfx");
    assert_eq!(runtime.command_invocation_count, 3);
}

#[test]
fn palette_enter_submits_first_vfx_match_and_persists_shell_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    runtime.open_command_palette();
    assert!(runtime.command_palette_focus_pending);
    runtime.command_palette_query = "vfx".to_string();
    let path = state_path(&runtime);
    let before = read_state(&runtime);
    println!(
        "issue1296 enter before path={} open={} active_mode={}",
        path.display(),
        runtime.command_palette_open,
        active_mode(&before)
    );

    let outcome = runtime
        .submit_command_palette_query("palette_enter")
        .unwrap();
    let after = read_state(&runtime);
    let evidence = runtime.evidence();
    println!(
        "issue1296 enter after command={} open={} active_mode={} inspector={} source={}",
        outcome.command_id,
        runtime.command_palette_open,
        active_mode(&after),
        evidence.active_inspector_title,
        runtime.last_command_source
    );

    assert!(outcome.known);
    assert_eq!(outcome.command_id, COMMAND_MODE_VFX);
    assert!(!runtime.command_palette_open);
    assert!(!runtime.command_palette_focus_pending);
    assert_eq!(active_mode(&before), "Terrain");
    assert_eq!(active_mode(&after), "Vfx");
    assert_eq!(evidence.active_inspector_title, "VFX Graph");
    assert_eq!(runtime.last_command_source, "palette_enter");
}

#[test]
fn palette_enter_empty_and_no_match_keep_state_unchanged() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = read_state_text(&runtime);
    runtime.open_command_palette();
    assert!(runtime.command_palette_focus_pending);
    runtime.command_palette_query = "  ".to_string();
    let empty = runtime
        .submit_command_palette_query("palette_enter")
        .unwrap();
    let after_empty = read_state_text(&runtime);
    println!(
        "issue1296 empty known={} open={} bytes_before={} bytes_after={}",
        empty.known,
        runtime.command_palette_open,
        before.len(),
        after_empty.len()
    );

    runtime.command_palette_query = "not-a-real-command".to_string();
    let no_match = runtime
        .submit_command_palette_query("palette_enter")
        .unwrap();
    let after_no_match = read_state_text(&runtime);
    println!(
        "issue1296 no_match known={} query={} open={} bytes_after={}",
        no_match.known,
        no_match.command_id,
        runtime.command_palette_open,
        after_no_match.len()
    );

    assert!(!empty.known);
    assert!(!no_match.known);
    assert_eq!(after_empty, before);
    assert_eq!(after_no_match, before);
    assert!(runtime.command_palette_open);
    assert!(runtime.command_palette_focus_pending);
    assert_eq!(runtime.command_invocation_count, 0);
}

#[test]
fn issue1296_palette_enter_fsv_writes_physical_readback() {
    let (_fsv_run, root) =
        litd_editor::fsv_support::artifact_dir("issue1296_command_palette_vfx", "manual").unwrap();
    fs::create_dir_all(&root).unwrap();
    let readback_path = root.join("command-palette-vfx-readback.json");

    let mut happy = EditorShellRuntime::new(1920, 1080).unwrap();
    let happy_before = read_state(&happy);
    happy.open_command_palette();
    happy.command_palette_query = "vfx".to_string();
    let happy_outcome = happy.submit_command_palette_query("palette_enter").unwrap();
    let happy_after = read_state(&happy);
    let happy_evidence = happy.evidence();

    let mut uppercase = EditorShellRuntime::new(1920, 1080).unwrap();
    uppercase.open_command_palette();
    uppercase.command_palette_query = "VFX".to_string();
    let uppercase_outcome = uppercase
        .submit_command_palette_query("palette_enter")
        .unwrap();
    let uppercase_after = read_state(&uppercase);

    let mut empty = EditorShellRuntime::new(1920, 1080).unwrap();
    let empty_before_text = read_state_text(&empty);
    empty.open_command_palette();
    empty.command_palette_query = String::new();
    let empty_outcome = empty.submit_command_palette_query("palette_enter").unwrap();
    let empty_after_text = read_state_text(&empty);

    let mut no_match = EditorShellRuntime::new(1920, 1080).unwrap();
    let no_match_before_text = read_state_text(&no_match);
    no_match.open_command_palette();
    no_match.command_palette_query = "zzzz-no-command".to_string();
    let no_match_outcome = no_match
        .submit_command_palette_query("palette_enter")
        .unwrap();
    let no_match_after_text = read_state_text(&no_match);

    let report = serde_json::json!({
        "issue": 1296,
        "source_of_truth": "persisted editor shell-state JSON files plus runtime evidence derived from the active shell plan",
        "happy": {
            "query": "vfx",
            "state_path": state_path(&happy),
            "before_active_mode": active_mode(&happy_before),
            "after_active_mode": active_mode(&happy_after),
            "after_inspector_title": happy_evidence.active_inspector_title,
            "command_id": happy_outcome.command_id,
            "palette_open_after": happy.command_palette_open,
            "state_save_count": happy.evidence().state_save_count
        },
        "edge_uppercase": {
            "query": "VFX",
            "state_path": state_path(&uppercase),
            "after_active_mode": active_mode(&uppercase_after),
            "command_id": uppercase_outcome.command_id
        },
        "edge_empty": {
            "known": empty_outcome.known,
            "before_bytes": empty_before_text.len(),
            "after_bytes": empty_after_text.len(),
            "state_unchanged": empty_before_text == empty_after_text,
            "palette_open_after": empty.command_palette_open
        },
        "edge_no_match": {
            "query": no_match_outcome.command_id,
            "known": no_match_outcome.known,
            "before_bytes": no_match_before_text.len(),
            "after_bytes": no_match_after_text.len(),
            "state_unchanged": no_match_before_text == no_match_after_text,
            "palette_open_after": no_match.command_palette_open
        }
    });
    fs::write(
        &readback_path,
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    let readback_text = fs::read_to_string(&readback_path).unwrap();
    let readback_json: serde_json::Value = serde_json::from_str(&readback_text).unwrap();
    println!(
        "issue1296 fsv path={} happy_mode={} empty_unchanged={} no_match_unchanged={}",
        readback_path.display(),
        readback_json["happy"]["after_active_mode"],
        readback_json["edge_empty"]["state_unchanged"],
        readback_json["edge_no_match"]["state_unchanged"]
    );

    assert_eq!(readback_json["happy"]["after_active_mode"], "Vfx");
    assert_eq!(readback_json["happy"]["after_inspector_title"], "VFX Graph");
    assert_eq!(readback_json["edge_uppercase"]["after_active_mode"], "Vfx");
    assert_eq!(readback_json["edge_empty"]["state_unchanged"], true);
    assert_eq!(readback_json["edge_no_match"]["state_unchanged"], true);
}

#[test]
fn unknown_command_is_noop_with_identical_state_file() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let path = state_path(&runtime);
    let before = runtime.persisted_shell_state_bytes();
    println!(
        "issue1217 unknown before path={} bytes={}",
        path.display(),
        before.len()
    );

    let outcome = runtime
        .invoke_editor_command_by_id("editor.command.missing", "palette")
        .unwrap();
    let after = runtime.persisted_shell_state_bytes();
    println!(
        "issue1217 unknown after known={} bytes={} invocations={}",
        outcome.known,
        after.len(),
        runtime.command_invocation_count
    );

    assert!(!outcome.known);
    assert_eq!(after, before);
    assert_eq!(runtime.command_invocation_count, 0);
}

#[test]
fn rebinding_command_persists_to_shell_state_and_reloads() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let path = state_path(&runtime);
    let before = read_state(&runtime);
    println!(
        "issue1217 rebind before path={} bindings={}",
        path.display(),
        before["command_keybindings"]["bindings"]
    );

    runtime
        .rebind_editor_command(COMMAND_MODE_VFX, EditorKeyChord::command("V"))
        .unwrap();
    let after = read_state(&runtime);
    println!(
        "issue1217 rebind after bindings={}",
        after["command_keybindings"]["bindings"]
    );

    assert!(
        after["command_keybindings"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|binding| {
                binding["command_id"] == COMMAND_MODE_VFX
                    && binding["key_chord"]["key"] == "V"
                    && binding["key_chord"]["command"] == true
            })
    );

    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let mut reloaded = ShellStateStore::new(path.clone());
    let _ = reloaded
        .load_or_initialize(settings, build_editor_shell_plan(settings).unwrap())
        .unwrap();
    assert!(
        reloaded
            .command_keybindings()
            .bindings
            .iter()
            .any(|binding| {
                binding.command_id == COMMAND_MODE_VFX
                    && binding.key_chord == EditorKeyChord::command("V")
            })
    );
    let _ = fs::remove_file(path);
}

#[test]
fn conflicting_persisted_keybindings_fail_loud_before_load() {
    let runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let path = state_path(&runtime);
    let mut state = read_state(&runtime);
    let bindings = state["command_keybindings"]["bindings"]
        .as_array_mut()
        .unwrap();
    let vfx = bindings
        .iter_mut()
        .find(|binding| binding["command_id"] == COMMAND_MODE_VFX)
        .expect("default VFX keybinding must exist");
    vfx["key_chord"] =
        serde_json::json!({"key": "F", "command": false, "shift": false, "alt": false});
    // Write the mutated state back through the Aster vault (the shell state is a vault
    // directory, not a flat file), then reload to prove the conflict fails loud (#1489).
    litd_editor::editor_state_store::EditorStateStore::new(path.clone())
        .put_shell_state(serde_json::to_string_pretty(&state).unwrap().as_bytes())
        .unwrap();
    println!(
        "issue1217 conflict before_load path={} bindings={}",
        path.display(),
        state["command_keybindings"]["bindings"]
    );

    let settings = UiRenderSettings {
        viewport_width: 1920.0,
        viewport_height: 1080.0,
    };
    let mut store = ShellStateStore::new(path.clone());
    match store.load_or_initialize(settings, build_editor_shell_plan(settings).unwrap()) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            println!("issue1217 conflict after_load field={field} reason={reason}");
            assert_eq!(field, "editor.commands.keybinding.conflict");
            assert!(reason.contains("F is bound to both"));
        }
        other => panic!("expected keybinding conflict failure, got {other:?}"),
    }
    let _ = fs::remove_dir_all(path);
}

#[test]
fn palette_open_command_is_registered_and_does_not_write_shell_state() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let before = runtime.persisted_shell_state_bytes();

    let outcome = runtime
        .invoke_editor_keybinding(&EditorKeyChord::command("K"))
        .unwrap();
    let after = runtime.persisted_shell_state_bytes();
    println!(
        "issue1217 open_palette command={} open={} bytes_before={} bytes_after={}",
        outcome.command_id,
        runtime.command_palette_open,
        before.len(),
        after.len()
    );

    assert_eq!(outcome.command_id, COMMAND_OPEN_PALETTE);
    assert!(runtime.command_palette_open);
    assert_eq!(after, before);
}
