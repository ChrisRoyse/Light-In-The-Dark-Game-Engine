//! #899 first-launch onboarding FSV: pick template → one change → see it (headless SoT).
//!
//! Proves the WC3-style on-ramp without mocks:
//! 1. Browse real template gallery (MCP list_templates)
//! 2. One-click clone into a fresh project path
//! 3. Author one synthetic mutation on the clone (known region id)
//! 4. Independent on-disk readback: clone has mutation; template unchanged
//! 5. Headless starter play path (terminal result present)
//! 6. Three edge cases with before/after state printed
//!
//! Source of truth: physical clone project files + report.json sha256 (certutil).

use std::fs;
use std::path::Path;
use std::process::Command;

use litd_editor::first_launch_coach::{FIRST_LAUNCH_BUDGET_MS, FirstLaunchCoachSession};
use litd_editor::history::EditorRegion;
use litd_editor::mcp_template_tools::{
    CLONE_TEMPLATE_TOOL, LIST_TEMPLATES_TOOL, call_mcp_template_tool, read_document,
    read_document_fingerprint,
};
use litd_editor::mcp_world_tools::write_mcp_world_document;
use litd_editor::starter_templates::{
    StarterTemplateKind, new_project_from_template, run_starter_template_headless,
};
use litd_editor::template_gallery::MELEE_SKIRMISH_TEMPLATE;
use litd_engine::math::Vec3;
use serde_json::{Value, json};

