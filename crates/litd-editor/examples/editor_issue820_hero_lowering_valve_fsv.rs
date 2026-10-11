//! FSV for #820 (hero data editor) + #829 (object-data lowering valve).
//!
//! Vertical slice through the REAL MCP surface: persist three #819 abilities (two
//! hero-learnable, one passive) and a hero authored FROM a base with hero + unit overrides,
//! all into one Aster vault document; INDEPENDENT fresh-handle vault readback matches the
//! authored fields; lower the hero through the #829 valve MCP tool to a fingerprinted litd/data
//! TOML on disk; re-read the TOML from disk verifying every overridden field + the fingerprint.
//!
//! Edge cases (before/after state printed): (1) invalid hero.primary_attribute is typed-rejected
//! with NOTHING persisted; (2) zero-override copy preserves base hero semantics, only identity
//! differs; (3) re-lowering unchanged data yields an identical fingerprint. Cross-record edges
//! for #820: a learnable ref to a NON-persisted ability, and a ref to a persisted but WRONG-kind
//! (passive) ability, BOTH fail lowering with a typed error.
//!
//! Negative control: corrupt the lowered artifact on disk and prove the readback rejects it.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_HERO_TOOL, READ_LOWERED_HERO_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use serde_json::{Value, json};

#[path = "fixtures/editor_issue820_hero_verify.rs"]
mod verify_support;
use verify_support::verify;

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

fn ability_props(kind: &str) -> Value {
    json!({
        "ability.kind": kind,
        "ability.mana_cost": "60",
        "ability.cooldown_ms": "6000",
        "ability.targeting": "unit",
        "ability.targets_allowed": "ground,air",
        "ability.levels": "1"
    })
}

fn hero_base_props() -> Value {
    json!({
        "stats.hit_points": "600",
        "stats.move_speed": "290",
        "stats.mana": "300",
        "stats.armor": "4",
        "art.model": "units/human/Paladin/Paladin.mdl",
        "combat.attack_type": "Hero",
        "combat.armor_type": "Hero",
        "hero.primary_attribute": "STR",
        "hero.str_base": "22",
        "hero.str_gain_milli": "2700",
        "hero.agi_base": "13",
        "hero.agi_gain_milli": "1500",
        "hero.int_base": "17",
        "hero.int_gain_milli": "1800",
        "hero.xp_curve_ref": "gameplay.xp.hero_default",
        "hero.level_curve_ref": "gameplay.level.hero_default",
        "hero.revive_base_time_secs": "30",
        "hero.revive_time_per_level_secs": "6",
        "hero.revive_base_cost_gold": "180",
        "hero.revive_cost_per_level_gold": "40",
        "hero.learnable": "ability.hero.fireball@1",
        "hero.flags": "hero_glow,revivable"
    })
}

fn hero_overrides() -> Value {
    json!({
        "hero.primary_attribute": "INT",
        "hero.int_base": "26",
        "hero.int_gain_milli": "3400",
        "hero.revive_base_cost_gold": "260",
        "stats.hit_points": "520",
        "hero.learnable": "ability.hero.fireball@1,ability.hero.blink@3"
    })
}

fn base_tpl(base_id: &str, kind: &str, display: &str, props: Value) -> Value {
    json!({
        "base_object_id": base_id,
        "kind": kind,
        "display_name": display,
        "properties": props,
    })
}

