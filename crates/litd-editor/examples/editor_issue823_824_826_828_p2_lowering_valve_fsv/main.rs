//! FSV for #823 (buffs/effects) + #824 (upgrades) + #826 (destructibles & doodads) + #828
//! (Game Interface), all lowering through the #829 object-data valve.
//!
//! Vertical slice through the REAL MCP surface for every editor: author a custom record FROM a
//! base with overrides; persist through `copy_base_object` into the Aster vault; INDEPENDENT
//! fresh-handle vault readback confirms the merged object; lower through the per-editor #829
//! valve MCP tool to a fingerprinted litd/data TOML on disk; re-read that TOML (separate MCP
//! tool) and verify every overridden + base-carried field and the fingerprint.
//!
//! The #829 valve is shared generic code, so the three required edge cases + the negative
//! control are exercised on the buff editor as the representative: (1) an invalid override is
//! typed-rejected with the vault byte-identical before/after; (2) a zero-override copy preserves
//! base semantics with only identity differing; (3) deterministic re-lowering yields an
//! identical fingerprint; negative control: a corrupted lowered artifact is rejected with
//! SerializedFingerprintMismatch. Each other editor additionally proves its own persist-time
//! rejection, and the interface editor proves the no-duplicate-cell rule is enforced at
//! lowering. The report is path-free so its sha256 is reproducible.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_BUFF_TOOL, READ_LOWERED_BUFF_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
};
use litd_engine::{EngineError, fsv_artifacts::begin_issue_run};
use serde_json::{Value, json};

mod editors;

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
    run: litd_engine::fsv_artifacts::FsvRun,
}

/// A base object template (identity + default properties) authored records copy from.
pub(crate) struct Base<'a> {
    pub kind: &'a str,
    pub id: &'a str,
    pub display: &'a str,
    pub props: Value,
}

pub(crate) fn copy(
    ctx: &Ctx,
    base: &Base,
    new_id: &str,
    rawcode: &str,
    overrides: Value,
    seed: Option<EditorDocument>,
) -> Result<Value, EngineError> {
    let initial = match seed {
        Some(doc) => serde_json::to_value(doc).unwrap(),
        None => Value::Null,
    };
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": ctx.document_path,
            "ledger_path": ctx.ledger_path,
            "ledger_id": "issue-p2-ledger",
            "request_id": format!("p2-copy-{new_id}"),
            "requested_by": "litd-editor-p2-fsv",
            "initial_document": initial,
            "new_object_id": new_id,
            "rawcode": rawcode,
            "base": json!({
                "base_object_id": base.id,
                "kind": base.kind,
                "display_name": base.display,
                "properties": base.props,
            }),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower(ctx: &Ctx, tool: &str, object_id: &str, artifact: &str) -> Value {
    let path = ctx.run.artifact_path(artifact).unwrap();
    call_mcp_object_tool(
        tool,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": path,
            "object_id": object_id,
        }),
    )
    .expect("lower MCP tool must succeed")
    .result
}

fn present(ctx: &Ctx, object_id: &str) -> bool {
    read_mcp_object_document(&ctx.document_path)
        .expect("vault readback")
        .document
        .objects
        .contains_key(object_id)
}

/// A stable, path-free field slug for a rejected-copy typed error.
fn err_field(error: &EngineError) -> String {
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => field.clone(),
        other => format!("{other:?}")
            .chars()
            .take_while(|c| *c != ' ')
            .collect(),
    }
}

