//! Per-editor #824/#826/#828 FSV runners split from `main.rs` to keep each example file
//! under the 500-line ratchet. Uses the shared helpers from the example crate root.

use litd_editor::mcp_object_tools::{
    LOWER_DESTRUCTIBLE_DOODAD_TOOL, LOWER_GAME_INTERFACE_TOOL, LOWER_UPGRADE_TOOL,
    call_mcp_object_tool, read_mcp_object_document,
};
use serde_json::{Value, json};

use crate::{Base, Ctx, copy, lower, present};

/// Persist the Tech + Unit objects that #824 upgrade `tech_requires` / `applies_to` name.
/// Must go through `copy_base_object` on the LIVE vault document (buff already created it);
/// `initial_document` seed is ignored when the document path already exists.
fn seed_upgrade_cross_record_targets(ctx: &Ctx) -> Result<(), Box<dyn std::error::Error>> {
    let tech = Base {
        kind: "Tech",
        id: "base.tech.smithy",
        display: "Blacksmith",
        props: json!({}),
    };
    copy(ctx, &tech, "tech.smithy", "Tsm1", json!({}), None)?;
    let footman = Base {
        kind: "Unit",
        id: "base.unit.footman",
        display: "Footman",
        props: json!({
            "stats.hit_points": "420",
            "stats.move_speed": "270",
            "art.model": "units/footman.glb",
        }),
    };
    // Base template properties already carry identity; overrides stay empty.
    copy(ctx, &footman, "unit.footman", "hfo1", json!({}), None)?;
    let knight = Base {
        kind: "Unit",
        id: "base.unit.knight",
        display: "Knight",
        props: json!({
            "stats.hit_points": "800",
            "stats.move_speed": "350",
            "art.model": "units/knight.glb",
        }),
    };
    copy(ctx, &knight, "unit.knight", "hkn1", json!({}), None)?;
    Ok(())
}

pub(crate) fn run_upgrade(
    ctx: &Ctx,
) -> Result<(Value, Option<String>, bool), Box<dyn std::error::Error>> {
    seed_upgrade_cross_record_targets(ctx)?;
    let base = Base {
        kind: "Upgrade",
        id: "base.upgrade.blades",
        display: "Steel Blades",
        props: json!({
            "upgrade.effect_kind": "attack",
            "upgrade.level_count": "3",
            "upgrade.level_deltas": "2,4,6",
            "upgrade.gold_costs": "100,200,300",
            "upgrade.lumber_costs": "0,0,0",
            "upgrade.research_times": "30,60,90",
            "upgrade.tech_requires": "tech.smithy",
            "upgrade.applies_to": "unit.footman",
        }),
    };
    copy(
        ctx,
        &base,
        "upgrade.custom.blades",
        "UbL1",
        json!({
            "upgrade.effect_kind": "armor",
            "upgrade.level_deltas": "1,2,3",
            "upgrade.applies_to": "unit.footman,unit.knight",
        }),
        None,
    )?;
    let vault_gold = read_mcp_object_document(&ctx.document_path)?
        .document
        .objects
        .get("upgrade.custom.blades")
        .and_then(|o| o.properties.get("upgrade.gold_costs").cloned());
    let lowered = lower(
        ctx,
        LOWER_UPGRADE_TOOL,
        "upgrade.custom.blades",
        "lowered/upgrade.toml",
    );
    let rec = lowered["lowered_artifact"]["record"]["upgrade"].clone();
    let bad = copy(
        ctx,
        &base,
        "upgrade.bad.blades",
        "UbB1",
        json!({ "upgrade.effect_kind": "teleport" }),
        None,
    )
    .is_err()
        && !present(ctx, "upgrade.bad.blades");
    Ok((rec, vault_gold, bad))
}

