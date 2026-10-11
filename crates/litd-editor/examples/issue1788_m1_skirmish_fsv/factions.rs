//! #1788 phase-1 content: the two asymmetric factions authored through the REAL object-data
//! editors (units #818, heroes #820, abilities #819, items #821, tech #825, upgrades #824).
//! Every record is an `EditorObject` in the property-bag model the editors lower from; nothing
//! game-specific is coded in Rust (#1071/#1312). Art refs bind to REAL vault records so the
//! rich unit lowering resolves them to content hashes.

use litd_editor::asset_products::assign_vault_model_to_unit;
use litd_editor::asset_vault::AssetVaultRecord;
use litd_editor::history::{EditorDocument, EditorObject};
use litd_engine::EngineError;
use litd_engine::math::Transform;

use crate::faction_data::{StructRow, UnitRow, tables};

/// A faction identity. Asymmetric factions differ in unit stats/costs and constants deltas.
/// Emberhold Compact = disciplined fortress-builders; Veilspun Court = swift shadow-weavers.
pub struct FactionSpec {
    pub key: &'static str,
    pub owner: &'static str,
}

pub const EMBERHOLD: FactionSpec = FactionSpec {
    key: "emberhold",
    owner: "player_one",
};
pub const VEILSPUN: FactionSpec = FactionSpec {
    key: "veilspun",
    owner: "player_two",
};

/// Exact phase-1 census counts per faction (asserted by the census pass).
pub const UNITS_PER_FACTION: usize = 8;
pub const STRUCTURES_PER_FACTION: usize = 6;
pub const HEROES_PER_FACTION: usize = 2;
pub const ABILITIES_PER_FACTION: usize = 6;
pub const ITEMS_PER_FACTION: usize = 2;
pub const TECH_PER_FACTION: usize = 5;
pub const UPGRADES_PER_FACTION: usize = 2;

/// The authored id lists a faction contributes (used by the map/AI + census).
#[derive(Debug, Clone, Default)]
pub struct FactionIds {
    pub units: Vec<String>,
    pub structures: Vec<String>,
    pub heroes: Vec<String>,
    pub abilities: Vec<String>,
    pub items: Vec<String>,
    pub tech: Vec<String>,
    pub upgrades: Vec<String>,
    pub hall_id: String,
    pub army_id: String,
    pub barracks_id: String,
}

pub fn unit_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_unit_{suffix}", f.key)
}
pub fn struct_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_bld_{suffix}", f.key)
}
fn hero_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_hero_{suffix}", f.key)
}
fn ability_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_ab_{suffix}", f.key)
}
fn item_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_item_{suffix}", f.key)
}
pub fn tech_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_tech_{suffix}", f.key)
}
fn upgrade_id(f: &FactionSpec, suffix: &str) -> String {
    format!("{}_upg_{suffix}", f.key)
}

/// A monotonically increasing 4-char rawcode source, unique across the whole document.
pub struct RawcodeGen {
    next: u32,
}
impl RawcodeGen {
    pub fn new() -> Self {
        Self { next: 1000 }
    }
    fn take(&mut self) -> String {
        let code = format!("{:04}", self.next);
        self.next += 1;
        code
    }
}

fn obj(id: &str, kind: &str, props: &[(&str, String)]) -> EditorObject {
    let mut object = EditorObject::new(id, kind, Some("world".to_string()), Transform::IDENTITY);
    for (key, value) in props {
        object = object.with_property(*key, value.clone());
    }
    object
}

fn upgrade_props(
    rawcode: String,
    name: &str,
    effect: &str,
    deltas: &[i32],
) -> Vec<(&'static str, String)> {
    let n = deltas.len();
    vec![
        ("object.rawcode", rawcode),
        ("object.base_id", "base.upgrade".to_string()),
        ("object.display_name", name.to_string()),
        ("upgrade.effect_kind", effect.to_string()),
        ("upgrade.level_count", n.to_string()),
        (
            "upgrade.level_deltas",
            deltas
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ),
        (
            "upgrade.gold_costs",
            vec!["100", "200", "300"][..n].join(","),
        ),
        ("upgrade.lumber_costs", vec!["0", "0", "0"][..n].join(",")),
        (
            "upgrade.research_times",
            vec!["30", "60", "90"][..n].join(","),
        ),
    ]
}

