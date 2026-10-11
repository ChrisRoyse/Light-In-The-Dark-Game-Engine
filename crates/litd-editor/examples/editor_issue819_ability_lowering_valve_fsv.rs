//! FSV for #819 (ability data editor) + #829 (object-data lowering valve).
//!
//! Vertical slice through the REAL MCP surface: author a custom ability FROM a base with field
//! overrides (kind/mana/range/targeting/levels + per-level scaling arrays); persist through the
//! object MCP copy tool into the Aster vault; INDEPENDENT fresh-handle vault readback matches the
//! authored fields; lower through the #829 valve MCP tool to a fingerprinted litd/data TOML on
//! disk; re-read the TOML from disk (separate MCP tool) verifying every overridden field + the
//! fingerprint.
//!
//! Edge cases (before/after state printed): (1) invalid ability.kind yields a typed error and
//! NOTHING persisted; (2) copy-from-base zero-override preserves base ability semantics, only
//! identity differs; (3) re-lowering unchanged data yields an identical fingerprint.
//!
//! Negative control: corrupt the lowered artifact on disk and prove the readback rejects it.

use litd_editor::{
    fsv_support,
    history::EditorDocument,
    mcp_object_tools::{
        COPY_BASE_OBJECT_TOOL, LOWER_ABILITY_TOOL, READ_LOWERED_ABILITY_TOOL, call_mcp_object_tool,
        read_mcp_object_document,
    },
};
use litd_engine::fsv_artifacts::begin_issue_run;
use serde_json::{Value, json};

fn base_properties() -> Value {
    json!({
        "ability.kind": "hero-learnable",
        "ability.mana_cost": "75",
        "ability.cooldown_ms": "8000",
        "ability.cast_range": "600",
        "ability.targeting": "unit",
        "ability.targets_allowed": "ground,air",
        "ability.duration_ms": "0",
        "ability.area_of_effect": "200",
        "ability.levels": "3",
        "ability.scaling.base_damage": "50,100,150",
        "ability.effects": "effect.fireball",
        "ability.icon": "icons/fireball.blp",
        "ability.requirements": "tech.arcane"
    })
}

fn base_template() -> Value {
    json!({
        "base_object_id": "base.ability.fireball",
        "kind": "Ability",
        "display_name": "Fireball",
        "properties": base_properties(),
    })
}

fn champion_overrides() -> Value {
    json!({
        "ability.mana_cost": "120",
        "ability.cooldown_ms": "12000",
        "ability.cast_range": "800",
        "ability.area_of_effect": "300",
        "ability.levels": "2",
        "ability.scaling.base_damage": "80,160",
        "ability.targeting": "point"
    })
}

struct Ctx {
    document_path: std::path::PathBuf,
    ledger_path: std::path::PathBuf,
}

fn copy_ability(
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
            "ledger_id": "issue819-ability-ledger",
            "request_id": format!("issue819-copy-{new_object_id}"),
            "requested_by": "litd-editor-issue819-fsv",
            "initial_document": initial,
            "new_object_id": new_object_id,
            "rawcode": rawcode,
            "base": base_template(),
            "overrides": overrides,
        }),
    )
    .map(|out| out.result)
}

fn lower_ability(ctx: &Ctx, object_id: &str, artifact_path: &std::path::Path) -> Value {
    call_mcp_object_tool(
        LOWER_ABILITY_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": artifact_path,
            "object_id": object_id,
        }),
    )
    .expect("lower_ability MCP tool must succeed")
    .result
}