pub(crate) fn run_scatter(ctx: &Ctx) -> Result<(Value, bool, Value), Box<dyn std::error::Error>> {
    let dbase = Base {
        kind: "Destructible",
        id: "base.destructible.tree",
        display: "Summer Tree",
        props: json!({
            "destructible.kind": "tree",
            "destructible.max_hp": "50",
            "destructible.armor": "0",
            "destructible.armor_type": "wood",
            "destructible.targeted_as": "tree",
            "destructible.death_art": "art/tree_death.mdx",
            "destructible.pathing_footprint": "paths/1x1.tga",
        }),
    };
    copy(
        ctx,
        &dbase,
        "destructible.custom.gate",
        "DgT1",
        json!({
            "destructible.kind": "gate",
            "destructible.max_hp": "500",
            "destructible.armor": "10",
            "destructible.armor_type": "metal",
            "destructible.targeted_as": "wall,structure",
        }),
        None,
    )?;
    let lowered = lower(
        ctx,
        LOWER_DESTRUCTIBLE_DOODAD_TOOL,
        "destructible.custom.gate",
        "lowered/destructible.toml",
    );
    let dstr_rec = lowered["lowered_artifact"]["record"]["destructible"].clone();
    let dstr_bad = copy(
        ctx,
        &dbase,
        "destructible.bad.tree",
        "DgB1",
        json!({ "destructible.max_hp": "0" }),
        None,
    )
    .is_err()
        && !present(ctx, "destructible.bad.tree");

    // Same tool, doodad kind: proves the #826 pair dispatches both record families.
    let dood_base = Base {
        kind: "Doodad",
        id: "base.doodad.pine",
        display: "Pine Cluster",
        props: json!({
            "doodad.variations": "3",
            "doodad.art_ref": "art/pine.mdx",
            "doodad.min_scale_milli": "900",
            "doodad.max_scale_milli": "1100",
            "doodad.pathing_footprint": "paths/1x1.tga",
        }),
    };
    copy(
        ctx,
        &dood_base,
        "doodad.custom.pine",
        "DdP1",
        json!({ "doodad.variations": "6", "doodad.max_scale_milli": "1500" }),
        None,
    )?;
    let dood_lowered = lower(
        ctx,
        LOWER_DESTRUCTIBLE_DOODAD_TOOL,
        "doodad.custom.pine",
        "lowered/doodad.toml",
    );
    let dood_rec = dood_lowered["lowered_artifact"]["record"]["doodad"].clone();
    Ok((dstr_rec, dstr_bad, dood_rec))
}

pub(crate) fn run_interface(ctx: &Ctx) -> Result<(Value, bool, bool), Box<dyn std::error::Error>> {
    let base = Base {
        kind: "GameInterface",
        id: "base.interface.default",
        display: "Project UI",
        props: json!({
            "interface.string.VICTORY": "You have prevailed.",
            "interface.color.player1": "ff0000",
            "interface.icon.attack": "icons/attack.blp",
            "interface.cmd.attack": "0,0",
            "interface.cmd.stop": "1,0",
            "interface.message_duration_secs": "10",
            "interface.message_font_pct": "100",
        }),
    };
    copy(
        ctx,
        &base,
        "interface.custom.dark",
        "IuD1",
        json!({
            "interface.string.VICTORY": "The dark recedes.",
            "interface.color.player2": "00ff00",
            "interface.cmd.hold": "2,0",
            "interface.message_font_pct": "120",
        }),
        None,
    )?;
    let lowered = lower(
        ctx,
        LOWER_GAME_INTERFACE_TOOL,
        "interface.custom.dark",
        "lowered/interface.toml",
    );
    let rec = lowered["lowered_artifact"]["record"]["interface"].clone();
    // Persist-time rejection: a command-card cell outside the 4x3 card is refused before any write.
    let bad = copy(
        ctx,
        &base,
        "interface.bad.dark",
        "IuB1",
        json!({ "interface.cmd.attack": "9,9" }),
        None,
    )
    .is_err()
        && !present(ctx, "interface.bad.dark");
    // Lowering-time rejection: a duplicate cell passes per-key persist validation but is refused
    // by the cross-key #828 no-duplicate-cell rule at lowering.
    copy(
        ctx,
        &base,
        "interface.dup.dark",
        "IuP1",
        json!({ "interface.cmd.stop": "0,0" }),
        None,
    )?;
    let dup_rejected = call_mcp_object_tool(
        LOWER_GAME_INTERFACE_TOOL,
        json!({
            "document_path": ctx.document_path,
            "artifact_path": ctx.run.artifact_path("lowered/interface.dup.toml")?,
            "object_id": "interface.dup.dark",
        }),
    )
    .is_err();
    Ok((rec, bad, dup_rejected))
}
