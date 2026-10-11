use std::fs;

use litd_editor::{
    history::stable_fingerprint,
    mcp_project_tools::{
        NEW_OPEN_SAVE_TOOL, ProjectLifecycleAction, READ_PROJECT_TOOL, RUN_HEADLESS_PLAYTEST_TOOL,
        call_mcp_project_tool, inspect_litdworld_archive,
    },
    play::PlayCameraRig,
    starter_templates::{
        STARTER_TEMPLATE_KINDS, StarterTemplateKind, StarterTerminalResult,
        new_project_from_template, run_starter_template_headless, starter_template,
        starter_template_catalog, validate_starter_template,
    },
};
use serde_json::{Value, json};

#[path = "support/editor_starter_templates_issue858_fsv_support.rs"]
mod editor_starter_templates_issue858_fsv_support;

use editor_starter_templates_issue858_fsv_support::{
    Paths, clone_independence_edge, file_sha256, invalid_stat_rejected_edge, safe_kind,
    unknown_template_id_rejected_edge, unsupported_object_rejected_edge, wasm_escape_rejected_edge,
};

const EXPECTED_TEMPLATE_IDS: &[&str] = &[
    "starter.hero_arena.sunspire_duel",
    "starter.hero_defense.last_brazier",
    "starter.maze_escape.gloomroot_path",
    "starter.melee_skirmish.frost_crossing",
    "starter.mini_game_party.crystal_dash",
    "starter.moba_aos.dawnward_crossing",
    "starter.orpg.silverleaf_chronicle",
    "starter.rpg_arena.crypt_ring",
    "starter.survival_chaos.obsidian_lanes",
    "starter.tower_defense.cinder_lane",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    // MCP project/playtest tools require LITD_EDITOR_PROJECT_ROOT (#858 FSV).
    litd_editor::fsv_support::ensure_project_root_env();
    let (run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_ISSUE858_FSV_ROOT",
        "issue858_starter_templates_full_range",
        "manual",
    )?;
    let paths = Paths::new(root);
    let catalog = starter_template_catalog()?;
    let mut catalog_ids = catalog
        .iter()
        .map(|template| template.template_id.clone())
        .collect::<Vec<_>>();
    catalog_ids.sort();
    let catalog_matches_expected = catalog_ids == EXPECTED_TEMPLATE_IDS.to_vec();
    let mut template_reports = Vec::new();
    for kind in STARTER_TEMPLATE_KINDS.iter().copied() {
        template_reports.push(verify_template(&paths, kind)?);
    }
    let edges = vec![
        clone_independence_edge()?,
        wasm_escape_rejected_edge()?,
        unsupported_object_rejected_edge()?,
        invalid_stat_rejected_edge()?,
        unknown_template_id_rejected_edge()?,
    ];
    let state_readback =
        call_mcp_project_tool(READ_PROJECT_TOOL, json!({"state_path": paths.state}))?;
    let acceptance = acceptance(&template_reports, &edges, catalog_matches_expected);
    if !acceptance["passed"].as_bool().unwrap_or(false) {
        return Err("issue858 FSV acceptance predicates failed".into());
    }
    let report = json!({
        "schema": "litd.issue858.starter_templates_full_range_fsv.v1",
        "issue": 858,
        "parent_issues": [805, 741],
        "source_of_truth": "physical .litdworld ZIP archives with ZIP magic/readback, Calyx Aster MCP project/playtest state, starter-template validation, and deterministic headless starter simulation reports",
        "research_sources": [
            "https://www.hiveworkshop.com/threads/top-100-most-popular-warcraft-3-maps.291952/",
            "https://wc3maps.com/",
            "https://www.hiveworkshop.com/repositories/maps.564/"
        ],
        "readme_references_inspected": [
            "docs/readme/ED01-editor-shell.png",
            "docs/readme/ED04-trigger-editor.png",
            "docs/readme/ED09-test-in-editor.png",
            "docs/readme/ED11-world-view.png",
            "docs/readme/ED12-play-topdown.png",
            "docs/readme/GP01-town.png",
            "docs/readme/GP05-dungeon.png",
            "docs/readme/GP06-pvp.png",
            "docs/readme/GP11-quest.png",
            "docs/readme/GP20-talents.png"
        ],
        "genre_range": genre_range(),
        "paths": paths.to_json(),
        "catalog": {
            "count": catalog.len(),
            "ids_sorted": catalog_ids,
            "expected_ids_sorted": EXPECTED_TEMPLATE_IDS,
            "matches_expected": catalog_matches_expected
        },
        "acceptance": acceptance,
        "templates": template_reports,
        "edges": edges,
        "project_state": {
            "path": state_readback.state_readback.path,
            "sha256": state_readback.state_readback.sha256,
            "project_count": state_readback.state_readback.project_count,
            "playtest_count": state_readback.state_readback.playtest_count,
            "package_count": state_readback.state_readback.package_count
        }
    });
    let readback =
        run.write_json_artifact("manual/issue858-starter-templates-fsv.json", &report)?;
    println!(
        "ISSUE858_STARTER_TEMPLATES_FSV report={} report_sha256={} templates={} catalog_match={} terminal={} deterministic={} edges={}",
        readback.path.display(),
        readback.sha256,
        template_reports.len(),
        catalog_matches_expected,
        acceptance["all_templates_terminal_win"],
        acceptance["all_templates_deterministic"],
        acceptance["all_edges_passed"]
    );
    Ok(())
}