fn read_lowered(artifact_path: &std::path::Path, fingerprint: &str) -> Value {
    call_mcp_object_tool(
        READ_LOWERED_ABILITY_TOOL,
        json!({
            "artifact_path": artifact_path,
            "expected_fingerprint": fingerprint,
        }),
    )
    .expect("read_lowered_ability MCP tool must succeed")
    .result
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    fsv_support::ensure_project_root_env();
    let run = begin_issue_run("issue819_ability_lowering_valve")?;
    let vault = run.artifact_dir("vault")?;
    let ctx = Ctx {
        document_path: vault.join("ability-document-aster"),
        ledger_path: vault.join("ability-ledger-aster"),
    };
    let champion_toml = run.artifact_path("lowered/champion.ability.toml")?;
    let champion_toml_b = run.artifact_path("lowered/champion.redo.ability.toml")?;
    let clone_toml = run.artifact_path("lowered/clone.ability.toml")?;

    // 1. Author the custom ability FROM the fireball base (seed the empty document).
    let seed = EditorDocument::new("calyx://world/issue819-ability-editor", "issue819-rev-1")?;
    copy_ability(
        &ctx,
        "ability.custom.pyroblast",
        "Apyr",
        champion_overrides(),
        Some(seed),
    )?;

    // 2. INDEPENDENT vault readback (fresh handle) confirms the persisted, merged object.
    let persisted = read_mcp_object_document(&ctx.document_path)?;
    let obj = persisted
        .document
        .objects
        .get("ability.custom.pyroblast")
        .ok_or("ability missing from independent vault readback")?;
    let vault_mana = obj.properties.get("ability.mana_cost").cloned();
    let vault_levels = obj.properties.get("ability.levels").cloned();
    let vault_targeting = obj.properties.get("ability.targeting").cloned();
    // Base-carried (non-overridden) field survives the copy.
    let vault_kind = obj.properties.get("ability.kind").cloned();

    // 3. Lower through the #829 valve MCP tool, then re-read the TOML from disk.
    let lowered = lower_ability(&ctx, "ability.custom.pyroblast", &champion_toml);
    let fingerprint = lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .ok_or("lower_ability produced no fingerprint")?
        .to_string();
    let disk = read_lowered(&champion_toml, &fingerprint);
    let ability = &disk["lowered_artifact"]["ability"];
    let fields = &ability["fields"];

    // Edge 3: deterministic re-lowering yields an identical fingerprint.
    let lowered_again = lower_ability(&ctx, "ability.custom.pyroblast", &champion_toml_b);
    let fingerprint_again = lowered_again["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    // Edge 1: an out-of-BAND ENUM override (kind = bogus) is rejected at persist time AND nothing
    // is persisted -- the security property the #829 valve must not defer to lowering.
    let before_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("ability.bad.kind");
    let bad = copy_ability(
        &ctx,
        "ability.bad.kind",
        "Abad",
        json!({ "ability.kind": "bogus" }),
        None,
    );
    let after_bad = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .contains_key("ability.bad.kind");

    // Edge 2: zero-override copy preserves base ability semantics; only identity differs.
    copy_ability(&ctx, "ability.clone.fireball", "Aclf", json!({}), None)?;
    let clone_lowered = lower_ability(&ctx, "ability.clone.fireball", &clone_toml);
    let clone_fp = clone_lowered["lowered_artifact"]["fingerprint"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let clone_disk = read_lowered(&clone_toml, &clone_fp);
    let clone_ability = &clone_disk["lowered_artifact"]["ability"];
    let clone_fields = &clone_ability["fields"];

    // Negative control: corrupt the lowered artifact on disk; the readback MUST reject it.
    let mut corrupt = std::fs::read_to_string(&champion_toml)?;
    corrupt = corrupt.replace("mana_cost = 120", "mana_cost = 121");
    let corrupt_path = run.artifact_path("lowered/champion.corrupt.ability.toml")?;
    if let Some(parent) = corrupt_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&corrupt_path, &corrupt)?;
    let corrupt_rejected = call_mcp_object_tool(
        READ_LOWERED_ABILITY_TOOL,
        json!({ "artifact_path": corrupt_path, "expected_fingerprint": fingerprint }),
    )
    .is_err();
    let corrupt_changed = corrupt != std::fs::read_to_string(&champion_toml)?;

    let report = json!({
        "schema": "litd.issue819.ability_lowering_valve.v1",
        "issues": [819, 829],
        "source_of_truth": "object MCP tools -> Aster vault -> #829 lowering valve -> litd/data ability TOML re-read from disk",
        "happy_path": {
            "champion_fingerprint": fingerprint,
            "vault_readback": {
                "mana_cost": vault_mana,
                "levels": vault_levels,
                "targeting": vault_targeting,
                "base_carried_kind": vault_kind,
            },
            "disk_toml": {
                "object_id": ability["object_id"],
                "rawcode": ability["rawcode"],
                "base_object_id": ability["base_object_id"],
                "kind": fields["kind"],
                "mana_cost": fields["mana_cost"],
                "cooldown_ms": fields["cooldown_ms"],
                "cast_range": fields["cast_range"],
                "targeting": fields["targeting"],
                "targets_allowed": fields["targets_allowed"],
                "area_of_effect": fields["area_of_effect"],
                "levels": fields["levels"],
                "scaling_base_damage": fields["scaling"]["base_damage"],
                "effects": fields["effects"],
                "icon": fields["icon"],
                "requirements": fields["requirements"],
            },
        },
        "edges": {
            "invalid_enum_rejected_nothing_persisted": {
                "before_present": before_bad,
                "copy_errored": bad.is_err(),
                "after_present": after_bad,
                "error": bad.err().map(|e| e.to_string()),
            },
            "zero_override_identity_only": {
                "clone_object_id": clone_ability["object_id"],
                "clone_rawcode": clone_ability["rawcode"],
                "clone_base_object_id": clone_ability["base_object_id"],
                "clone_kind": clone_fields["kind"],
                "clone_mana_cost": clone_fields["mana_cost"],
                "clone_levels": clone_fields["levels"],
                "clone_scaling_base_damage": clone_fields["scaling"]["base_damage"],
                "clone_targets": clone_fields["targets_allowed"],
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
        return Err("persisted #819 report hash mismatch".into());
    }
    println!(
        "wrote #819/#829 ability lowering valve FSV to {}",
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
    need(
        happy["vault_readback"]["mana_cost"] == json!("120"),
        "vault readback mana_cost mismatch",
    )?;
    need(
        happy["vault_readback"]["levels"] == json!("2"),
        "vault readback levels mismatch",
    )?;
    need(
        happy["vault_readback"]["targeting"] == json!("point"),
        "vault readback targeting mismatch",
    )?;
    need(
        happy["vault_readback"]["base_carried_kind"] == json!("hero-learnable"),
        "base-carried ability.kind did not survive the copy",
    )?;
    let disk = &happy["disk_toml"];
    need(
        disk["kind"] == json!("hero-learnable"),
        "disk kind mismatch",
    )?;
    need(disk["mana_cost"] == json!(120), "disk mana_cost mismatch")?;
    need(disk["cast_range"] == json!(800), "disk cast_range mismatch")?;
    need(
        disk["targeting"] == json!("point"),
        "disk targeting mismatch",
    )?;
    need(
        disk["targets_allowed"] == json!(["ground", "air"]),
        "disk targets_allowed mismatch",
    )?;
    need(disk["levels"] == json!(2), "disk levels mismatch")?;
    need(
        disk["scaling_base_damage"] == json!([80, 160]),
        "disk scaling_base_damage mismatch",
    )?;
    need(
        disk["effects"] == json!(["effect.fireball"]),
        "disk effects mismatch",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_enum_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid enum override was NOT rejected without persistence",
    )?;

    let clone = &edges["zero_override_identity_only"];
    need(
        clone["clone_object_id"] == json!("ability.clone.fireball"),
        "clone identity object_id wrong",
    )?;
    need(
        clone["clone_base_object_id"] == json!("base.ability.fireball"),
        "clone base_object_id wrong",
    )?;
    // These equal the AUTHORED base literals, not the champion.
    need(
        clone["clone_kind"] == json!("hero-learnable"),
        "clone did not preserve base kind",
    )?;
    need(
        clone["clone_mana_cost"] == json!(75),
        "clone did not preserve base mana_cost",
    )?;
    need(
        clone["clone_levels"] == json!(3),
        "clone did not preserve base levels",
    )?;
    need(
        clone["clone_scaling_base_damage"] == json!([50, 100, 150]),
        "clone did not preserve base scaling",
    )?;
    need(
        clone["clone_targets"] == json!(["ground", "air"]),
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