const MUTATION_REGION: &str = "region.first_launch_win";
const SYNTHETIC_REGION_NAME: &str = "First Launch Win";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_editor::fsv_support::ensure_project_root_env();
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE899_FSV_ROOT",
        "issue899_first_launch",
        "manual",
    )?;
    if root.exists() {
        fs::remove_dir_all(&root)?;
    }
    fs::create_dir_all(&root)?;
    let report_path = root.join("issue899-first-launch-fsv.json");

    // --- Happy path: browse → clone → mutate → readback ---
    let listed = call_mcp_template_tool(LIST_TEMPLATES_TOOL, json!({}))?;
    let template_ids: Vec<String> = serde_json::from_value(listed.result["templates"].clone())
        .map(|v: Vec<Value>| {
            v.into_iter()
                .map(|t| t["template_id"].as_str().unwrap().to_string())
                .collect()
        })?;
    println!("issue899 browse templates={template_ids:?}");
    if template_ids.is_empty() {
        return Err("template gallery empty — first-launch cannot offer a pick".into());
    }
    if !template_ids.iter().any(|id| id == MELEE_SKIRMISH_TEMPLATE) {
        return Err(format!("expected starter {MELEE_SKIRMISH_TEMPLATE} in gallery").into());
    }

    let template_path = root.join("gallery-template-melee");
    let clone_path = root.join("my-first-project");
    let ledger = root.join("first-launch-ledger.json");

    let clone_out = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": template_path,
            "clone_path": clone_path,
            "clone_calyx_document_id": "calyx://project/first-launch-899",
            "clone_revision_id": "first-launch-rev-1",
            "ledger_path": ledger,
            "ledger_id": "first-launch-ledger",
            "request_id": "issue899-clone-1",
            "requested_by": "issue899-fsv",
        }),
    )?;
    let template_fp_before = read_document_fingerprint(&template_path)?;
    let clone_fp_before = clone_out.result["clone_fingerprint"]
        .as_str()
        .ok_or("clone_fingerprint missing")?
        .to_string();
    println!("issue899 clone_ok template_fp={template_fp_before} clone_fp={clone_fp_before}");

    // One synthetic change the author "makes" — known region id for SoT assert.
    let mut clone = read_document(&clone_path)?;
    let before_region_count = clone.regions.len();
    clone.upsert_region(EditorRegion::new(
        MUTATION_REGION,
        SYNTHETIC_REGION_NAME,
        Vec3::new(2.0, 0.0, 2.0),
        Vec3::new(1.0, 2.0, 1.0),
    ))?;
    write_mcp_world_document(&clone_path, &clone)?;
    let clone_after = read_document(&clone_path)?;
    let template_after = read_document(&template_path)?;
    let after_region_count = clone_after.regions.len();
    let mutation_present = clone_after.regions.contains_key(MUTATION_REGION);
    let template_untouched = !template_after.regions.contains_key(MUTATION_REGION)
        && read_document_fingerprint(&template_path)? == template_fp_before;
    println!(
        "issue899 mutate before_regions={before_region_count} after_regions={after_region_count} \
         mutation_present={mutation_present} template_untouched={template_untouched}"
    );
    if !mutation_present || !template_untouched || after_region_count <= before_region_count {
        return Err("first-launch mutation SoT failed".into());
    }

    // Headless "see it live" — starter simulation produces a terminal result (not a blank run).
    let starter = new_project_from_template(
        StarterTemplateKind::MeleeSkirmish,
        "first-launch-899",
        "First Launch Melee",
        "rev-1",
    )?;
    let sim = run_starter_template_headless(&starter, 48)?;
    println!(
        "issue899 headless_play terminal={:?} final_tick={}",
        sim.terminal_result, sim.final_tick
    );

    // Guided wizard chrome: coach session binds real tip targets on the clone document.
    let t0 = 1_700_000_000_000u64;
    let coach_path = root.join("first-launch-coach.json");
    let mut coach =
        FirstLaunchCoachSession::start_for_gallery_template(MELEE_SKIRMISH_TEMPLATE, t0)?;
    let tip0 = coach.current_tip().ok_or("coach missing tip0")?.clone();
    if !clone_after.regions.contains_key(&tip0.target_id)
        && !clone_after.objects.contains_key(&tip0.target_id)
    {
        // play.instant is chrome, not a document id — only first tips must bind.
        if tip0.surface != "play_mode" {
            return Err(format!(
                "coach tip0 target {} missing from clone document",
                tip0.target_id
            )
            .into());
        }
    }
    // Tip0 targets a real region on the melee clone.
    if !clone_after.regions.contains_key(&tip0.target_id) {
        return Err(format!(
            "coach tip0 region target {} not in clone regions {:?}",
            tip0.target_id,
            clone_after.regions.keys().collect::<Vec<_>>()
        )
        .into());
    }
    let tip1 = coach.advance()?.clone();
    if !clone_after.objects.contains_key(&tip1.target_id) {
        return Err(format!(
            "coach tip1 object target {} not in clone objects",
            tip1.target_id
        )
        .into());
    }
    let _tip2 = coach.advance()?.clone();
    coach.complete_final_tip()?;
    coach.write_to_path(&coach_path)?;
    // Independent SoT readback of persisted coach session.
    let coach_readback = FirstLaunchCoachSession::read_from_path(&coach_path)?;
    let snap = coach_readback.snapshot(t0 + 90_000);
    let coach_ok = coach_readback.completed
        && snap.within_budget
        && snap.tip_count >= 3
        && coach_readback.budget_ms == FIRST_LAUNCH_BUDGET_MS;
    println!(
        "issue899 coach completed={} within_budget={} tip_count={} status={}",
        coach_readback.completed, snap.within_budget, snap.tip_count, snap.status_line
    );
    if !coach_ok {
        return Err("first-launch coach SoT failed".into());
    }

    // --- Edge cases ---
    let edges = vec![
        edge_empty_gallery_pick(&root)?,
        edge_collision_second_clone(&root, &template_path, &clone_path, &ledger)?,
        edge_unknown_template(&root)?,
        edge_coach_empty_tips()?,
        edge_coach_budget_expired()?,
        edge_coach_unknown_template()?,
    ];

    let report = json!({
        "schema": "litd.issue899.first_launch_fsv.v1",
        "issue": 899,
        "parent_issues": [898, 901, 858],
        "source_of_truth": "clone project document on disk + template fingerprint + headless sim terminal + coach session JSON",
        "happy_path": {
            "template_ids": template_ids,
            "picked": MELEE_SKIRMISH_TEMPLATE,
            "template_fp_before": template_fp_before,
            "clone_fp_before": clone_fp_before,
            "mutation_region": MUTATION_REGION,
            "mutation_present": mutation_present,
            "template_untouched": template_untouched,
            "before_region_count": before_region_count,
            "after_region_count": after_region_count,
            "headless_final_tick": sim.final_tick,
            "headless_terminal": format!("{:?}", sim.terminal_result),
            "headless_deterministic": sim.deterministic,
            "coach_completed": coach_readback.completed,
            "coach_tip_count": snap.tip_count,
            "coach_within_budget": snap.within_budget,
            "coach_status_line": snap.status_line,
            "coach_path": coach_path.display().to_string(),
        },
        "edges": edges,
        "acceptance": {
            "gallery_nonempty": true,
            "clone_independent": template_untouched,
            "one_change_visible": mutation_present,
            "headless_play_ran": sim.final_tick > 0 && sim.deterministic,
            "coach_wizard_chrome": coach_ok,
            "edges_fail_loud": edges.iter().all(|e| e["ok"].as_bool() == Some(true)),
        }
    });

    let acceptance = &report["acceptance"];
    if acceptance["gallery_nonempty"] != true
        || acceptance["clone_independent"] != true
        || acceptance["one_change_visible"] != true
        || acceptance["headless_play_ran"] != true
        || acceptance["coach_wizard_chrome"] != true
        || acceptance["edges_fail_loud"] != true
    {
        return Err(format!("issue899 acceptance failed: {acceptance}").into());
    }

    fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
    let recorded = fsv_run.record_json_file(&report_path)?;
    let certutil = certutil_sha256(&report_path)?;
    if certutil != recorded.sha256 {
        return Err(format!(
            "sha256 mismatch fsv={} certutil={certutil}",
            recorded.sha256
        )
        .into());
    }
    println!(
        "ISSUE899_FIRST_LAUNCH_FSV VERDICT=GREEN report={} report_sha256={} certutil_sha256={}",
        report_path.display(),
        recorded.sha256,
        certutil
    );
    Ok(())
}

