//! FSV for #818 (unit data editor) + #829 (object-data lowering valve).
//!
//! Vertical slice, all through the REAL MCP surface: author a custom unit FROM a base
//! with full WC3 field overrides; persist through the object MCP tool into the Aster
//! vault; INDEPENDENT fresh-handle vault readback matches every authored field; lower
//! through the #829 valve MCP tool to a fingerprinted litd/data TOML on disk; re-read the
//! TOML from disk (separate MCP tool) and verify every overridden field + the fingerprint.
//!
//! Edge cases (before/after state printed): (1) invalid field value yields a typed error
//! and NOTHING persisted; (2) copy-from-base zero-override preserves base WC3 semantics,
//! only identity differs; (3) re-lowering unchanged data yields an identical fingerprint.
//!
//! Negative control: corrupt the lowered artifact on disk and prove the readback rejects it.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_UNIT_TOOL, READ_LOWERED_UNIT_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use serde_json::{Value, json};

fn base_properties() -> Value {
    json!({
        "stats.hit_points": "420",
        "stats.mana": "0",
        "stats.armor": "2",
        "stats.move_speed": "270",
        "art.model": "units/human/Footman/Footman.mdl",
        "art.icon": "icons/footman.blp",
        "art.scale_pct": "100",
        "sound.move_set": "FootmanMove",
        "sound.ready": "FootmanReady",
        "combat.damage_dice": "1",
        "combat.damage_sides": "2",
        "combat.damage_base": "12",
        "combat.attack_cooldown_ms": "1350",
        "combat.attack_range": "90",
        "combat.attack_type": "Normal",
        "combat.armor_type": "Heavy",
        "combat.targets_allowed": "ground,structure,debris",
        "movement.type": "Foot",
        "movement.height": "0",
        "movement.turn_rate_pct": "50",
        "techtree.requires": "tech.barracks",
        "economy.gold_cost": "135",
        "economy.lumber_cost": "0",
        "economy.food_cost": "2",
        "economy.build_time_secs": "20",
        "abilities.normal": "ability.defend"
    })
}

fn base_template() -> Value {
    json!({
        "base_object_id": "base.unit.footman",
        "kind": "Unit",
        "display_name": "Footman",
        "properties": base_properties(),
    })
}

/// The overrides that make the champion diverge from the base across every WC3 tab.
fn champion_overrides() -> Value {
    json!({
        "stats.hit_points": "560",
        "stats.armor": "5",
        "stats.mana": "200",
        "combat.attack_type": "Magic",
        "combat.armor_type": "Hero",
        "combat.attack_range": "600",
        "combat.damage_base": "34",
        "combat.targets_allowed": "ground,air,structure",
        "movement.type": "Fly",
        "movement.height": "80",
        "economy.gold_cost": "245",
        "economy.food_cost": "3",
        "techtree.trains": "unit.footman,unit.rifleman",
        "techtree.upgrades_to": "unit.champion.awakened"
    })
}

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

