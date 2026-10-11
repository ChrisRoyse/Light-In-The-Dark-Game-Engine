use std::collections::{BTreeMap, BTreeSet};

use litd_engine::{EngineError, math::Transform};

use crate::history::{EditorDocument, EditorObject};
// Single source of truth for the WC3 enum/flag allowed-value sets AND numeric bounds (#1837):
// the SAME constants the lowering-time validators check against, re-exported through
// `object_editor`. Referencing them here (never re-listing them) keeps persist-time and
// lowering-time validation from drifting apart.
use crate::object_editor::{
    ABILITY_AOE_MAX, ABILITY_COOLDOWN_MAX, ABILITY_DURATION_MAX, ABILITY_KINDS, ABILITY_LEVELS_MAX,
    ABILITY_LEVELS_MIN, ABILITY_MANA_MAX, ABILITY_RANGE_MAX, ABILITY_SCALE_MAX,
    ABILITY_TARGET_TYPES, HERO_ATTR_BASE_MAX, HERO_ATTR_GAIN_MILLI_MAX, HERO_FLAGS,
    HERO_PRIMARY_ATTRS, HERO_REVIVE_COST_MAX, HERO_REVIVE_TIME_MAX, UNIT_ARMOR_MAX,
    UNIT_ATTACK_COOLDOWN_MS_MAX, UNIT_ATTACK_COOLDOWN_MS_MIN, UNIT_ATTACK_RANGE_MAX,
    UNIT_BUILD_TIME_SECS_MAX, UNIT_DAMAGE_BASE_MAX, UNIT_DAMAGE_DICE_MAX, UNIT_DAMAGE_DICE_MIN,
    UNIT_FOOD_COST_MAX, UNIT_GOLD_COST_MAX, UNIT_HIT_POINTS_MAX, UNIT_HIT_POINTS_MIN,
    UNIT_LUMBER_COST_MAX, UNIT_MANA_MAX, UNIT_MOVE_SPEED_MAX, UNIT_MOVE_SPEED_MIN,
    UNIT_SCALE_PCT_MAX, UNIT_SCALE_PCT_MIN, UNIT_TURN_RATE_PCT_MAX, WC3_ARMOR_TYPES,
    WC3_ATTACK_TYPES, WC3_MOVE_TYPES, WC3_TARGET_FLAGS,
};
// #821 item + #822 commerce canonical enum/flag/range sets: the SAME constants the lowering-time
// validators (`object_editor_item_fields`, `object_editor_shop_fields`) check against, referenced
// here (never re-listed) so persist-time and lowering-time validation cannot drift.
use crate::object_editor::{
    ITEM_BONUS_ARMOR_MAX, ITEM_BONUS_ATTACK_MAX, ITEM_BONUS_ATTR_MAX, ITEM_BONUS_HP_MAX,
    ITEM_BONUS_MANA_MAX, ITEM_CHARGES_MAX, ITEM_CLASSES, ITEM_DROP_ON_DEATH, ITEM_INTERVAL_MAX,
    ITEM_INVENTORY_FLAGS, ITEM_PICKUP_RANGE_MAX, ITEM_STOCK_MAX, SHOP_INTERVAL_MAX, SHOP_STOCK_MAX,
};
// #825 tech-tree + #827 gameplay-constants canonical range sets: the SAME bounds the lowering-time
// validators (`object_editor_tech_fields`, `object_editor_constants_fields`) check against.
use crate::object_editor::{
    CONVERSION_RATE_MAX, CYCLE_LEN_MAX, CYCLE_LEN_MIN, DROP_RATE_PCT_MAX, FOOD_CAP_MAX,
    HERO_LEVEL_MAX, REVIVE_GOLD_MAX, REVIVE_SECS_MAX, TECH_MAX_EQUIVALENTS, TECH_MAX_REQUIRES,
    TECH_TIER_MAX, XP_GAIN_BASE_MAX, XP_GAIN_PCT_MAX,
};

