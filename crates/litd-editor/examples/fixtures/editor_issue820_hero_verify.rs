//! Verify helper for the #820 hero lowering-valve FSV, split out to keep the example under
//! the <500-line ratchet. Asserts every happy-path field, all cross-record + generic edges,
//! and the negative control on the run-invariant report.

use serde_json::{Value, json};

pub(crate) fn verify(report: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let need = |cond: bool, msg: &str| -> Result<(), Box<dyn std::error::Error>> {
        if cond { Ok(()) } else { Err(msg.into()) }
    };
    let happy = &report["happy_path"];
    let vault = &happy["vault_readback"];
    need(
        vault["primary_attribute"] == json!("INT"),
        "vault attr mismatch",
    )?;
    need(
        vault["hit_points"] == json!("520"),
        "vault hit_points mismatch",
    )?;
    need(
        vault["learnable"] == json!("ability.hero.fireball@1,ability.hero.blink@3"),
        "vault learnable mismatch",
    )?;
    need(
        vault["base_carried_str_base"] == json!("22"),
        "base-carried hero.str_base did not survive the copy",
    )?;

    let disk = &happy["disk_toml"];
    need(disk["hit_points"] == json!(520), "disk hit_points mismatch")?;
    need(
        disk["wc3_attack_type"] == json!("Hero"),
        "disk wc3 attack_type mismatch",
    )?;
    need(
        disk["primary_attribute"] == json!("INT"),
        "disk primary_attribute mismatch",
    )?;
    need(disk["int_base"] == json!(26), "disk int_base mismatch")?;
    need(
        disk["int_gain_milli"] == json!(3400),
        "disk int_gain_milli mismatch",
    )?;
    need(
        disk["str_base"] == json!(22),
        "disk base-carried str_base mismatch",
    )?;
    need(
        disk["revive_base_cost_gold"] == json!(260),
        "disk revive cost mismatch",
    )?;
    need(
        disk["xp_curve_ref"] == json!("gameplay.xp.hero_default"),
        "disk xp_curve_ref mismatch",
    )?;
    need(
        disk["flags"] == json!(["hero_glow", "revivable"]),
        "disk flags mismatch",
    )?;
    need(
        disk["learnable"]
            == json!([
                { "ability_id": "ability.hero.fireball", "level_requirement": 1 },
                { "ability_id": "ability.hero.blink", "level_requirement": 3 },
            ]),
        "disk learnable mismatch",
    )?;

    let edges = &report["edges"];
    let bad = &edges["invalid_rejected_nothing_persisted"];
    need(
        bad["before_present"] == json!(false)
            && bad["copy_errored"] == json!(true)
            && bad["after_present"] == json!(false),
        "invalid primary_attribute was NOT rejected without persistence",
    )?;

    let dangling = &edges["dangling_learnable_ref_fails_lowering"];
    need(
        dangling["lower_errored"] == json!(true)
            && dangling["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not a persisted object"),
        "dangling learnable ref did NOT fail lowering",
    )?;
    let wrongkind = &edges["wrong_kind_learnable_ref_fails_lowering"];
    need(
        wrongkind["lower_errored"] == json!(true)
            && wrongkind["error"]
                .as_str()
                .unwrap_or_default()
                .contains("hero-learnable"),
        "wrong-kind learnable ref did NOT fail lowering",
    )?;

    let clone = &edges["zero_override_identity_only"];
    need(
        clone["clone_primary_attribute"] == json!("STR"),
        "clone did not preserve base primary_attribute",
    )?;
    need(
        clone["clone_str_base"] == json!(22),
        "clone did not preserve base str_base",
    )?;
    need(
        clone["clone_learnable"]
            == json!([
                { "ability_id": "ability.hero.fireball", "level_requirement": 1 },
            ]),
        "clone did not preserve base learnable",
    )?;
    need(
        clone["clone_flags"] == json!(["hero_glow", "revivable"]),
        "clone did not preserve base flags",
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
