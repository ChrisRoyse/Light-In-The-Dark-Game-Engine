use super::*;

#[test]
fn issue1315_headless_playtest_persists_failed_oracle_verdicts() {
    for case in [
        (
            "perf-budget",
            "oracle.max_visible_instances",
            "0",
            HarnessOracleKind::PerfBudget,
        ),
        (
            "physics-bound",
            "oracle.physics_max_abs_milli",
            "0",
            HarnessOracleKind::PhysicsBounds,
        ),
        (
            "reward-format",
            "reward_milli",
            "not-an-int",
            HarnessOracleKind::EconomyRewardSanity,
        ),
    ] {
        let (label, key, value, expected_oracle) = case;
        let root = test_root(&format!("issue1315-{label}"));
        clean_dir(&root);
        let state_path = root.join("project-state.json");
        let project_path = root.join("ashen-reach.litdworld.json");
        let package_path = root.join("unused.litdgame.json");
        let document = with_world_property(fixture_document(), key, value);

        let before = state_summary(&state_path, &project_path, &package_path);
        println!(
            "issue1315 edge={label} before {}",
            serde_json::to_string(&before).unwrap()
        );
        call_mcp_project_tool(
            NEW_OPEN_SAVE_TOOL,
            save_args(&state_path, &root, &project_path, document),
        )
        .unwrap();
        call_mcp_project_tool(
            RUN_HEADLESS_PLAYTEST_TOOL,
            playtest_args(
                &state_path,
                &root,
                &project_path,
                &format!("issue1315-{label}-playtest"),
            ),
        )
        .unwrap();

        let readback = read_mcp_project_state(&state_path).unwrap();
        let playtest = &readback.state.playtests[0];
        let failed_oracles = playtest
            .oracle_verdict
            .checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| check.oracle)
            .collect::<Vec<_>>();
        assert_eq!(playtest.verdict, "failed");
        assert_eq!(playtest.oracle_verdict.verdict, "failed");
        assert!(failed_oracles.contains(&expected_oracle));
        assert!(playtest.verdict_reason.contains("failed"));

        let after = state_summary(&state_path, &project_path, &package_path);
        println!(
            "issue1315 edge={label} after {} failed_oracles={:?} playtest={}",
            serde_json::to_string(&after).unwrap(),
            failed_oracles,
            serde_json::to_string(playtest).unwrap()
        );
    }
}

#[test]
fn issue1315_invalid_oracle_config_fails_without_playtest_write() {
    let root = test_root("issue1315-invalid-config");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("unused.litdgame.json");
    let document = with_world_property(fixture_document(), "oracle.max_reward_delta_milli", "-1");

    call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        save_args(&state_path, &root, &project_path, document),
    )
    .unwrap();
    let before = state_summary(&state_path, &project_path, &package_path);
    let error = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        playtest_args(
            &state_path,
            &root,
            &project_path,
            "issue1315-invalid-config-playtest",
        ),
    )
    .unwrap_err()
    .to_string();
    let after = state_summary(&state_path, &project_path, &package_path);
    assert_eq!(before, after);
    assert!(error.contains("oracle.max_reward_delta_milli"));
    println!(
        "issue1315 invalid_config before={} after={} error={}",
        before, after, error
    );
}

#[test]
fn issue1518_project_tool_rejects_out_of_root_paths_without_writing() {
    // #1518 confinement close gate for the core mcp_project_tools handlers
    // (new/open/save representative): a poisoned state_path (absolute-escape) and a
    // ..-traversal project_path are both rejected before any fs effect, leaving the
    // filesystem untouched. Delivery (#1487) and the ecosystem sub-tools (parts 4-9)
    // were already confined; this closes new/open/save, playtest, package, and read.
    let root = test_root("issue1518-confine");
    clean_dir(&root);
    let state_path = root.join("project-state.json");
    let project_path = root.join("ashen-reach.litdworld.json");
    let package_path = root.join("unused.litdgame.json");

    // --- EDGE 1: absolute state_path OUTSIDE the workspace root ---
    let _temp_root = litd_engine::test_temp::TestTempDir::new("issue1518-proj");
    let escape_dir = _temp_root.path().to_path_buf();
    if escape_dir.exists() {
        fs::remove_dir_all(&escape_dir).ok();
    }
    let before_escape = state_summary(&state_path, &project_path, &package_path);
    let mut poisoned = save_args(&state_path, &root, &project_path, fixture_document());
    poisoned["state_path"] = json!(escape_dir.join("project-state.json").display().to_string());
    let e1 = call_mcp_project_tool(NEW_OPEN_SAVE_TOOL, poisoned)
        .unwrap_err()
        .to_string();
    let after_escape = state_summary(&state_path, &project_path, &package_path);
    println!("issue1518 project absolute-escape rejected: {e1}");
    println!("issue1518 escape before={before_escape} after={after_escape}");
    assert!(
        e1.contains("outside the project root"),
        "expected outside-root rejection, got: {e1}"
    );
    assert!(
        !escape_dir.exists(),
        "no directory created outside the root"
    );
    assert_eq!(before_escape, after_escape, "no fs effect on escape reject");

    // --- EDGE 2: ..-traversal project_path ---
    let before_trav = state_summary(&state_path, &project_path, &package_path);
    let mut poisoned2 = save_args(&state_path, &root, &project_path, fixture_document());
    poisoned2["project_path"] = json!("worlds/../../litd-issue1518-proj-traversal.litdworld.json");
    let e2 = call_mcp_project_tool(NEW_OPEN_SAVE_TOOL, poisoned2)
        .unwrap_err()
        .to_string();
    let after_trav = state_summary(&state_path, &project_path, &package_path);
    println!("issue1518 project traversal rejected: {e2}");
    assert!(
        e2.contains("parent-directory"),
        "expected traversal rejection, got: {e2}"
    );
    assert!(
        !Path::new("../litd-issue1518-proj-traversal.litdworld.json").exists(),
        "no file created outside the root via traversal"
    );
    assert_eq!(before_trav, after_trav, "no fs effect on traversal reject");
}