// #823/#824/#826/#828 p2 editors: the SAME canonical enum/flag/range sets the lowering-time
// validators use, so a bogus buff polarity, upgrade kind, destructible/doodad enum, or
// interface value is rejected at persist time and can never reach the Aster vault.
use crate::object_editor::{
    BUFF_AURA_RADIUS_MAX, BUFF_DURATION_SECS_MAX, DESTRUCTIBLE_HP_MAX, DOODAD_SCALE_MILLI_MAX,
    DOODAD_VARIATIONS_MAX, INTERFACE_CMD_PREFIX, INTERFACE_COLOR_PREFIX, MESSAGE_DURATION_SECS_MAX,
    MESSAGE_FONT_PCT_MAX, MESSAGE_FONT_PCT_MIN, UPGRADE_LEVEL_COUNT_MAX,
    WC3_BUFF_DURATION_SEMANTICS, WC3_BUFF_POLARITIES, WC3_BUFF_STACK_RULES,
    WC3_DESTRUCTIBLE_ARMOR_TYPES, WC3_DESTRUCTIBLE_KINDS, WC3_UPGRADE_EFFECT_KINDS, parse_cell,
    validate_hex_color,
};

#[path = "mcp_object_tools_validation_lists.rs"]
mod lists;
// #825/#827 numeric-list + float persist validators live in a child module so this file stays
// under the 500-line ratchet; they read the SAME canonical ranges the lowering validator uses.
#[path = "mcp_object_tools_validation_ranges.rs"]
mod ranges;
#[path = "mcp_object_tools_validation_values.rs"]
mod values;

use super::{GameplayTable, ObjectBaseTemplate, ObjectPropertyDelta, invalid_mcp_object};
use lists::{
    validate_ability_list, validate_enum_value, validate_flag_list, validate_id_list,
    validate_learnable_list, validate_model_path, validate_u32_list, validate_u32_range,
};
use ranges::{validate_bounded_id_list, validate_f64_range, validate_monotonic_u32_list};

const MAX_IDENTIFIER_LEN: usize = 128;
const MAX_PROPERTY_KEY_LEN: usize = 128;
const MAX_PROPERTY_VALUE_LEN: usize = 4096;
const MAX_PROPERTY_COUNT: usize = 512;
const MAX_GAMEPLAY_TABLES: usize = 16;
const MAX_GAMEPLAY_ROWS: usize = 256;

pub(super) fn validate_initial_document(
    document: Option<&EditorDocument>,
) -> Result<(), EngineError> {
    if let Some(document) = document {
        document.validate()?;
    }
    Ok(())
}

pub(super) fn validate_base_copy(
    new_object_id: &str,
    rawcode: &str,
    base: &ObjectBaseTemplate,
    overrides: &BTreeMap<String, String>,
) -> Result<(), EngineError> {
    validate_identifier("new_object_id", new_object_id)?;
    validate_rawcode("rawcode", rawcode)?;
    validate_identifier("base.base_object_id", &base.base_object_id)?;
    validate_single_line("base.display_name", &base.display_name)?;
    validate_property_map("base.properties", &base.properties, true)?;
    validate_property_map("overrides", overrides, true)?;
    Ok(())
}