fn edge_empty_gallery_pick(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    // Empty clone identity must fail loud (before any write).
    let before = list_dir_names(root);
    let err = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": root.join("edge-empty-template"),
            "clone_path": root.join("edge-empty-clone"),
            "clone_calyx_document_id": "",
            "clone_revision_id": "",
            "ledger_path": root.join("edge-empty-ledger.json"),
            "ledger_id": "edge-empty",
            "request_id": "edge-empty",
            "requested_by": "issue899-fsv",
        }),
    )
    .err()
    .map(|e| e.to_string())
    .ok_or("empty identity should fail")?;
    let after = list_dir_names(root);
    println!("issue899 edge empty_identity before={before:?} after={after:?} err={err}");
    Ok(json!({
        "name": "empty_clone_identity",
        "ok": err.contains("clone") || err.contains("invalid") || err.contains("empty"),
        "error": err,
        "before_dirs": before,
        "after_dirs": after,
    }))
}

fn edge_collision_second_clone(
    _root: &Path,
    template_path: &Path,
    existing_clone: &Path,
    ledger: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let before_exists = existing_clone.exists();
    let err = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": MELEE_SKIRMISH_TEMPLATE,
            "template_path": template_path,
            "clone_path": existing_clone,
            "clone_calyx_document_id": "calyx://project/collide",
            "clone_revision_id": "collide-1",
            "ledger_path": ledger,
            "ledger_id": "first-launch-ledger",
            "request_id": "edge-collide",
            "requested_by": "issue899-fsv",
        }),
    )
    .err()
    .map(|e| e.to_string())
    .ok_or("collision should fail")?;
    println!("issue899 edge collision before_exists={before_exists} err={err}");
    Ok(json!({
        "name": "name_collision",
        "ok": before_exists && (err.contains("exists") || err.contains("overwrite") || err.contains("invalid")),
        "error": err,
        "before_clone_exists": before_exists,
    }))
}

