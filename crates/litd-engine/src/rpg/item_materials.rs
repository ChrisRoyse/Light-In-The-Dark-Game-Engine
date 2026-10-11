//! The Carrion Fields item-material vocabulary and material-physics table
//! (#1685).
//!
//! Facts are transcribed, never invented (#984/#959): the material set is the CF
//! item-database "Material" filter (`carrionfields.net/itemsearch`, captured
//! 2026-07-12) -- the full 72-entry enum (adamantite..wood). Validation fails
//! LOUD (structured `LITD-RPG-STATS-INVALID-CONFIG`) on any material outside the
//! vocabulary, so a mistyped material can never persist.
//!
//! Material PHYSICS (the iron-burns-elves class of rules): a subset of materials
//! carry a physical affinity that interacts with a race's authored
//! resistance/vulnerability profile. Those affinities are wired to the SAME
//! [`RaceAffinity`] ids the #1631 race side already encodes (iron / silver /
//! mithril / whitesteel / wood) -- ONE vocabulary, referential integrity both
//! ways: [`ItemMaterial::physical_affinity_id`] emits an id, and
//! [`resolve_material_affinity`] fails LOUD if that id is not in the authored
//! race-affinity vocabulary. The interaction itself is read from the race's
//! authored `vulnerabilities`/`resistances` (single source of truth), never a
//! parallel hand-maintained table.

use super::race_specs_types::{RaceAffinity, RaceSpec};
use crate::EngineError;

macro_rules! item_materials {
    ($($variant:ident => $id:literal),+ $(,)?) => {
        /// The CF itemsearch "Material" vocabulary. A closed set: an item's
        /// material MUST resolve to one of these (validation fails loud otherwise).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum ItemMaterial {
            $($variant),+
        }

        impl ItemMaterial {
            /// Every material in the vocabulary, in itemsearch declaration order.
            pub const ALL: &'static [ItemMaterial] = &[$(ItemMaterial::$variant),+];

            /// The canonical snake_case id stored in `ItemSpec.material`.
            pub const fn as_id(self) -> &'static str {
                match self { $(ItemMaterial::$variant => $id),+ }
            }

            /// Resolve a stored material id, or `None` if it is not in the
            /// vocabulary (the loud-fail path for `validate_item_material`).
            pub fn from_id(id: &str) -> Option<Self> {
                match id { $($id => Some(ItemMaterial::$variant),)+ _ => None }
            }
        }
    };
}

item_materials! {
    Adamantite => "adamantite", Agate => "agate", Amber => "amber",
    Amethyst => "amethyst", Aquamarine => "aquamarine", Azurite => "azurite",
    Bloodstone => "bloodstone", Bone => "bone", Brass => "brass",
    Bronze => "bronze", Burlap => "burlap", Canvas => "canvas",
    Carrion => "carrion", Clay => "clay", Cloth => "cloth", Copper => "copper",
    Coral => "coral", Crystal => "crystal", Diamond => "diamond",
    Dragonbone => "dragonbone", Dragonscale => "dragonscale", Emerald => "emerald",
    Energy => "energy", Feathers => "feathers", Fire => "fire", Flesh => "flesh",
    Fur => "fur", Garnet => "garnet", Gem => "gem", Glass => "glass",
    Gold => "gold", Heartstone => "heartstone", Hide => "hide", Ice => "ice",
    Iron => "iron", Ivory => "ivory", Jade => "jade", Lapis => "lapis",
    Lead => "lead", Leather => "leather", Lightning => "lightning",
    Marble => "marble", Metal => "metal", Mithril => "mithril",
    Moonstone => "moonstone", Obsidian => "obsidian", Onyx => "onyx",
    Opal => "opal", Pearl => "pearl", Peridot => "peridot", Plant => "plant",
    Platinum => "platinum", Quartz => "quartz", Rope => "rope", Ruby => "ruby",
    Sapphire => "sapphire", Seashell => "seashell", Seaweed => "seaweed",
    Silk => "silk", Silver => "silver", Skin => "skin", Spidersilk => "spidersilk",
    Steel => "steel", Stone => "stone", Straw => "straw", Tin => "tin",
    Titanium => "titanium", Topaz => "topaz", Turquoise => "turquoise",
    Water => "water", Whitesteel => "whitesteel", Wood => "wood",
}