fn copy_unit(
    ctx: &Ctx,
    new_object_id: &str,
    rawcode: &str,
    overrides: Value,
    seed: Option<EditorDocument>,
) -> Result<Value, litd_engine::EngineError> {
    let initial = match seed {
        Some(doc) => serde_json::to_value(doc).unwrap(),
        None => Value::Null,
    };
    call_mcp_object_tool(
        COPY_BASE_OBJECT_TOOL,
        json!({
            "document_path": ctx.document_path,
            "ledger_path": ctx.ledger_path,
            "ledger_id": "issue818-unit-ledger",
            "request_id": format!("issue818-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue818-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": base_template(),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower_unit(ctx: &Ctx, object_id: &str, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_UNIT_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .expect("lower_unit MCP tool must succeed")
    .result
}

fn read_lowered(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_UNIT_TOOL,
        json!({
            "artifact_path": artifact_path,
            "expected_fingerprint": fingerprint,
        }),
    )
    .expect("read_lowered_unit MCP tool must succeed")
    .result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue818_unit_lowering_valve")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("unit-document-aster"),
        ledger_path: vault.join("unit-ledger-aster"),
    };
    let champion_toml = run.artifact_path("lowered/champion.unit.toml")?;
    let champion_toml_b = run.artifact_path("lowered/champion.redo.unit.toml")?;
    let clone_toml = run.artifact_path("lowered/clone.unit.toml")?;

    // 1. Author the custom champion FROM the footman base (seed the empty document).
    let seed = EditorDocument::new("calyx://world/issue818-unit-editor", "issue818-rev-1")?;
    copy_unit(
        &ctx,
        "unit.custom.champion",
        "uCh1",
        champion_overrides(),
        Some(seed),
    )?;

    // 2. INDEPENDENT vault readback (fresh handle) confirms the persisted, merged object.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let champion_obj = persisted
        .document
        .objects
        .get("unit.custom.champion")
        .ok_or("champion missing from independent vault readback")?;
    let vault_armor = champion_obj.properties.get("stats.armor").cloned();
    let vault_attack_type = champion_obj.properties.get("combat.attack_type").cloned();
    let vault_gold = champion_obj.properties.get("economy.gold_cost").cloned();
    // Base-carried (non-overridden) field survives the copy.
    let vault_sound = champion_obj.properties.get("sound.ready").cloned();

    // 3. Lower through the #829 valve MCP tool, then re-read the TOML from disk.
    let lowered = lower_unit(&ctx, "unit.custom.champion", &champion_toml);
    let fingerprint = lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_unit produced no fingerprint")?
        .to_string();
    let disk = read_lowered(&champion_toml, &fingerprint);
    let unit = &disk["lowered_artifact"]["unit"];
    let disk_wc3 = &unit["wc3"];

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let lowered_again = lower_unit(&ctx, "unit.custom.champion", &champion_toml_b);
    let fingerprint_again = lowered_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: an out-of-range override is rejected AND nothing is persisted.
    let before_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("unit.bad.footman");
    let bad = copy_unit(
        &ctx,
        "unit.bad.footman",
        "uBd1",
        json!({ "stats.armor": "99999" }),
        None,
    );
    let after_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("unit.bad.footman");

    // Edge 1b: an out-of-BAND ENUM override (attack_type = Bogus) is rejected at persist time
    // AND nothing is persisted -- the security property the #829 valve must not defer to lowering.
    let before_bad_enum = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("unit.badenum.footman");
    let bad_enum = copy_unit(
        &ctx,
        "unit.badenum.footman",
        "uBe1",
        json!({ "combat.attack_type": "Bogus" }),
        None,
    );
    let after_bad_enum = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("unit.badenum.footman");

    // Edge 2: zero-override copy preserves base WC3 semantics; only identity differs.
    copy_unit(&ctx, "unit.clone.footman", "uCf1", json!({}), None)?;
    let clone_lowered = lower_unit(&ctx, "unit.clone.footman", &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered(&clone_toml, &clone_fp);
    let clone_unit = &clone_disk["lowered_artifact"]["unit"];
    let clone_wc3 = &clone_unit["wc3"];

    // Negative control: corrupt the lowered artifact on disk; the readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&champion_toml)?;
    corrupt = corrupt.replace("armor = 5", "armor = 6");
    let corrupt_path = run.artifact_path("lowered/champion.corrupt.unit.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_UNIT_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": fingerprint }),
    )
    .is_err();
    let corrupt_changed = corrupt != std::fs::read_to_string(&champion_toml)?;

    let report = json!({
        "schema": "litd.issue818.unit_lowering_valve.v1",
        "issues": [818, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> litd/data unit TOML re-read from disk",
        "happy_path": {
            "champion_fingerprint": fingerprint,
            "vault_readback": {
                "armor": vault_armor,
                "attack_type": vault_attack_type,
                "gold_cost": vault_gold,
                "base_carried_sound_ready": vault_sound,
            },
            "disk_toml": {
                "object_id": unit["object_id"],
                "rawcode": unit["rawcode"],
                "base_object_id": unit["base_object_id"],
                "hit_points": unit["hit_points"],
                "armor": disk_wc3["stats"]["armor"],
                "mana": disk_wc3["stats"]["mana"],
                "attack_type": disk_wc3["combat"]["attack_type"],
                "armor_type": disk_wc3["combat"]["armor_type"],
                "attack_range": disk_wc3["combat"]["attack_range"],
                "damage_base": disk_wc3["combat"]["damage_base"],
                "targets_allowed": disk_wc3["combat"]["targets_allowed"],
                "move_type": disk_wc3["movement"]["move_type"],
                "move_height": disk_wc3["movement"]["height"],
                "gold_cost": disk_wc3["economy"]["gold_cost"],
                "food_cost": disk_wc3["economy"]["food_cost"],
                "trains": disk_wc3["techtree"]["trains"],
                "upgrades_to": disk_wc3["techtree"]["upgrades_to"],
                "sound_ready": disk_wc3["sound"]["ready"],
            },
        },
        "edges": {
            "invalid_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
                "error": bad.err().map(|e| e.to_string()),
            },
            "invalid_enum_rejected_nothing_persisted": {
                "before_present": before_bad_enum,
                "copy_errored": bad_enum.is_err(),
                "after_present": after_bad_enum,
                "error": bad_enum.err().map(|e| e.to_string()),
            },
            "zero_override_identity_only": {
                "clone_object_id": clone_unit["object_id"],
                "clone_rawcode": clone_unit["rawcode"],
                "clone_base_object_id": clone_unit["base_object_id"],
                "clone_armor": clone_wc3["stats"]["armor"],
                "clone_attack_type": clone_wc3["combat"]["attack_type"],
                "clone_armor_type": clone_wc3["combat"]["armor_type"],
                "clone_gold_cost": clone_wc3["economy"]["gold_cost"],
                "clone_sound_ready": clone_wc3["sound"]["ready"],
                "clone_targets": clone_wc3["combat"]["targets_allowed"],
            },
            "deterministic_relowering": {
                "fingerprint_a": fingerprint,
                "fingerprint_b": fingerprint_again,
            },
        },
        "negative_control": {
            "corrupt_written": corrupt_changed,
            "corrupt_readback_rejected": corrupt_rejected,
        },
    });

    verify(&report)?;
    let readback = run.write_report(&report)?;
    let bytes = std::fs::read(&readback.path)?;
    let reparsed: Value = serde_json::from_slice(&bytes)?;
    verify(&reparsed)?;
    if litd_hash::sha256_hex(&bytes) != readback.sha256 {
        return Err("persisted #818 report hash mismatch".into());
    }
    println!(
        "wrote #818/#829 unit lowering valve FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}

fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let happy = &report["happy_path"];
    // Independent vault readback carried every overridden + base field.
    need(
        happy["vault_readback"]["armor"] == json!("5"),
        "vault readback armor mismatch",
    )?;
    need(
        happy["vault_readback"]["attack_type"] == json!("Magic"),
        "vault readback attack_type mismatch",
    )?;
    need(
        happy["vault_readback"]["gold_cost"] == json!("245"),
        "vault readback gold_cost mismatch",
    )?;
    need(
        happy["vault_readback"]["base_carried_sound_ready"] == json!("FootmanReady"),
        "base-carried sound.ready did not survive the copy",
    )?;
    // Disk TOML matches every overridden field (typed values, not strings).
    let disk = &happy["disk_toml"];
    need(disk["armor"] == json!(5), "disk armor mismatch")?;
    need(disk["mana"] == json!(200), "disk mana mismatch")?;
    need(disk["hit_points"] == json!(560), "disk hit_points mismatch")?;
    need(
        disk["attack_type"] == json!("Magic"),
        "disk attack_type mismatch",
    )?;
    need(
        disk["armor_type"] == json!("Hero"),
        "disk armor_type mismatch",
    )?;
    need(
        disk["attack_range"] == json!(600),
        "disk attack_range mismatch",
    )?;
    need(
        disk["damage_base"] == json!(34),
        "disk damage_base mismatch",
    )?;
    need(
        disk["targets_allowed"] == json!(["ground", "air", "structure"]),
        "disk targets_allowed mismatch",
    )?;
    need(disk["move_type"] == json!("Fly"), "disk move_type mismatch")?;
    need(
        disk["move_height"] == json!(80),
        "disk move_height mismatch",
    )?;
    need(disk["gold_cost"] == json!(245), "disk gold_cost mismatch")?;
    need(disk["food_cost"] == json!(3), "disk food_cost mismatch")?;
    need(
        disk["trains"] == json!(["unit.footman", "unit.rifleman"]),
        "disk trains mismatch",
    )?;
    need(
        disk["upgrades_to"] == json!("unit.champion.awakened"),
        "disk upgrades_to mismatch",
    )?;
    need(
        disk["sound_ready"] == json!("FootmanReady"),
        "base-carried sound.ready did not lower",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid override was NOT rejected without persistence",
    )?;
    let bad_enum = &edges["invalid_enum_rejected_nothing_persisted"];
    need(
        bad_enum["before_present"] == json!(false)
            && bad_enum["copy_errored"] == json!(true)
            && bad_enum["after_present"] == json!(false),
        "invalid enum override was NOT rejected without persistence",
    )?;

    // Zero-override clone: base WC3 semantics preserved, only identity differs.
    let clone = &edges["zero_override_identity_only"];
    need(
        clone["clone_object_id"] == json!("unit.clone.footman"),
        "clone identity object_id wrong",
    )?;
    need(
        clone["clone_rawcode"] == json!("uCf1"),
        "clone identity rawcode wrong",
    )?;
    need(
        clone["clone_base_object_id"] == json!("base.unit.footman"),
        "clone base_object_id wrong",
    )?;
    // These equal the AUTHORED base literals (armor 2, Normal, Heavy, gold 135), not the champion.
    need(
        clone["clone_armor"] == json!(2),
        "clone did not preserve base armor",
    )?;
    need(
        clone["clone_attack_type"] == json!("Normal"),
        "clone did not preserve base attack_type",
    )?;
    need(
        clone["clone_armor_type"] == json!("Heavy"),
        "clone did not preserve base armor_type",
    )?;
    need(
        clone["clone_gold_cost"] == json!(135),
        "clone did not preserve base gold_cost",
    )?;
    need(
        clone["clone_sound_ready"] == json!("FootmanReady"),
        "clone did not preserve base sound.ready",
    )?;
    need(
        clone["clone_targets"] == json!(["ground", "structure", "debris"]),
        "clone did not preserve base targets_allowed",
    )?;

    let det = &edges["deterministic_relowering"];
    need(
        det["fingerprint_a"] == det["fingerprint_b"]
            && !det["fingerprint_a"].as_str().unwrap_or_default().is_empty(),
        "re-lowering unchanged data was not deterministic",
    )?;

    let neg = &report["negative_control"];
    need(
        neg["corrupt_written"] == json!(true) && neg["corrupt_readback_rejected"] == json!(true),
        "corrupt lowered artifact was NOT rejected on readback",
    )?;
    Ok(())
}
