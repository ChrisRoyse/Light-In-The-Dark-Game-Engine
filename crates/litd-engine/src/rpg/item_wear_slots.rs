//! The full Carrion Fields wear-slot equipment model, including racial slots
//! (#1683).
//!
//! Facts are transcribed, never invented (#984/#959): the slot set is the CF
//! item-database "Slot" filter (`carrionfields.net/itemsearch`, 2026-07-12):
//! light, finger, neck, body, head, legs, feet, hands, arms, shield, about
//! (cloak), waist, wrist, wield, hold, back, face, ears -- PLUS the racial
//! slots claws / foreclaws / forepaws / hindpaws (felar), hooves (centaur),
//! horns (minotaur), tail (saurian), and wings (arial / Sylphkin). Racial
//! anatomy IS an equipment surface (#946): a centaur shoes its hooves, an arial
//! armors its wings.
//!
//! Every slot maps to a model attachment point (#782 rigs); the attachment id is
//! recorded here so the render layer has a single source of truth for where a
//! worn item hangs on the skeleton.
//!
//! A racial-slot item is GATED to the races that possess that anatomy. The
//! gating race ids are validated for referential integrity against the authored
//! `RaceSpec` corpus by [`super::race_registry`], reusing the #1631 registry
//! pattern (per-item validation stays form-only; existence is a corpus check).

use serde::{Deserialize, Serialize};

use crate::EngineError;

/// One equipment wear location. `Take` models a carried-but-not-worn item
/// (potions, keys, most treasure-in-inventory) so consumables have a valid slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WearSlot {
    // Non-wearable: carried in inventory only.
    Take,
    // Standard humanoid slots.
    Light,
    Finger,
    Neck,
    Body,
    Head,
    Legs,
    Feet,
    Hands,
    Arms,
    Shield,
    About,
    Waist,
    Wrist,
    Wield,
    Hold,
    Back,
    Face,
    Ears,
    // Racial slots (anatomy-gated).
    Claws,
    Foreclaws,
    Forepaws,
    Hindpaws,
    Hooves,
    Horns,
    Tail,
    Wings,
}

impl WearSlot {
    pub const ALL: [WearSlot; 27] = [
        WearSlot::Take,
        WearSlot::Light,
        WearSlot::Finger,
        WearSlot::Neck,
        WearSlot::Body,
        WearSlot::Head,
        WearSlot::Legs,
        WearSlot::Feet,
        WearSlot::Hands,
        WearSlot::Arms,
        WearSlot::Shield,
        WearSlot::About,
        WearSlot::Waist,
        WearSlot::Wrist,
        WearSlot::Wield,
        WearSlot::Hold,
        WearSlot::Back,
        WearSlot::Face,
        WearSlot::Ears,
        WearSlot::Claws,
        WearSlot::Foreclaws,
        WearSlot::Forepaws,
        WearSlot::Hindpaws,
        WearSlot::Hooves,
        WearSlot::Horns,
        WearSlot::Tail,
        WearSlot::Wings,
    ];

    pub const fn as_id(self) -> &'static str {
        match self {
            WearSlot::Take => "take",
            WearSlot::Light => "light",
            WearSlot::Finger => "finger",
            WearSlot::Neck => "neck",
            WearSlot::Body => "body",
            WearSlot::Head => "head",
            WearSlot::Legs => "legs",
            WearSlot::Feet => "feet",
            WearSlot::Hands => "hands",
            WearSlot::Arms => "arms",
            WearSlot::Shield => "shield",
            WearSlot::About => "about",
            WearSlot::Waist => "waist",
            WearSlot::Wrist => "wrist",
            WearSlot::Wield => "wield",
            WearSlot::Hold => "hold",
            WearSlot::Back => "back",
            WearSlot::Face => "face",
            WearSlot::Ears => "ears",
            WearSlot::Claws => "claws",
            WearSlot::Foreclaws => "foreclaws",
            WearSlot::Forepaws => "forepaws",
            WearSlot::Hindpaws => "hindpaws",
            WearSlot::Hooves => "hooves",
            WearSlot::Horns => "horns",
            WearSlot::Tail => "tail",
            WearSlot::Wings => "wings",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        WearSlot::ALL.into_iter().find(|s| s.as_id() == id)
    }

