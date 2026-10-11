use std::fs;

use litd_engine::app::LifecycleReport;

use crate::{
    main_build_identity::BuildIdentity, main_fsv::write_fsv_lifecycle_stats,
    main_lifecycle::EditorLifecycleEvidence, main_test_support::editor_exe_for_tests,
};

#[test]
fn lifecycle_stats_without_snapshot_write_failure_source_of_truth() {
    // #1535: TestTempDir auto-cleans on Drop (was a leaking raw env::temp_dir()).
    let _temp_root = litd_engine::test_temp::TestTempDir::new("editor-lifecycle-fsv");
    let dir = _temp_root.path().to_path_buf();
    let path = dir.join("stats.txt");
    let _ = fs::remove_file(&path);

    // #1559: worktree-portable fixture — resolve the exe/repo-root at RUNTIME from the primary
    // checkout instead of the old hardcoded machine-specific checkout-root literals.
    let repo_root = crate::main_test_support::repo_root_for_tests();
    let build = BuildIdentity {
        package_version: "0.0.1",
        build_git_commit: "0123456789abcdef0123456789abcdef01234567",
        build_git_dirty: true,
        build_profile: "debug",
        build_target: "x86_64-pc-windows-msvc",
        build_timestamp_unix_seconds: "1782720000",
        executable_path: editor_exe_for_tests(&repo_root),
        executable_sha256: "abc123".to_string(),
        repo_root: Some(repo_root.clone()),
        repo_head_commit: Some("0123456789abcdef0123456789abcdef01234567".to_string()),
        repo_head_matches_build: Some(true),
    };
    let lifecycle = LifecycleReport {
        module_count: 1,
        started: false,
        clean_shutdown: false,
        module_start_calls: 0,
        module_fixed_update_calls: 0,
        module_update_calls: 0,
        module_shutdown_calls: 0,
    };
    let editor_lifecycle = EditorLifecycleEvidence {
        state: "process_returned",
        last_exit_reason: "renderer_init_failed",
        close_requested_count: 0,
        loop_exiting_seen: true,
        shutdown_started_count: 1,
        shutdown_finished_count: 1,
        process_return_seen: true,
        process_exit_code: "failure",
    };

    write_fsv_lifecycle_stats(
        &path,
        "Light in the Dark Editor - world view",
        &build,
        &lifecycle,
        &editor_lifecycle,
    )
    .unwrap();

    let stats = fs::read_to_string(&path).unwrap();
    assert!(stats.contains("fsv_snapshot_available=false"));
    assert!(stats.contains("fsv_snapshot_missing_reason=no_rendered_frame_snapshot"));
    assert!(stats.contains("module_currently_started=false"));
    assert!(stats.contains("module_ever_started=false"));
    assert!(stats.contains("editor_last_exit_reason=renderer_init_failed"));
    assert!(stats.contains("editor_process_return_seen=true"));
    assert!(stats.contains("editor_process_exit_code=failure"));

    let _ = fs::remove_file(path);
    let _ = fs::remove_dir(dir);
}