fn verify_template(
    paths: &Paths,
    kind: StarterTemplateKind,
) -> Result<Value, Box<dyn std::error::Error>> {
    let safe = safe_kind(kind);
    let project_id = format!("issue858.{safe}");
    let title = format!("Issue 858 {}", kind.default_title());
    let template = starter_template(kind)?;
    let validation = validate_starter_template(&template)?;
    let clone = new_project_from_template(kind, &project_id, &title, "issue858-template-rev-1")?;
    let project_path = paths.project_path(&safe);
    let save_readback = call_mcp_project_tool(
        NEW_OPEN_SAVE_TOOL,
        json!({
            "request_id": format!("issue858-save-{safe}"),
            "requested_by": "litd-editor-issue858-fsv",
            "state_path": paths.state,
            "state_id": "issue858-starter-template-state",
            "ledger_path": paths.root.join(format!("{safe}-save-ledger")),
            "ledger_id": format!("issue858-{safe}-save-ledger"),
            "action": ProjectLifecycleAction::New,
            "project_id": project_id,
            "title": title,
            "version": "0.1.0",
            "tags": ["starter-template", safe.as_str(), "issue858"],
            "project_path": project_path,
            "snapshot_label": format!("{safe}-new-from-template"),
            "document": clone.project.document
        }),
    )?;
    let archive = inspect_litdworld_archive(&project_path)?;
    let file_bytes = fs::metadata(&project_path)?.len();
    let playtest_readback = call_mcp_project_tool(
        RUN_HEADLESS_PLAYTEST_TOOL,
        json!({
            "request_id": format!("issue858-play-{safe}"),
            "requested_by": "litd-editor-issue858-fsv",
            "state_path": paths.state,
            "state_id": "issue858-starter-template-state",
            "ledger_path": paths.root.join(format!("{safe}-play-ledger")),
            "ledger_id": format!("issue858-{safe}-play-ledger"),
            "project_path": project_path,
            "camera_rig": PlayCameraRig::TopDown,
            "ticks": 96
        }),
    )?;
    let starter_run = run_starter_template_headless(&clone, 512)?;
    if starter_run.terminal_result != StarterTerminalResult::Win || !starter_run.deterministic {
        return Err(format!("starter run failed acceptance for {safe}").into());
    }
    let archive_document_fingerprint = archive.project.document_fingerprint.clone();
    if archive_document_fingerprint != stable_fingerprint(&clone.project.document)? {
        return Err(format!("archive document fingerprint mismatch for {safe}").into());
    }
    Ok(json!({
        "kind": format!("{kind:?}"),
        "template_id": template.template_id,
        "title": template.title,
        "copy_tweak_summary": template.copy_tweak_summary,
        "validation": {
            "supported_data_only": validation.supported_data_only,
            "no_wasm_escape": validation.no_wasm_escape,
            "object_count": validation.object_count,
            "unit_count": validation.unit_count,
            "region_count": validation.region_count,
            "graph_count": validation.graph_count,
            "event_count": validation.event_count,
            "condition_count": validation.condition_count,
            "action_count": validation.action_count,
            "supported_action_counts": validation.supported_action_counts,
            "tooltip_count": validation.tooltip_count,
            "document_fingerprint": validation.document_fingerprint,
            "trigger_fingerprint": validation.trigger_fingerprint,
            "scenario_fingerprint": validation.scenario_fingerprint,
            "world_validation_errors": validation.world_validation.error_count
        },
        "clone": {
            "project_id": clone.project.project_id,
            "template_fingerprint": clone.template_fingerprint,
            "clone_fingerprint": clone.clone_fingerprint,
            "clone_independent": clone.clone_independent,
            "tooltip_count": clone.guided_first_edit_tooltips.len()
        },
        "archive": {
            "path": archive.path,
            "bytes": archive.bytes,
            "file_bytes": file_bytes,
            "sha256": archive.sha256,
            "file_sha256": file_sha256(&project_path)?,
            "zip_magic": archive.zip_magic,
            "zip_entry_count": archive.zip_entry_count,
            "zip_entries": archive.zip_entries,
            "manifest_project_sha256": archive.manifest.project_sha256,
            "manifest_document_fingerprint": archive.manifest.document_fingerprint,
            "project_id": archive.project.project_id,
            "document_fingerprint": archive_document_fingerprint
        },
        "mcp_state": {
            "save_state_sha256": save_readback.state_readback.sha256,
            "save_project_count": save_readback.state_readback.project_count,
            "playtest_state_sha256": playtest_readback.state_readback.sha256,
            "playtest_count": playtest_readback.state_readback.playtest_count,
            "playtest": playtest_readback.result["playtest"]
        },
        "starter_run": starter_run
    }))
}