impl ItemMaterial {
    /// The race-affinity id (from the #1631 [`RaceAffinity`] vocabulary) a
    /// material physically interacts with on a hold/wear attempt, or `None` for
    /// an inert material (leather, cloth, gem, ...). These ids are exactly the
    /// material affinities the race side encodes as resistances/vulnerabilities:
    /// iron burns elves, mithril burns the drow-line, whitesteel burns the
    /// deep-dwarf line, silver bites certain lines, wood bites others.
    pub const fn physical_affinity_id(self) -> Option<&'static str> {
        Some(match self {
            ItemMaterial::Iron => "iron",
            ItemMaterial::Silver => "silver",
            ItemMaterial::Mithril => "mithril",
            ItemMaterial::Whitesteel => "whitesteel",
            ItemMaterial::Wood => "wood",
            _ => return None,
        })
    }

    /// Resolve this material's physical affinity to a [`RaceAffinity`], failing
    /// LOUD if the emitted id is not in the authored race-affinity vocabulary
    /// (the referential-integrity guard: material -> race-affinity ids must stay
    /// a single shared vocabulary). `Ok(None)` for an inert material.
    pub fn physical_affinity(self) -> Result<Option<RaceAffinity>, EngineError> {
        match self.physical_affinity_id() {
            Some(id) => Ok(Some(resolve_material_affinity(
                "material.physical_affinity",
                id,
            )?)),
            None => Ok(None),
        }
    }
}

/// The race-affinity ids a physical material may emit. Kept alongside
/// [`ItemMaterial::physical_affinity_id`] so the both-ways integrity test can
/// assert the mapping is a bijection onto exactly these RaceAffinity variants.
pub const MATERIAL_PHYSICS_AFFINITIES: [RaceAffinity; 5] = [
    RaceAffinity::Iron,
    RaceAffinity::Silver,
    RaceAffinity::Mithril,
    RaceAffinity::Whitesteel,
    RaceAffinity::Wood,
];

/// Resolve a material-physics affinity id against the authored race-affinity
/// vocabulary (#1631). Fails LOUD naming the id when it is not a real affinity,
/// which is the loud diagnostic for a material that names a race-side affinity id
/// that does not exist.
pub fn resolve_material_affinity(
    field: &str,
    affinity_id: &str,
) -> Result<RaceAffinity, EngineError> {
    RaceAffinity::from_id(affinity_id).ok_or_else(|| {
        invalid(
            field,
            format!(
                "material affinity {affinity_id} is not in the race affinity vocabulary (#1631)"
            ),
        )
    })
}

/// Validate that an item's material is in the CF vocabulary. Fails LOUD on an
/// unknown material.
pub fn validate_item_material(field: &str, material: &str) -> Result<(), EngineError> {
    if ItemMaterial::from_id(material).is_none() {
        return Err(invalid(
            field,
            format!("unknown material {material} (not in the CF itemsearch material vocabulary)"),
        ));
    }
    Ok(())
}

/// The typed outcome of holding/wearing a material against a specific race.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialRaceInteraction {
    /// The item material id.
    pub material_id: String,
    /// The race-affinity id the material interacts through.
    pub affinity_id: String,
    /// The race this interaction was computed for.
    pub race_id: String,
    /// True when the race is VULNERABLE to the affinity: the material burns them
    /// on a hold/wear attempt (the iron-burns-elves rule).
    pub burns: bool,
    /// True when the race RESISTS the affinity: the material is warded, not
    /// harmful.
    pub warded: bool,
    /// The signed magnitude from the race's authored profile (>0 vulnerability,
    /// <0 resistance amount as authored), or 0 when the race is neutral.
    pub severity: i32,
}

/// Compute the material-physics interaction between an item material and a race.
///
/// Returns `Ok(None)` when the material is inert (no physical affinity). For a
/// physical material it reads the race's AUTHORED `vulnerabilities` then
/// `resistances` for the material's affinity id (single source of truth -- the
/// #1631 race side), producing a typed [`MaterialRaceInteraction`]. Fails LOUD
/// only if the material emits an affinity id absent from the race vocabulary.
pub fn material_race_interaction(
    material: ItemMaterial,
    race: &RaceSpec,
) -> Result<Option<MaterialRaceInteraction>, EngineError> {
    let Some(affinity) = material.physical_affinity()? else {
        return Ok(None);
    };
    let affinity_id = affinity.as_id();
    let (burns, warded, severity) = if let Some(profile) = race
        .vulnerabilities
        .iter()
        .find(|p| p.resistance_id == affinity_id)
    {
        (true, false, profile.amount)
    } else if let Some(profile) = race
        .resistances
        .iter()
        .find(|p| p.resistance_id == affinity_id)
    {
        (false, true, profile.amount)
    } else {
        (false, false, 0)
    };
    Ok(Some(MaterialRaceInteraction {
        material_id: material.as_id().to_string(),
        affinity_id: affinity_id.to_string(),
        race_id: race.race_id.clone(),
        burns,
        warded,
        severity,
    }))
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