pub(super) fn validate_object_delta(
    object_id: &str,
    delta: &ObjectPropertyDelta,
) -> Result<(), EngineError> {
    validate_identifier("object_id", object_id)?;
    if delta.set_kind.is_none()
        && delta.upsert_properties.is_empty()
        && delta.remove_properties.is_empty()
    {
        return Err(invalid_mcp_object(
            "delta",
            "at least one kind, upsert, or remove operation is required",
        ));
    }
    validate_property_map("delta.upsert_properties", &delta.upsert_properties, true)?;
    let mut removals = BTreeSet::new();
    for key in &delta.remove_properties {
        validate_property_key("delta.remove_properties", key)?;
        reject_reserved_property("delta.remove_properties", key)?;
        if !removals.insert(key.as_str()) {
            return Err(invalid_mcp_object(
                "delta.remove_properties",
                format!("duplicate remove key {key}"),
            ));
        }
        if delta.upsert_properties.contains_key(key) {
            return Err(invalid_mcp_object(
                "delta",
                format!("{key} cannot be both upserted and removed"),
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_gameplay_tables(tables: &[GameplayTable]) -> Result<(), EngineError> {
    if tables.is_empty() || tables.len() > MAX_GAMEPLAY_TABLES {
        return Err(invalid_mcp_object(
            "gameplay.tables",
            format!("table count must be in 1..={MAX_GAMEPLAY_TABLES}"),
        ));
    }
    let mut table_ids = BTreeSet::new();
    for table in tables {
        validate_key_component("gameplay.table_id", &table.table_id)?;
        validate_single_line("gameplay.category", &table.category)?;
        if !table_ids.insert(table.table_id.as_str()) {
            return Err(invalid_mcp_object(
                "gameplay.table_id",
                format!("duplicate table {}", table.table_id),
            ));
        }
        if table.rows.is_empty() || table.rows.len() > MAX_GAMEPLAY_ROWS {
            return Err(invalid_mcp_object(
                "gameplay.rows",
                format!("row count must be in 1..={MAX_GAMEPLAY_ROWS}"),
            ));
        }
        let mut row_ids = BTreeSet::new();
        for row in &table.rows {
            validate_key_component("gameplay.row_id", &row.row_id)?;
            if !row_ids.insert(row.row_id.as_str()) {
                return Err(invalid_mcp_object(
                    "gameplay.row_id",
                    format!("duplicate row {} in {}", row.row_id, table.table_id),
                ));
            }
            validate_property_map("gameplay.row.fields", &row.fields, false)?;
        }
    }
    Ok(())
}

pub(super) fn validate_single_line(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty()
        || value.len() > MAX_PROPERTY_VALUE_LEN
        || value.contains(['\n', '\r', '\t'])
    {
        return Err(invalid_mcp_object(
            field,
            "value must be non-empty bounded single-line text",
        ));
    }
    Ok(())
}

pub(super) fn validate_identifier(field: &str, value: &str) -> Result<(), EngineError> {
    validate_single_line(field, value)?;
    if value.len() > MAX_IDENTIFIER_LEN {
        return Err(invalid_mcp_object(
            field,
            format!("identifier must be at most {MAX_IDENTIFIER_LEN} bytes"),
        ));
    }
    Ok(())
}

pub(super) fn gameplay_table_object_id(table_id: &str) -> String {
    format!("gameplay.table.{table_id}")
}

pub fn sample_issue919_document() -> EditorDocument {
    let mut document = EditorDocument::new(
        "calyx://world/issue919-mcp-objects",
        "issue919-object-rev-1",
    )
    .expect("issue919 sample document must validate");
    document
        .upsert_object(
            EditorObject::new("unit.issue919.seed", "Unit", None, Transform::IDENTITY)
                .with_property("object.rawcode", "uS19")
                .with_property("object.display_name", "Seed Footman")
                .with_property("stats.hit_points", "420"),
        )
        .expect("issue919 sample object must validate");
    document
}

fn validate_property_map(
    field: &str,
    properties: &BTreeMap<String, String>,
    reject_reserved: bool,
) -> Result<(), EngineError> {
    if properties.len() > MAX_PROPERTY_COUNT {
        return Err(invalid_mcp_object(
            field,
            format!("property count must be at most {MAX_PROPERTY_COUNT}"),
        ));
    }
    for (key, value) in properties {
        validate_property_key(field, key)?;
        if reject_reserved {
            reject_reserved_property(field, key)?;
        }
        validate_single_line(field, value)?;
        validate_known_property_value(field, key, value)?;
    }
    Ok(())
}

fn validate_known_property_value(field: &str, key: &str, value: &str) -> Result<(), EngineError> {
    match key {
        "stats.move_speed" => {
            validate_u32_range(field, key, value, UNIT_MOVE_SPEED_MIN, UNIT_MOVE_SPEED_MAX)
        }
        "stats.hit_points" => {
            validate_u32_range(field, key, value, UNIT_HIT_POINTS_MIN, UNIT_HIT_POINTS_MAX)
        }
        "stats.mana" => validate_u32_range(field, key, value, 0, UNIT_MANA_MAX),
        "abilities.normal" | "abilities.hero" => validate_ability_list(field, key, value),
        "ability.primary" => validate_identifier(key, value),
        "art.model" => validate_model_path(field, key, value),
        // #818 full WC3 unit field set: reject out-of-band numerics at persist time so a
        // poisoned stat can never reach the Aster vault (edge case: invalid -> nothing persisted).
        "stats.armor" => validate_u32_range(field, key, value, 0, UNIT_ARMOR_MAX),
        "art.scale_pct" => {
            validate_u32_range(field, key, value, UNIT_SCALE_PCT_MIN, UNIT_SCALE_PCT_MAX)
        }
        "combat.damage_dice" | "combat.damage_sides" => validate_u32_range(
            field,
            key,
            value,
            UNIT_DAMAGE_DICE_MIN,
            UNIT_DAMAGE_DICE_MAX,
        ),
        "combat.damage_base" => validate_u32_range(field, key, value, 0, UNIT_DAMAGE_BASE_MAX),
        "combat.attack_cooldown_ms" => validate_u32_range(
            field,
            key,
            value,
            UNIT_ATTACK_COOLDOWN_MS_MIN,
            UNIT_ATTACK_COOLDOWN_MS_MAX,
        ),
        "combat.attack_range" | "movement.height" => {
            validate_u32_range(field, key, value, 0, UNIT_ATTACK_RANGE_MAX)
        }
        "movement.turn_rate_pct" => {
            validate_u32_range(field, key, value, 0, UNIT_TURN_RATE_PCT_MAX)
        }
        "economy.gold_cost" => validate_u32_range(field, key, value, 0, UNIT_GOLD_COST_MAX),
        "economy.lumber_cost" => validate_u32_range(field, key, value, 0, UNIT_LUMBER_COST_MAX),
        "economy.food_cost" => validate_u32_range(field, key, value, 0, UNIT_FOOD_COST_MAX),
        "economy.build_time_secs" => {
            validate_u32_range(field, key, value, 0, UNIT_BUILD_TIME_SECS_MAX)
        }
        // #818 full WC3 unit field set: reject out-of-band ENUMS and FLAGS at persist time too,
        // against the SAME canonical sets the lowering-time validator uses, so a bogus
        // attack/armor/movement type or targeting flag can never reach the Aster vault and only
        // fail later at lowering (edge case: invalid -> nothing persisted).
        "combat.attack_type" => validate_enum_value(field, key, value, WC3_ATTACK_TYPES),
        "combat.armor_type" => validate_enum_value(field, key, value, WC3_ARMOR_TYPES),
        "movement.type" => validate_enum_value(field, key, value, WC3_MOVE_TYPES),
        "combat.targets_allowed" => validate_flag_list(field, key, value, WC3_TARGET_FLAGS),
        // #819 ability field set: reject out-of-band enums/flags/ranges at persist time against
        // the SAME canonical sets the lowering-time validator uses (single source of truth), so a
        // bogus ability can never reach the Aster vault and only fail later at lowering.
        "ability.kind" => validate_enum_value(field, key, value, ABILITY_KINDS),
        "ability.targeting" => validate_enum_value(field, key, value, ABILITY_TARGET_TYPES),
        "ability.targets_allowed" => validate_flag_list(field, key, value, WC3_TARGET_FLAGS),
        "ability.mana_cost" => validate_u32_range(field, key, value, 0, ABILITY_MANA_MAX),
        "ability.cooldown_ms" => validate_u32_range(field, key, value, 0, ABILITY_COOLDOWN_MAX),
        "ability.duration_ms" => validate_u32_range(field, key, value, 0, ABILITY_DURATION_MAX),
        "ability.cast_range" => validate_u32_range(field, key, value, 0, ABILITY_RANGE_MAX),
        "ability.area_of_effect" => validate_u32_range(field, key, value, 0, ABILITY_AOE_MAX),
        "ability.levels" => {
            validate_u32_range(field, key, value, ABILITY_LEVELS_MIN, ABILITY_LEVELS_MAX)
        }
        "ability.scaling.base_damage"
        | "ability.scaling.mana_cost"
        | "ability.scaling.duration_ms"
        | "ability.scaling.area_of_effect" => {
            validate_u32_list(field, key, value, 0, ABILITY_SCALE_MAX)
        }
        "ability.effects" | "ability.requirements" => validate_id_list(field, key, value),
        // #820 hero field set: same single-source-of-truth persist-time rejection.
        "hero.primary_attribute" => validate_enum_value(field, key, value, HERO_PRIMARY_ATTRS),
        "hero.flags" => validate_flag_list(field, key, value, HERO_FLAGS),
        "hero.str_base" | "hero.agi_base" | "hero.int_base" => {
            validate_u32_range(field, key, value, 0, HERO_ATTR_BASE_MAX)
        }
        "hero.str_gain_milli" | "hero.agi_gain_milli" | "hero.int_gain_milli" => {
            validate_u32_range(field, key, value, 0, HERO_ATTR_GAIN_MILLI_MAX)
        }
        "hero.revive_base_time_secs" | "hero.revive_time_per_level_secs" => {
            validate_u32_range(field, key, value, 0, HERO_REVIVE_TIME_MAX)
        }
        "hero.revive_base_cost_gold" | "hero.revive_cost_per_level_gold" => {
            validate_u32_range(field, key, value, 0, HERO_REVIVE_COST_MAX)
        }
        "hero.learnable" => validate_learnable_list(field, key, value),
        // #821 item field set: reject out-of-band item enums/flags/ranges at persist time from the
        // SAME canonical sets the item lowering validator uses (economy.gold_cost/lumber_cost are
        // already covered by the shared unit arm above at the identical shared cost max bound).
        "item.class" => validate_enum_value(field, key, value, ITEM_CLASSES),
        "item.inventory_flags" => validate_flag_list(field, key, value, ITEM_INVENTORY_FLAGS),
        "stock.stock_max" => validate_u32_range(field, key, value, 0, ITEM_STOCK_MAX),
        "stock.replenish_interval_secs" | "stock.start_delay_secs" => {
            validate_u32_range(field, key, value, 0, ITEM_INTERVAL_MAX)
        }
        "charges.initial" | "charges.max" => {
            validate_u32_range(field, key, value, 0, ITEM_CHARGES_MAX)
        }
        "bonus.attack" => validate_u32_range(field, key, value, 0, ITEM_BONUS_ATTACK_MAX),
        "bonus.armor" => validate_u32_range(field, key, value, 0, ITEM_BONUS_ARMOR_MAX),
        "bonus.hit_points" => validate_u32_range(field, key, value, 0, ITEM_BONUS_HP_MAX),
        "bonus.mana" => validate_u32_range(field, key, value, 0, ITEM_BONUS_MANA_MAX),
        "bonus.strength" | "bonus.agility" | "bonus.intelligence" => {
            validate_u32_range(field, key, value, 0, ITEM_BONUS_ATTR_MAX)
        }
        "ability.active" | "ability.passive" => values::validate_id_list_value(field, key, value),
        "pickup.range" => validate_u32_range(field, key, value, 0, ITEM_PICKUP_RANGE_MAX),
        "pickup.drop_on_death" => validate_enum_value(field, key, value, ITEM_DROP_ON_DEATH),
        // #822 commerce records: shape-check refs/ranges/bools at persist time; the authoritative
        // CROSS-record validation (dangling refs, cycles) happens at lowering against the document.
        "recipe.components" | "shop.stock" => values::validate_id_list_value(field, key, value),
        "recipe.result" => validate_identifier(key, value),
        "shop.replenish_interval_secs" => {
            validate_u32_range(field, key, value, 0, SHOP_INTERVAL_MAX)
        }
        "shop.stock_max" => validate_u32_range(field, key, value, 0, SHOP_STOCK_MAX),
        "shop.buy_enabled" | "shop.sell_enabled" => values::validate_bool_value(field, key, value),
        "drop.entries" => values::validate_drop_entries_value(field, key, value),
        // #825 tech records: reject out-of-band tier / malformed prerequisite lists at persist
        // time against the SAME bounds the lowering-time graph validator uses. Cross-record
        // dangling/cycle checks are inherently a lowering-time (whole-graph) concern.
        "tech.tier" => validate_u32_range(field, key, value, 0, TECH_TIER_MAX),
        "tech.requires" => validate_bounded_id_list(field, key, value, TECH_MAX_REQUIRES),
        "tech.equivalents" => validate_bounded_id_list(field, key, value, TECH_MAX_EQUIVALENTS),
        // #827 gameplay constants: reject out-of-band numerics/floats at persist time against
        // the SAME documented ranges the lowering-time validator uses (single source of truth).
        // NOTE: `hero.revive_time_per_level_secs` is validated by the #820 hero arm above at the
        // identical 0..=REVIVE_SECS_MAX (100_000) bound, so it is not repeated here (a duplicate
        // literal arm would be unreachable).
        "xp.level_thresholds" => validate_monotonic_u32_list(field, key, value),
        "xp.gain_base" => validate_u32_range(field, key, value, 0, XP_GAIN_BASE_MAX),
        "xp.gain_per_level_pct" => validate_u32_range(field, key, value, 0, XP_GAIN_PCT_MAX),
        "cycle.day_length_secs" | "cycle.night_length_secs" => {
            validate_u32_range(field, key, value, CYCLE_LEN_MIN, CYCLE_LEN_MAX)
        }
        "hero.revive_time_base_secs" => validate_u32_range(field, key, value, 0, REVIVE_SECS_MAX),
        "hero.revive_gold_per_level" => validate_u32_range(field, key, value, 0, REVIVE_GOLD_MAX),
        "hero.str_to_hp"
        | "hero.str_to_hp_regen"
        | "hero.agi_to_armor"
        | "hero.agi_to_attack_speed_pct"
        | "hero.int_to_mana"
        | "hero.int_to_mana_regen" => validate_f64_range(field, key, value, CONVERSION_RATE_MAX),
        "item.drop_rate_pct" | "item.drop_rate_max_pct" => {
            validate_f64_range(field, key, value, DROP_RATE_PCT_MAX)
        }
        "upkeep.no_upkeep_food_cap"
        | "upkeep.low_upkeep_food_cap"
        | "upkeep.high_upkeep_food_max" => validate_u32_range(field, key, value, 0, FOOD_CAP_MAX),
        "misc.max_hero_level" => validate_u32_range(field, key, value, 1, HERO_LEVEL_MAX),
        // #823 buffs/effects (canonical sets shared with object_editor_buff_fields).
        "buff.polarity" => validate_enum_value(field, key, value, WC3_BUFF_POLARITIES),
        "buff.stacking_rule" => validate_enum_value(field, key, value, WC3_BUFF_STACK_RULES),
        "buff.duration_semantics" => {
            validate_enum_value(field, key, value, WC3_BUFF_DURATION_SEMANTICS)
        }
        "buff.aura_radius" => validate_u32_range(field, key, value, 0, BUFF_AURA_RADIUS_MAX),
        "buff.aura_targets" => validate_flag_list(field, key, value, WC3_TARGET_FLAGS),
        "buff.duration_secs" => validate_u32_range(field, key, value, 0, BUFF_DURATION_SECS_MAX),
        // #824 upgrades (per-level vector lengths are cross-field, checked at lowering).
        "upgrade.effect_kind" => validate_enum_value(field, key, value, WC3_UPGRADE_EFFECT_KINDS),
        "upgrade.level_count" => validate_u32_range(field, key, value, 1, UPGRADE_LEVEL_COUNT_MAX),
        // #826 destructibles & doodads (armor<hp and min<=max are cross-field, checked at lowering).
        "destructible.kind" => validate_enum_value(field, key, value, WC3_DESTRUCTIBLE_KINDS),
        "destructible.max_hp" => validate_u32_range(field, key, value, 1, DESTRUCTIBLE_HP_MAX),
        "destructible.armor" => validate_u32_range(field, key, value, 0, DESTRUCTIBLE_HP_MAX),
        "destructible.armor_type" => {
            validate_enum_value(field, key, value, WC3_DESTRUCTIBLE_ARMOR_TYPES)
        }
        "destructible.targeted_as" => validate_flag_list(field, key, value, WC3_TARGET_FLAGS),
        "doodad.variations" => validate_u32_range(field, key, value, 1, DOODAD_VARIATIONS_MAX),
        "doodad.min_scale_milli" | "doodad.max_scale_milli" => {
            validate_u32_range(field, key, value, 1, DOODAD_SCALE_MILLI_MAX)
        }
        // #828 Game Interface scalar params.
        "interface.message_duration_secs" => {
            validate_u32_range(field, key, value, 0, MESSAGE_DURATION_SECS_MAX)
        }
        "interface.message_font_pct" => validate_u32_range(
            field,
            key,
            value,
            MESSAGE_FONT_PCT_MIN,
            MESSAGE_FONT_PCT_MAX,
        ),
        // #828 variable-count prefixed interface overrides (color palette + command-card cells).
        _ => validate_interface_prefixed(field, key, value),
    }
}

/// Persist-time validation for #828 prefixed interface override keys. A color override must be a
/// 6-digit hex RGB; a command-card cell must parse as `col,row` inside the 4x3 card (the SAME
/// `parse_cell`/`validate_hex_color` the lowering-time validator uses). Cross-key rules
/// (no duplicate cell) are enforced at lowering. All other keys pass.
fn validate_interface_prefixed(field: &str, key: &str, value: &str) -> Result<(), EngineError> {
    if let Some(slot) = key.strip_prefix(INTERFACE_COLOR_PREFIX) {
        return validate_hex_color(slot, value)
            .map_err(|error| invalid_mcp_object(format!("{field}.{key}"), error.to_string()));
    }
    if let Some(command) = key.strip_prefix(INTERFACE_CMD_PREFIX) {
        return parse_cell(command, value)
            .map(|_| ())
            .map_err(|error| invalid_mcp_object(format!("{field}.{key}"), error.to_string()));
    }
    Ok(())
}

fn validate_property_key(field: &str, key: &str) -> Result<(), EngineError> {
    if key.trim().is_empty()
        || key.len() > MAX_PROPERTY_KEY_LEN
        || !key.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(invalid_mcp_object(
            field,
            "property keys must be non-empty bounded printable ASCII",
        ));
    }
    Ok(())
}

fn validate_key_component(field: &str, value: &str) -> Result<(), EngineError> {
    validate_identifier(field, value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(invalid_mcp_object(
            field,
            "key components must use ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_rawcode(field: &str, value: &str) -> Result<(), EngineError> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(invalid_mcp_object(
            field,
            "rawcode must be exactly four printable ASCII bytes",
        ));
    }
    Ok(())
}

fn reject_reserved_property(field: &str, key: &str) -> Result<(), EngineError> {
    if matches!(
        key,
        "object.base_id" | "object.rawcode" | "object.display_name" | "object.category"
    ) {
        return Err(invalid_mcp_object(
            field,
            format!("{key} is reserved for object tool provenance"),
        ));
    }
    Ok(())
}