fn copy_object(
    ctx: &Ctx,
    new_object_id: &str,
    rawcode: &str,
    base: Value,
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
            "ledger_id": "issue820-hero-ledger",
            "request_id": format!("issue820-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue820-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": base,
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower_hero(ctx: &Ctx, object_id: &str, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_HERO_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .expect("lower_hero MCP tool must succeed")
    .result
}

fn try_lower_hero(
    ctx: &Ctx,
    object_id: &str,
    artifact_path: &std::path::Path,
) -> Result<Value, litd_engine::EngineError> {
    call_mcp_object_tool(
        LOWER_HERO_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .map(|out| out.result)
}

fn read_lowered(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_HERO_TOOL,
        json!({ "artifact_path": artifact_path, "expected_fingerprint": fingerprint }),
    )
    .expect("read_lowered_hero MCP tool must succeed")
    .result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue820_hero_lowering_valve")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("hero-document-aster"),
        ledger_path: vault.join("hero-ledger-aster"),
    };
    let champ_toml = run.artifact_path("lowered/champion.hero.toml")?;
    let champ_toml_b = run.artifact_path("lowered/champion.redo.hero.toml")?;
    let clone_toml = run.artifact_path("lowered/clone.hero.toml")?;
    let dangling_toml = run.artifact_path("lowered/dangling.hero.toml")?;
    let wrongkind_toml = run.artifact_path("lowered/wrongkind.hero.toml")?;

    // Persist the ability records first: two hero-learnable, one passive (wrong kind).
    let seed = EditorDocument::new("calyx://world/issue820-hero-editor", "issue820-rev-1")?;
    let none = json!({});
    let ability = |kind: &str, display: &str| {
        base_tpl("base.ability", "Ability", display, ability_props(kind))
    };
    let paladin = || base_tpl("base.hero.paladin", "Hero", "Paladin", hero_base_props());
    copy_object(
        &ctx,
        "ability.hero.fireball",
        "Ahfb",
        ability("hero-learnable", "Fireball"),
        none.clone(),
        Some(seed),
    )?;
    copy_object(
        &ctx,
        "ability.hero.blink",
        "Ahbl",
        ability("hero-learnable", "Blink"),
        none.clone(),
        None,
    )?;
    copy_object(
        &ctx,
        "ability.hero.aura",
        "Ahau",
        ability("passive", "Aura"),
        none.clone(),
        None,
    )?;

    // Author the champion hero FROM a paladin base with hero + unit overrides.
    copy_object(
        &ctx,
        "hero.custom.archmage",
        "Harc",
        paladin(),
        hero_overrides(),
        None,
    )?;

    // INDEPENDENT vault readback (fresh handle) confirms the persisted, merged hero.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let obj = persisted
        .document
        .objects
        .get("hero.custom.archmage")
        .ok_or("hero missing from independent vault readback")?;
    let vault_attr = obj.properties.get("hero.primary_attribute").cloned();
    let vault_hp = obj.properties.get("stats.hit_points").cloned();
    let vault_learnable = obj.properties.get("hero.learnable").cloned();
    let vault_str_base = obj.properties.get("hero.str_base").cloned();

    // Lower through the #829 valve MCP tool, then re-read the TOML from disk.
    let lowered = lower_hero(&ctx, "hero.custom.archmage", &champ_toml);
    let fingerprint = lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_hero produced no fingerprint")?
        .to_string();
    let disk = read_lowered(&champ_toml, &fingerprint);
    let hero_obj = &disk["lowered_artifact"]["hero"];
    let hero_fields = &hero_obj["hero"];

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let lowered_again = lower_hero(&ctx, "hero.custom.archmage", &champ_toml_b);
    let fingerprint_again = lowered_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: invalid hero.primary_attribute rejected at persist time, nothing persisted.
    let before_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("hero.bad.attr");
    let bad = copy_object(
        &ctx,
        "hero.bad.attr",
        "Hbad",
        paladin(),
        json!({ "hero.primary_attribute": "LUCK" }),
        None,
    );
    let after_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("hero.bad.attr");

    // Cross-record edge D: learnable ref to a NON-persisted ability fails LOWERING (typed error).
    copy_object(
        &ctx,
        "hero.dangling",
        "Hdng",
        paladin(),
        json!({ "hero.learnable": "ability.hero.nonexistent@2" }),
        None,
    )?;
    let dangling = try_lower_hero(&ctx, "hero.dangling", &dangling_toml);

    // Cross-record edge W: learnable ref to a persisted but WRONG-kind (passive) ability fails.
    copy_object(
        &ctx,
        "hero.wrongkind",
        "Hwrg",
        paladin(),
        json!({ "hero.learnable": "ability.hero.aura@2" }),
        None,
    )?;
    let wrongkind = try_lower_hero(&ctx, "hero.wrongkind", &wrongkind_toml);

    // Edge 2: zero-override copy preserves base hero semantics; only identity differs.
    copy_object(&ctx, "hero.clone.paladin", "Hclp", paladin(), none, None)?;
    let clone_lowered = lower_hero(&ctx, "hero.clone.paladin", &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered(&clone_toml, &clone_fp);
    let clone_hero = &clone_disk["lowered_artifact"]["hero"]["hero"];

    // Negative control: corrupt the lowered artifact on disk; the readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&champ_toml)?;
    corrupt = corrupt.replace("hit_points = 520", "hit_points = 521");
    let corrupt_path = run.artifact_path("lowered/champion.corrupt.hero.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_HERO_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": fingerprint }),
    )
    .is_err();
    let corrupt_changed = corrupt != std::fs::read_to_string(&champ_toml)?;

    let report = json!({
        "schema": "litd.issue820.hero_lowering_valve.v1",
        "issues": [820, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> litd/data hero TOML re-read from disk",
        "happy_path": {
            "champion_fingerprint": fingerprint,
            "vault_readback": {
                "primary_attribute": vault_attr,
                "hit_points": vault_hp,
                "learnable": vault_learnable,
                "base_carried_str_base": vault_str_base,
            },
            "disk_toml": {
                "object_id": hero_obj["object_id"],
                "base_object_id": hero_obj["base_object_id"],
                "hit_points": hero_obj["hit_points"],
                "wc3_attack_type": hero_obj["wc3"]["combat"]["attack_type"],
                "primary_attribute": hero_fields["primary_attribute"],
                "int_base": hero_fields["int_base"],
                "int_gain_milli": hero_fields["int_gain_milli"],
                "str_base": hero_fields["str_base"],
                "revive_base_cost_gold": hero_fields["revive_base_cost_gold"],
                "xp_curve_ref": hero_fields["xp_curve_ref"],
                "flags": hero_fields["flags"],
                "learnable": hero_fields["learnable"],
            },
        },
        "edges": {
            "invalid_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
                "error": bad.err().map(|e| e.to_string()),
            },
            "dangling_learnable_ref_fails_lowering": {
                "lower_errored": dangling.is_err(),
                "error": dangling.err().map(|e| e.to_string()),
            },
            "wrong_kind_learnable_ref_fails_lowering": {
                "lower_errored": wrongkind.is_err(),
                "error": wrongkind.err().map(|e| e.to_string()),
            },
            "zero_override_identity_only": {
                "clone_primary_attribute": clone_hero["primary_attribute"],
                "clone_str_base": clone_hero["str_base"],
                "clone_learnable": clone_hero["learnable"],
                "clone_flags": clone_hero["flags"],
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
        return Err("persisted #820 report hash mismatch".into());
    }
    println!(
        "wrote #820/#829 hero lowering valve FSV to {}",
        readback.path.display()
    );
    println!("report_sha256={}", readback.sha256);
    Ok(())
}