fn buff_base() -> Value {
    json!({
        "buff.status_name": "Inner Fire",
        "buff.status_icon": "icons/innerfire.blp",
        "buff.status_tooltip": "Increases armor and damage.",
        "buff.polarity": "positive",
        "buff.aura_radius": "0",
        "buff.aura_targets": "ground",
        "buff.effect_art": "art/innerfire.mdx",
        "buff.stacking_rule": "refresh",
        "buff.duration_semantics": "timed",
        "buff.duration_secs": "30",
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue823_824_826_828_p2_lowering_valve")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("p2-document-aster"),
        ledger_path: vault.join("p2-ledger-aster"),
        run,
    };
    let seed = EditorDocument::new("calyx://world/issue-p2-editors", "issue-p2-rev-1")?;
    let buff = Base {
        kind: "Buff",
        id: "base.buff.innerfire",
        display: "Inner Fire",
        props: buff_base(),
    };

    // ---- #823 buff: author champion buff, persist, readback, lower, read, verify ----
    copy(
        &ctx,
        &buff,
        "buff.custom.pyre",
        "BpY1",
        json!({
            "buff.aura_radius": "500",
            "buff.aura_targets": "ground,air,structure",
            "buff.stacking_rule": "stack",
            "buff.duration_secs": "45",
        }),
        Some(seed),
    )?;
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let buff_obj = persisted
        .document
        .objects
        .get("buff.custom.pyre")
        .ok_or("buff missing from independent vault readback")?;
    let vault_radius = buff_obj.properties.get("buff.aura_radius").cloned();
    let vault_effect_art = buff_obj.properties.get("buff.effect_art").cloned();

    let buff_lowered = lower(
        &ctx,
        LOWER_BUFF_TOOL,
        "buff.custom.pyre",
        "lowered/buff.toml",
    );
    let buff_fp = buff_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("no buff fingerprint")?
        .to_string();
    let buff_rec = buff_lowered["lowered_artifact"]["record"]["buff"].clone();

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let buff_again = lower(
        &ctx,
        LOWER_BUFF_TOOL,
        "buff.custom.pyre",
        "lowered/buff.b.toml",
    );
    let buff_fp_again = buff_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: invalid enum override rejected AND nothing persisted (byte-identical vault).
    let before_bad = present(&ctx, "buff.bad.pyre");
    let vault_bytes_before = std::fs::read(&ctx.document_path).unwrap_or_default();
    let bad = copy(
        &ctx,
        &buff,
        "buff.bad.pyre",
        "BpB1",
        json!({ "buff.polarity": "chaotic" }),
        None,
    );
    let after_bad = present(&ctx, "buff.bad.pyre");
    let vault_bytes_after = std::fs::read(&ctx.document_path).unwrap_or_default();

    // Edge 2: zero-override clone preserves base buff semantics; only identity differs.
    copy(&ctx, &buff, "buff.clone.pyre", "BpC1", json!({}), None)?;
    let clone_lowered = lower(
        &ctx,
        LOWER_BUFF_TOOL,
        "buff.clone.pyre",
        "lowered/buff.clone.toml",
    );
    let clone_rec = clone_lowered["lowered_artifact"]["record"].clone();

    // Negative control: corrupt the lowered buff artifact -> readback rejects it.
    let buff_path = ctx.run.artifact_path("lowered/buff.toml")?;
    let corrupt =
        std::fs::read_to_string(&buff_path)?.replace("aura_radius = 500", "aura_radius = 501");
    let corrupt_path = ctx.run.artifact_path("lowered/buff.corrupt.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_BUFF_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": buff_fp }),
    )
    .is_err();

    // ---- #824 upgrade ----
    let (upg_rec, upg_vault_gold, upg_bad) = editors::run_upgrade(&ctx)?;
    // ---- #826 destructible + doodad (one tool, both kinds) ----
    let (dstr_rec, dstr_bad, dood_rec) = editors::run_scatter(&ctx)?;
    // ---- #828 game interface ----
    let (iface_rec, iface_bad, iface_dup_lower_rejected) = editors::run_interface(&ctx)?;

    let report = json!({
        "schema": "litd.issue823_824_826_828.p2_lowering_valve.v1",
        "issues": [823, 824, 826, 828],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> litd/data TOML re-read from disk",
        "buff": {
            "fingerprint": buff_fp,
            "vault_readback": { "aura_radius": vault_radius, "base_carried_effect_art": vault_effect_art },
            "disk": {
                "polarity": buff_rec["polarity"],
                "aura_radius": buff_rec["aura_radius"],
                "aura_targets": buff_rec["aura_targets"],
                "stacking_rule": buff_rec["stacking_rule"],
                "duration_secs": buff_rec["duration_secs"],
                "base_carried_effect_art": buff_rec["effect_art"],
            },
            "edges": {
                "invalid_rejected_nothing_persisted": {
                    "before_present": before_bad,
                    "copy_errored": bad.is_err(),
                    "after_present": after_bad,
                    "error_field": bad.err().map(|e| err_field(&e)),
                    "vault_byte_identical": vault_bytes_before == vault_bytes_after,
                },
                "zero_override_identity_only": {
                    "object_id": clone_rec["object_id"],
                    "rawcode": clone_rec["rawcode"],
                    "polarity": clone_rec["buff"]["polarity"],
                    "aura_radius": clone_rec["buff"]["aura_radius"],
                    "stacking_rule": clone_rec["buff"]["stacking_rule"],
                },
                "deterministic_relowering": { "a": buff_fp, "b": buff_fp_again },
            },
            "negative_control": { "corrupt_readback_rejected": corrupt_rejected },
        },
        "upgrade": {
            "vault_base_carried_gold_costs": upg_vault_gold,
            "disk": {
                "effect_kind": upg_rec["effect_kind"],
                "level_count": upg_rec["level_count"],
                "level_deltas": upg_rec["level_deltas"],
                "applies_to": upg_rec["applies_to"],
                "base_carried_gold_costs": upg_rec["gold_costs"],
            },
            "invalid_effect_kind_rejected": upg_bad,
        },
        "destructible": {
            "disk": {
                "kind": dstr_rec["kind"],
                "max_hp": dstr_rec["max_hp"],
                "armor": dstr_rec["armor"],
                "armor_type": dstr_rec["armor_type"],
                "targeted_as": dstr_rec["targeted_as"],
                "base_carried_death_art": dstr_rec["death_art"],
            },
            "invalid_hp_rejected": dstr_bad,
        },
        "doodad": {
            "disk": {
                "variations": dood_rec["variations"],
                "min_scale_milli": dood_rec["min_scale_milli"],
                "max_scale_milli": dood_rec["max_scale_milli"],
            },
        },
        "interface": {
            "disk": {
                "victory_string": iface_rec["ui_strings"]["VICTORY"],
                "color_slots": iface_rec["color_overrides"],
                "command_cell_count": iface_rec["command_cells"].as_object().map(|m| m.len()).unwrap_or(0),
                "message_font_pct": iface_rec["message_font_pct"],
            },
            "invalid_cell_rejected": iface_bad,
            "duplicate_cell_rejected_at_lowering": iface_dup_lower_rejected,
        },
    });

    verify(&report)?;
    let readback = ctx.run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted p2 report hash mismatch".into());
    }
    println!(
        "wrote #823/#824/#826/#828 p2 lowering valve FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let buff = &report["buff"];
    need(
        buff["vault_readback"]["aura_radius"] == json!("500"),
        "buff vault aura_radius",
    )?;
    need(
        buff["vault_readback"]["base_carried_effect_art"] == json!("art/innerfire.mdx"),
        "buff base-carried effect_art did not survive copy",
    )?;
    need(
        buff["disk"]["aura_radius"] == json!(500),
        "buff disk aura_radius",
    )?;
    need(
        buff["disk"]["aura_targets"] == json!(["ground", "air", "structure"]),
        "buff disk aura_targets",
    )?;
    need(
        buff["disk"]["stacking_rule"] == json!("stack"),
        "buff disk stacking_rule",
    )?;
    need(
        buff["disk"]["base_carried_effect_art"] == json!("art/innerfire.mdx"),
        "buff base-carried effect_art did not lower",
    )?;
    let edges = &buff["edges"];
    need(
        edges["invalid_rejected_nothing_persisted"]["before_present"] == json!(false)
            && edges["invalid_rejected_nothing_persisted"]["copy_errored"] == json!(true)
            && edges["invalid_rejected_nothing_persisted"]["after_present"] == json!(false)
            && edges["invalid_rejected_nothing_persisted"]["vault_byte_identical"] == json!(true),
        "buff invalid override was not rejected with the vault byte-identical",
    )?;
    need(
        edges["zero_override_identity_only"]["polarity"] == json!("positive")
            && edges["zero_override_identity_only"]["aura_radius"] == json!(0)
            && edges["zero_override_identity_only"]["stacking_rule"] == json!("refresh"),
        "buff zero-override clone did not preserve base semantics",
    )?;
    need(
        edges["deterministic_relowering"]["a"] == edges["deterministic_relowering"]["b"]
            && !edges["deterministic_relowering"]["a"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
        "buff re-lowering was not deterministic",
    )?;
    need(
        buff["negative_control"]["corrupt_readback_rejected"] == json!(true),
        "corrupt buff artifact was not rejected on readback",
    )?;

    let upg = &report["upgrade"];
    need(
        upg["disk"]["effect_kind"] == json!("armor"),
        "upgrade effect_kind",
    )?;
    need(
        upg["disk"]["level_deltas"] == json!([1, 2, 3]),
        "upgrade level_deltas",
    )?;
    need(
        upg["disk"]["applies_to"] == json!(["unit.footman", "unit.knight"]),
        "upgrade applies_to",
    )?;
    need(
        upg["disk"]["base_carried_gold_costs"] == json!([100, 200, 300]),
        "upgrade base gold_costs",
    )?;
    need(
        upg["invalid_effect_kind_rejected"] == json!(true),
        "upgrade invalid kind not rejected",
    )?;

    let dstr = &report["destructible"];
    need(dstr["disk"]["kind"] == json!("gate"), "destructible kind")?;
    need(dstr["disk"]["max_hp"] == json!(500), "destructible max_hp")?;
    need(dstr["disk"]["armor"] == json!(10), "destructible armor")?;
    need(
        dstr["disk"]["targeted_as"] == json!(["wall", "structure"]),
        "destructible targeted_as",
    )?;
    need(
        dstr["disk"]["base_carried_death_art"] == json!("art/tree_death.mdx"),
        "destructible base-carried death_art",
    )?;
    need(
        dstr["invalid_hp_rejected"] == json!(true),
        "destructible invalid hp not rejected",
    )?;

    let dood = &report["doodad"];
    need(dood["disk"]["variations"] == json!(6), "doodad variations")?;
    need(
        dood["disk"]["min_scale_milli"] == json!(900),
        "doodad min_scale",
    )?;
    need(
        dood["disk"]["max_scale_milli"] == json!(1500),
        "doodad max_scale",
    )?;

    let iface = &report["interface"];
    need(
        iface["disk"]["victory_string"] == json!("The dark recedes."),
        "interface override string",
    )?;
    need(
        iface["disk"]["command_cell_count"] == json!(3),
        "interface command cell count",
    )?;
    need(
        iface["disk"]["message_font_pct"] == json!(120),
        "interface font pct",
    )?;
    need(
        iface["invalid_cell_rejected"] == json!(true),
        "interface out-of-range cell not rejected",
    )?;
    need(
        iface["duplicate_cell_rejected_at_lowering"] == json!(true),
        "interface duplicate cell not rejected at lowering",
    )?;
    Ok(())
}