/// Author ONLY the upgrade records (parentless object-data doc) for persistence to the Calyx
/// Aster object store; the p2 (upgrade) valve re-reads them from that persisted document.
pub fn author_upgrade_records(
    document: &mut EditorDocument,
    f: &FactionSpec,
    rawcodes: &mut RawcodeGen,
) -> Result<Vec<String>, EngineError> {
    let (.., upgrades) = tables(f);
    let mut ids = Vec::new();
    for (suffix, name, effect, deltas) in upgrades {
        let id = upgrade_id(f, suffix);
        let mut object = EditorObject::new(&id, "Upgrade", None, Transform::IDENTITY);
        for (key, value) in upgrade_props(rawcodes.take(), name, effect, deltas) {
            object = object.with_property(key, value);
        }
        document.upsert_object(object)?;
        ids.push(id);
    }
    Ok(ids)
}

fn author_unit_row(
    document: &mut EditorDocument,
    f: &FactionSpec,
    idx: usize,
    row: &UnitRow,
    rawcodes: &mut RawcodeGen,
    unit_art: &AssetVaultRecord,
) -> Result<String, EngineError> {
    let (suffix, name, hp, speed, gold, lumber, food, dmg) = row;
    let id = unit_id(f, suffix);
    let role = if idx == 0 { "worker" } else { "soldier" };
    document.upsert_object(obj(
        &id,
        "Unit",
        &[
            ("object.rawcode", rawcodes.take()),
            ("object.base_id", "base.unit".to_string()),
            ("object.display_name", name.to_string()),
            ("stats.hit_points", hp.to_string()),
            ("stats.move_speed", speed.to_string()),
            ("art.model", unit_art.relative_path.clone()),
            ("combat.damage_base", dmg.to_string()),
            ("runtime.role", role.to_string()),
            ("economy.gold_cost", gold.to_string()),
            ("economy.lumber_cost", lumber.to_string()),
            ("economy.food_cost", food.to_string()),
            ("economy.build_time_secs", "1".to_string()),
        ],
    ))?;
    assign_vault_model_to_unit(document, &id, unit_art)?;
    Ok(id)
}

fn author_struct_row(
    document: &mut EditorDocument,
    f: &FactionSpec,
    row: &StructRow,
    rawcodes: &mut RawcodeGen,
    struct_art: &AssetVaultRecord,
) -> Result<String, EngineError> {
    let (suffix, name, hp, gold, foot, food_prov, trains) = row;
    let id = struct_id(f, suffix);
    let trains_ids: Vec<String> = trains.iter().map(|s| unit_id(f, s)).collect();
    document.upsert_object(obj(
        &id,
        "Unit",
        &[
            ("object.rawcode", rawcodes.take()),
            ("object.base_id", "base.structure".to_string()),
            ("object.display_name", name.to_string()),
            ("stats.hit_points", hp.to_string()),
            ("stats.move_speed", "1".to_string()),
            ("art.model", struct_art.relative_path.clone()),
            ("runtime.role", "structure".to_string()),
            ("economy.gold_cost", gold.to_string()),
            ("economy.build_time_secs", "1".to_string()),
            ("economy.food_provided", food_prov.to_string()),
            ("runtime.footprint_w", foot.to_string()),
            ("runtime.footprint_h", foot.to_string()),
            ("runtime.food_provided", food_prov.to_string()),
            ("runtime.trains", trains_ids.join(",")),
        ],
    ))?;
    assign_vault_model_to_unit(document, &id, struct_art)?;
    Ok(id)
}