    /// True for the anatomy-gated racial slots (claws/paws/hooves/horns/tail/
    /// wings) that only some races possess.
    pub const fn is_racial(self) -> bool {
        matches!(
            self,
            WearSlot::Claws
                | WearSlot::Foreclaws
                | WearSlot::Forepaws
                | WearSlot::Hindpaws
                | WearSlot::Hooves
                | WearSlot::Horns
                | WearSlot::Tail
                | WearSlot::Wings
        )
    }

    /// The `#782` model attachment point a worn item hangs from on the skeleton.
    /// `Take` items are never attached; the render layer keys off `None`.
    pub const fn attachment_point(self) -> Option<&'static str> {
        Some(match self {
            WearSlot::Take => return None,
            WearSlot::Light => "attach.hand.light",
            WearSlot::Finger => "attach.hand.finger",
            WearSlot::Neck => "attach.neck",
            WearSlot::Body => "attach.torso",
            WearSlot::Head => "attach.head",
            WearSlot::Legs => "attach.legs",
            WearSlot::Feet => "attach.feet",
            WearSlot::Hands => "attach.hands",
            WearSlot::Arms => "attach.arms",
            WearSlot::Shield => "attach.hand.offhand",
            WearSlot::About => "attach.torso.cloak",
            WearSlot::Waist => "attach.waist",
            WearSlot::Wrist => "attach.wrist",
            WearSlot::Wield => "attach.hand.main",
            WearSlot::Hold => "attach.hand.hold",
            WearSlot::Back => "attach.back",
            WearSlot::Face => "attach.face",
            WearSlot::Ears => "attach.ears",
            WearSlot::Claws => "attach.claws",
            WearSlot::Foreclaws => "attach.foreclaws",
            WearSlot::Forepaws => "attach.forepaws",
            WearSlot::Hindpaws => "attach.hindpaws",
            WearSlot::Hooves => "attach.hooves",
            WearSlot::Horns => "attach.horns",
            WearSlot::Tail => "attach.tail",
            WearSlot::Wings => "attach.wings",
        })
    }
}

/// A racial-slot item records which races may equip it. The gating race ids are
/// validated for existence against the authored `RaceSpec` corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WearGating {
    pub slot: WearSlot,
    /// Race ids that possess this anatomy (e.g. `race.felisar` for claws).
    /// MUST be non-empty for a racial slot, and MUST be empty for a standard slot.
    pub gating_race_ids: Vec<String>,
}

/// Validate one item's wear location.
///
/// - A standard (non-racial) slot must carry NO gating race ids (a body-armour
///   item is not race-locked by anatomy).
/// - A racial slot MUST name at least one gating race id (an unowned racial slot
///   would be unequippable by anyone).
/// - Every gating race id must be a well-formed token; duplicates fail loud.
///
/// Referential integrity (does `race.felisar` actually exist?) is a CORPUS-level
/// check performed by [`super::race_registry`], mirroring #1631/#1704 -- a single
/// item cannot know the whole race roster.
pub fn validate_wear_gating(gating: &WearGating) -> Result<(), EngineError> {
    if gating.slot.is_racial() {
        if gating.gating_race_ids.is_empty() {
            return Err(invalid(
                "wear.gating_race_ids",
                format!(
                    "racial slot {} must name at least one gating race id",
                    gating.slot.as_id()
                ),
            ));
        }
    } else if !gating.gating_race_ids.is_empty() {
        return Err(invalid(
            "wear.gating_race_ids",
            format!(
                "standard slot {} must not carry gating race ids",
                gating.slot.as_id()
            ),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for race_id in &gating.gating_race_ids {
        validate_token("wear.gating_race_ids", race_id)?;
        if !seen.insert(race_id.as_str()) {
            return Err(invalid(
                "wear.gating_race_ids",
                format!("duplicate gating race id {race_id}"),
            ));
        }
    }
    Ok(())
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return Err(invalid(
            field,
            "must be non-empty and <= 96 bytes".to_string(),
        ));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash".to_string(),
        ));
    }
    Ok(())
}

fn invalid(field: &str, reason: String) -> EngineError {
    EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason,
    }
}
