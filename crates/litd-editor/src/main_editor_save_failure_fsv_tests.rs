// FSV for #1489: on a forced save failure the editor must NOT diverge from disk and
// must surface the error to the user (not log-only). This drives the real editor
// runtime, breaks its Aster shell-state vault so every persist fails for real, and
// reads the persisted source of truth back to prove UI/memory and disk agree.

use super::main_editor_world_layers::WorldLayerToggle;
use super::*;
use std::{fs, path::Path};

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

#[test]
fn issue1489_forced_save_failure_reverts_ui_to_match_disk_and_surfaces_error() {
    let mut runtime = EditorShellRuntime::new(1920, 1080).unwrap();
    let vault_dir = std::path::PathBuf::from(runtime.evidence().state_path);

    // Durable baseline: foliage OFF, persisted successfully to the Aster vault.
    runtime
        .set_world_layer_visible(WorldLayerToggle::Foliage, false)
        .unwrap();
    assert!(!runtime.world_layers.is_enabled(WorldLayerToggle::Foliage));
    let disk_before = runtime.persisted_shell_state_json();
    assert_eq!(
        disk_before["world_layers"]["foliage"],
        serde_json::json!(false),
        "baseline foliage=false must be on disk"
    );

    // Back up the durable vault, then BREAK it: a regular file where the vault directory
    // must be makes every subsequent Aster open/write fail for real (no mock).
    let backup = vault_dir.with_extension("issue1489-backup");
    let _ = fs::remove_dir_all(&backup);
    copy_dir_all(&vault_dir, &backup);
    fs::remove_dir_all(&vault_dir).unwrap();
    fs::write(&vault_dir, b"not-an-aster-vault").unwrap();
    assert!(
        vault_dir.is_file(),
        "vault path is now a file so writes fail"
    );

    // Toggle foliage ON via the real command path. The save fails; the fix keeps the
    // in-memory layer matching disk (save-then-commit) instead of flipping optimistically.
    let toggle_result =
        runtime.invoke_editor_command_by_id("editor.view.toggle_foliage", "palette");
    assert!(
        toggle_result.is_err(),
        "toggle command must fail when the store write fails, got {toggle_result:?}"
    );
    assert!(
        !runtime.world_layers.is_enabled(WorldLayerToggle::Foliage),
        "world layer must NOT flip in memory when the save fails"
    );

    // The failure is surfaced to the user (status bar), not just logged.
    let status = runtime.status_summary();
    assert!(
        status.error_count >= 1,
        "status bar must report at least the command error, got {status:?}"
    );
    assert!(
        status.message.contains("Command"),
        "status must name the failed command: {}",
        status.message
    );
    assert!(
        runtime.last_command_error_message().is_some(),
        "last command error must be set for the status bar"
    );

    // Add a camera bookmark. The save fails; the optimistic in-memory bookmark must roll
    // back so it can't linger while disk keeps the old value.
    let bookmarks_before = runtime.shell_state.camera_bookmark_count();
    let bookmark_result = runtime.save_world_view_camera_bookmark(1);
    assert!(
        bookmark_result.is_err(),
        "bookmark save must fail when the store write fails, got {bookmark_result:?}"
    );
    assert_eq!(
        runtime.shell_state.camera_bookmark_count(),
        bookmarks_before,
        "bookmark must roll back in memory when the save fails"
    );

    // Restore the durable vault and read the source of truth: it never received the
    // failed writes, and the in-memory UI state agrees with it.
    fs::remove_file(&vault_dir).unwrap();
    copy_dir_all(&backup, &vault_dir);
    let disk_after = runtime.persisted_shell_state_json();
    assert_eq!(
        disk_after, disk_before,
        "disk must be byte-identical: the failed writes never reached it"
    );
    assert_eq!(
        runtime.world_layers.is_enabled(WorldLayerToggle::Foliage),
        disk_after["world_layers"]["foliage"].as_bool().unwrap(),
        "in-memory layer state must equal the persisted source of truth"
    );

    println!(
        "ISSUE1489_FSV vault={} disk_foliage={} mem_foliage={} status_errors={} bookmarks_mem={} status_message={}",
        vault_dir.display(),
        disk_after["world_layers"]["foliage"],
        runtime.world_layers.is_enabled(WorldLayerToggle::Foliage),
        status.error_count,
        runtime.shell_state.camera_bookmark_count(),
        status.message,
    );

    let _ = fs::remove_dir_all(&backup);
    let _ = fs::remove_dir_all(&vault_dir);
}