/// Author one faction's full content into the document, binding art to real vault records.
/// `unit_art`/`struct_art` are real mesh records; heroes carry the humanoid relative path.
pub fn author_faction(
    document: &mut EditorDocument,
    f: &FactionSpec,
    rawcodes: &mut RawcodeGen,
    unit_art: &AssetVaultRecord,
    struct_art: &AssetVaultRecord,
) -> Result<FactionIds, EngineError> {
    let (units, structs, heroes, abilities, items, tech, upgrades) = tables(f);
    let mut ids = FactionIds::default();

    // --- Units (#818): rich stat/econ record + runtime role, art bound to a real mesh. ---
    for (idx, row) in units.iter().enumerate() {
        ids.units
            .push(author_unit_row(document, f, idx, row, rawcodes, unit_art)?);
    }
    // --- Structures (#818, role=structure): footprint + trains list. ---
    for row in structs {
        ids.structures
            .push(author_struct_row(document, f, row, rawcodes, struct_art)?);
    }
    ids.hall_id = struct_id(f, structs[0].0);
    ids.barracks_id = struct_id(f, structs[2].0);
    ids.army_id = unit_id(f, units[1].0);

    // --- Abilities (#819): hero-learnable so heroes may reference them. ---
    for (suffix, name) in abilities {
        let id = ability_id(f, suffix);
        document.upsert_object(obj(
            &id,
            "Ability",
            &[
                ("object.rawcode", rawcodes.take()),
                ("object.base_id", "base.ability".to_string()),
                ("object.display_name", name.to_string()),
                ("ability.kind", "hero-learnable".to_string()),
                ("ability.levels", "3".to_string()),
                ("ability.mana_cost", "75".to_string()),
                ("ability.cooldown_ms", "8000".to_string()),
                ("ability.targeting", "unit".to_string()),
                ("ability.scaling.base_damage", "50,100,150".to_string()),
            ],
        ))?;
        ids.abilities.push(id);
    }

    // --- Heroes (#820): STR/AGI/INT identity + learnable ability refs (cross-checked). ---
    for (suffix, name, attr, hp, speed, learn) in heroes {
        let id = hero_id(f, suffix);
        let learnable = learn
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}@{}", ability_id(f, s), i + 1))
            .collect::<Vec<_>>()
            .join(",");
        document.upsert_object(obj(
            &id,
            "Hero",
            &[
                ("object.rawcode", rawcodes.take()),
                ("object.base_id", "base.hero".to_string()),
                ("object.display_name", name.to_string()),
                ("stats.hit_points", hp.to_string()),
                ("stats.move_speed", speed.to_string()),
                ("art.model", unit_art.relative_path.clone()),
                ("hero.primary_attribute", attr.to_string()),
                ("hero.str_base", "22".to_string()),
                ("hero.agi_base", "20".to_string()),
                ("hero.int_base", "18".to_string()),
                ("hero.str_gain_milli", "2600".to_string()),
                ("hero.agi_gain_milli", "2400".to_string()),
                ("hero.int_gain_milli", "2200".to_string()),
                ("hero.learnable", learnable),
                ("hero.flags", "hero_glow,revivable".to_string()),
            ],
        ))?;
        ids.heroes.push(id);
    }

    // --- Items (#821): starting-kit records with attribute bonuses. ---
    for (suffix, name, class, gold, bonus_key, bonus_val) in items {
        let id = item_id(f, suffix);
        document.upsert_object(obj(
            &id,
            "Item",
            &[
                ("object.rawcode", rawcodes.take()),
                ("object.base_id", "base.item".to_string()),
                ("object.display_name", name.to_string()),
                ("item.class", class.to_string()),
                ("economy.gold_cost", gold.to_string()),
                ("charges.initial", "1".to_string()),
                ("charges.max", "3".to_string()),
                (bonus_key, bonus_val.to_string()),
            ],
        ))?;
        ids.items.push(id);
    }

    // --- Tech (#825): 2-tier tree, tier-2 nodes require tier-1 prerequisites. ---
    for (suffix, name, tier, requires) in tech {
        let id = tech_id(f, suffix);
        let requires_ids: Vec<String> = requires.iter().map(|s| tech_id(f, s)).collect();
        document.upsert_object(obj(
            &id,
            "Tech",
            &[
                ("object.rawcode", rawcodes.take()),
                ("object.display_name", name.to_string()),
                ("tech.tier", tier.to_string()),
                ("tech.requires", requires_ids.join(",")),
            ],
        ))?;
        ids.tech.push(id);
    }

    // --- Upgrades (#824): multi-level research with per-level deltas + costs. ---
    for (suffix, name, effect, deltas) in upgrades {
        let id = upgrade_id(f, suffix);
        document.upsert_object(obj(
            &id,
            "Upgrade",
            &upgrade_props(rawcodes.take(), name, effect, deltas),
        ))?;
        ids.upgrades.push(id);
    }

    Ok(ids)
}