fn edge_unknown_template(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let dest = root.join("edge-unknown-clone");
    let before = dest.exists();
    let err = call_mcp_template_tool(
        CLONE_TEMPLATE_TOOL,
        json!({
            "template_id": "template.does.not.exist",
            "template_path": root.join("edge-unknown-template"),
            "clone_path": dest,
            "clone_calyx_document_id": "calyx://project/unknown",
            "clone_revision_id": "unknown-1",
            "ledger_path": root.join("edge-unknown-ledger.json"),
            "ledger_id": "edge-unknown",
            "request_id": "edge-unknown",
            "requested_by": "issue899-fsv",
        }),
    )
    .err()
    .map(|e| e.to_string())
    .ok_or("unknown template should fail")?;
    let after = dest.exists();
    println!("issue899 edge unknown before={before} after={after} err={err}");
    Ok(json!({
        "name": "unknown_template",
        "ok": !after && (err.contains("not a known") || err.contains("invalid") || err.contains("template")),
        "error": err,
        "before_exists": before,
        "after_exists": after,
    }))
}

fn edge_coach_empty_tips() -> Result<Value, Box<dyn std::error::Error>> {
    let before = "tips=[]";
    let err = FirstLaunchCoachSession::start("template.empty", vec![], 0)
        .err()
        .map(|e| e.to_string())
        .ok_or("empty tips should fail")?;
    println!("issue899 edge coach_empty_tips before={before} after=err err={err}");
    Ok(json!({
        "name": "coach_empty_tips",
        "ok": err.contains("first-launch coach") || err.contains("tips"),
        "error": err,
        "before": before,
        "after": "rejected",
    }))
}

fn edge_coach_budget_expired() -> Result<Value, Box<dyn std::error::Error>> {
    let t0 = 0u64;
    let session = FirstLaunchCoachSession::start_for_gallery_template(MELEE_SKIRMISH_TEMPLATE, t0)?;
    let before_remaining = session.remaining_budget_ms(t0);
    let after_now = FIRST_LAUNCH_BUDGET_MS + 1;
    let after_remaining = session.remaining_budget_ms(after_now);
    let within = session.within_budget(after_now);
    println!(
        "issue899 edge coach_budget before_remaining={before_remaining} after_remaining={after_remaining} within={within}"
    );
    Ok(json!({
        "name": "coach_budget_expired",
        "ok": before_remaining == FIRST_LAUNCH_BUDGET_MS && after_remaining == 0 && !within,
        "before_remaining_ms": before_remaining,
        "after_remaining_ms": after_remaining,
        "within_budget_after": within,
    }))
}

fn edge_coach_unknown_template() -> Result<Value, Box<dyn std::error::Error>> {
    let before = "template_id=template.does.not.exist";
    let err = FirstLaunchCoachSession::start_for_gallery_template("template.does.not.exist", 0)
        .err()
        .map(|e| e.to_string())
        .ok_or("unknown coach template should fail")?;
    println!("issue899 edge coach_unknown before={before} after=err err={err}");
    Ok(json!({
        "name": "coach_unknown_template",
        "ok": err.contains("unknown template") || err.contains("first-launch"),
        "error": err,
        "before": before,
        "after": "rejected",
    }))
}

fn list_dir_names(root: &Path) -> Vec<String> {
    fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn certutil_sha256(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("certutil")
        .args(["-hashfile", &path.display().to_string(), "SHA256"])
        .output()?;
    if !out.status.success() {
        return Err(format!("certutil failed: {}", String::from_utf8_lossy(&out.stderr)).into());
    }
    let text = String::from_utf8(out.stdout)?;
    let hash = text
        .lines()
        .nth(1)
        .ok_or("certutil missing hash line")?
        .split_whitespace()
        .collect::<String>()
        .to_lowercase();
    Ok(hash)
}