fn acceptance(
    template_reports: &[Value],
    edges: &[Value],
    catalog_matches_expected: bool,
) -> Value {
    let all_supported = template_reports.iter().all(|report| {
        report["validation"]["supported_data_only"] == true
            && report["validation"]["no_wasm_escape"] == true
            && report["validation"]["world_validation_errors"] == 0
    });
    let all_archives = template_reports.iter().all(|report| {
        report["archive"]["zip_magic"] == "PK0304"
            && report["archive"]["zip_entry_count"].as_u64().unwrap_or(0) >= 2
            && report["archive"]["file_bytes"].as_u64().unwrap_or(0) > 0
    });
    let all_terminal = template_reports.iter().all(|report| {
        report["starter_run"]["terminal_result"] == "win"
            && report["starter_run"]["executed_trigger_ids"]
                .as_array()
                .is_some_and(|ids| {
                    ids.iter()
                        .any(|id| id.as_str().unwrap_or("").contains("victory"))
                })
    });
    let all_deterministic = template_reports.iter().all(|report| {
        report["starter_run"]["deterministic"] == true
            && report["starter_run"]["final_state_hash"]
                == report["starter_run"]["replay_state_hash"]
    });
    let all_visible = template_reports.iter().all(|report| {
        report["starter_run"]["visible_instance_count"]
            .as_u64()
            .unwrap_or(0)
            > 0
            && report["starter_run"]["draw_batches"].as_u64().unwrap_or(0) > 0
            && report["mcp_state"]["playtest"]["visible_instance_count"]
                .as_u64()
                .unwrap_or(0)
                > 0
            && report["mcp_state"]["playtest"]["draw_batches"]
                .as_u64()
                .unwrap_or(0)
                > 0
    });
    let all_clones = template_reports
        .iter()
        .all(|report| report["clone"]["clone_independent"] == true);
    let all_edges = edges.iter().all(|edge| edge["passed"] == true);
    let passed = catalog_matches_expected
        && template_reports.len() == EXPECTED_TEMPLATE_IDS.len()
        && all_supported
        && all_archives
        && all_terminal
        && all_deterministic
        && all_visible
        && all_clones
        && all_edges;
    json!({
        "passed": passed,
        "catalog_matches_expected": catalog_matches_expected,
        "template_count": template_reports.len(),
        "all_templates_supported_data_only": all_supported,
        "all_archives_zip_readback": all_archives,
        "all_templates_terminal_win": all_terminal,
        "all_templates_deterministic": all_deterministic,
        "all_headless_playtests_visible": all_visible,
        "all_clones_independent": all_clones,
        "all_edges_passed": all_edges
    })
}

fn genre_range() -> Value {
    json!({
        "melee_rts": "starter.melee_skirmish.frost_crossing",
        "moba_aos": "starter.moba_aos.dawnward_crossing",
        "tower_defense": "starter.tower_defense.cinder_lane",
        "orpg_rpg": "starter.orpg.silverleaf_chronicle",
        "rpg_arena": "starter.rpg_arena.crypt_ring",
        "hero_defense_survival": "starter.hero_defense.last_brazier",
        "hero_arena": "starter.hero_arena.sunspire_duel",
        "survival_chaos_line_wars": "starter.survival_chaos.obsidian_lanes",
        "mini_game_party": "starter.mini_game_party.crystal_dash",
        "maze_escape": "starter.maze_escape.gloomroot_path"
    })
}