/// Create a real directory reparse point on disk: a junction on Windows (no admin
/// required, unlike symlinks) or a directory symlink on POSIX. Panics loudly on
/// failure — a skipped setup would silently pass the security test.
fn make_dir_reparse_point(link: &Path, target: &Path) {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .status()
            .expect("spawn `cmd /C mklink /J`");
        assert!(
            status.success(),
            "mklink /J {} {} failed",
            link.display(),
            target.display()
        );
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).expect("create directory symlink");
    }
}

#[test]
fn issue1518_store_reverify_rejects_reparse_point_toctou_without_writing() {
    // #1518 part 3 bullet 3: the store write functions re-verify confinement IMMEDIATELY
    // before the Aster/archive write, closing the symlink/junction TOCTOU window a handler
    // check leaves open. Proven against a REAL on-disk reparse point (junction/symlink).
    use crate::mcp_project_tools::mcp_project_tools_validation::reverify_confined_before_write;

    let root = test_root("issue1518-toctou");
    clean_dir(&root);

    // Source of truth: an out-of-root directory a planted reparse point would redirect
    // writes into. It must stay empty — nothing may be written through the escape.
    let _outside_temp = litd_engine::test_temp::TestTempDir::new("issue1518-toctou");
    let outside = _outside_temp.path().to_path_buf();
    if outside.exists() {
        fs::remove_dir_all(&outside).ok();
    }
    fs::create_dir_all(&outside).unwrap();
    let outside_before: Vec<_> = fs::read_dir(&outside).unwrap().collect();
    assert!(outside_before.is_empty(), "escape target starts empty");

    // --- EDGE A: reparse point INSIDE root pointing OUT of root (the real escape) ---
    let escape_link = root.join("escape_junction");
    make_dir_reparse_point(&escape_link, &outside);
    let through_escape = escape_link.join("project-state.dir");
    let e_escape = reverify_confined_before_write("state_path", &through_escape)
        .expect_err("write through an out-of-root reparse point must be rejected");
    let e_escape = e_escape.to_string();
    println!("issue1518 toctou out-of-root reparse rejected: {e_escape}");
    assert!(
        e_escape.contains("outside the project root") || e_escape.contains("reparse point"),
        "expected confinement/reparse rejection, got: {e_escape}"
    );

    // --- EDGE B: reparse point INSIDE root pointing to another IN-root dir (policy) ---
    // canonicalize keeps it in-root, so confine_path alone would pass; the explicit
    // reparse-point rejection is what refuses it — confined tool writes never traverse a
    // reparse point at all.
    let in_root_target = root.join("real_dir");
    fs::create_dir_all(&in_root_target).unwrap();
    let in_root_link = root.join("in_root_junction");
    make_dir_reparse_point(&in_root_link, &in_root_target);
    let through_in_root = in_root_link.join("project-state.dir");
    let e_policy = reverify_confined_before_write("state_path", &through_in_root)
        .expect_err("write traversing an in-root reparse point must be rejected");
    let e_policy = e_policy.to_string();
    println!("issue1518 toctou in-root reparse rejected: {e_policy}");
    assert!(
        e_policy.contains("reparse point"),
        "expected reparse-point rejection, got: {e_policy}"
    );

    // --- EDGE C: a normal in-root path with no reparse component is accepted ---
    let clean = root.join("clean_state.dir");
    reverify_confined_before_write("state_path", &clean)
        .expect("a clean in-root path must pass the store re-verification");
    println!("issue1518 toctou clean in-root path accepted");

    // Source-of-truth check: the out-of-root escape target received nothing.
    let outside_after: Vec<_> = fs::read_dir(&outside).unwrap().collect();
    assert!(
        outside_after.is_empty(),
        "no bytes were written through the reparse point into {}",
        outside.display()
    );
    fs::remove_dir_all(&outside).ok();
}
