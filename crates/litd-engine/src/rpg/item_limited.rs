//! The Carrion Fields limited-item (rarity economy) data model (#1686).
//!
//! Facts are transcribed, never invented (#984/#959): the mechanics are CF
//! helpfile 61 (`LIMITED 'LIMITED EQUIPMENT' 'LIMITED OBJECTS' ILLUSIONARY`,
//! transcribed 2026-07-12):
//!   * limited items are world-capped and "too powerful to be placed inside a
//!     container" (the no-container rule);
//!   * players under level 11 "will find that they are too weak to keep these
//!     items while they are not present in Thera" (the L11 offline-keep gate);
//!   * when an object becomes "too widely spread ... they will occasionally
//!     crumble under certain circumstances" (the anti-hoard pressure valve);
//!   * over the limit, carriers display "Illusionary gear ... merely illusions
//!     of the limited gear" (the illusionary display); and
//!   * hoarding is a rules violation ("See also: HOARDING", #956).
//!
//! SCOPE: this module is the DATA MODEL + VALIDATION + readback (the authoring
//! surface). The live enforcement -- the global scarcity ledger authority
//! (#973), the offline sub-L11 purge, the actual crumble trigger under
//! overspread, and the runtime illusion swap -- is the game sim's runtime scope
//! and is recorded per spec in `runtime_enforcement_notes`, not enforced here.

use litd_hash::sha256_hex;

use crate::EngineError;

pub const RPG_LIMITED_ITEM_SPEC_SCHEMA: &str = "litd.rpg.limited_item.v1";

/// CF canon: characters below this level cannot keep a limited item while
/// offline (helpfile 61). The authored floor is validated `>= 1`; the CF value
/// is 11.
pub const CF_LIMITED_KEEP_LEVEL_FLOOR: u16 = 11;

/// The anti-hoard crumble rule (helpfile 61: overspread objects "occasionally
/// crumble under certain circumstances").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CrumbleRule {
    /// Whether overspread instances crumble. Intrinsic to a limited item
    /// (validated true) -- it is the pressure valve that pairs with 7-day decay
    /// (#978).
    pub crumble_on_overspread: bool,
    /// Human note describing the transcribed circumstance.
    pub overspread_note: String,
}

/// The illusionary-display rule (helpfile 61: over-limit carriers show illusions
/// of the gear, "no stats").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IllusionRule {
    /// Whether over-limit carriers display a stat-less illusion copy. Intrinsic
    /// to a limited item (validated true).
    pub illusion_over_limit: bool,
    /// Human note describing the RP/deception wrinkle.
    pub illusion_note: String,
}

/// A limited-item scarcity record: the rarity mechanics attached to a named
/// item. Keyed by `item_id` so it can bind to the item's [`super::ItemSpec`]
/// through the global ledger (#973) without duplicating its combat stats.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LimitedItemSpec {
    pub schema: String,
    /// Stable id for this limited record.
    pub limited_id: String,
    /// The item this scarcity record governs (references an `ItemSpec` id).
    pub item_id: String,
    pub display_name: String,
    /// Maximum instances allowed in the world (the "limited count"). A signed
    /// count so a ledger delta error surfaces as a loud negative rather than a
    /// silent wrap; validated `>= 1`.
    pub world_cap: i64,
    /// Whether the item may enter a container. CF: limited items are too
    /// powerful to be bagged -- validated FALSE (the no-container rule).
    pub may_enter_container: bool,
    /// Below this level a character cannot keep the item while offline.
    pub keep_level_floor: u16,
    pub crumble: CrumbleRule,
    pub illusion: IllusionRule,
    /// Whether hoarding this item is an Immortal-enforced rules violation (#956).
    pub hoarding_is_violation: bool,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    /// The enforcement that remains the game sim's runtime scope (ledger,
    /// offline purge, crumble trigger, illusion swap).
    pub runtime_enforcement_notes: Vec<String>,
    pub implementation_notes: Vec<String>,
}

/// The FSV readback surface: the scarcity mechanics + the deterministic hash.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LimitedItemSpecReadback {
    pub schema: String,
    pub limited_id: String,
    pub item_id: String,
    pub display_name: String,
    pub world_cap: i64,
    pub may_enter_container: bool,
    pub keep_level_floor: u16,
    pub crumble_on_overspread: bool,
    pub illusion_over_limit: bool,
    pub hoarding_is_violation: bool,
    pub runtime_note_count: usize,
    pub source_issue: u32,
    pub limited_item_sha256: String,
}

pub fn limited_item_readback(
    spec: &LimitedItemSpec,
) -> Result<LimitedItemSpecReadback, EngineError> {
    validate_limited_item_spec(spec)?;
    Ok(LimitedItemSpecReadback {
        schema: spec.schema.clone(),
        limited_id: spec.limited_id.clone(),
        item_id: spec.item_id.clone(),
        display_name: spec.display_name.clone(),
        world_cap: spec.world_cap,
        may_enter_container: spec.may_enter_container,
        keep_level_floor: spec.keep_level_floor,
        crumble_on_overspread: spec.crumble.crumble_on_overspread,
        illusion_over_limit: spec.illusion.illusion_over_limit,
        hoarding_is_violation: spec.hoarding_is_violation,
        runtime_note_count: spec.runtime_enforcement_notes.len(),
        source_issue: spec.source_issue,
        limited_item_sha256: spec_hash(spec)?,
    })
}

pub fn validate_limited_item_spec(spec: &LimitedItemSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_LIMITED_ITEM_SPEC_SCHEMA {
        return invalid("schema", format!("expected {RPG_LIMITED_ITEM_SPEC_SCHEMA}"));
    }
    validate_token("limited_id", &spec.limited_id)?;
    validate_token("item_id", &spec.item_id)?;
    validate_text("display_name", &spec.display_name)?;

    // Limited count: must be a real positive cap. Fails loud on zero AND on a
    // negative (ledger-delta) count.
    if spec.world_cap < 1 {
        return invalid(
            "world_cap",
            format!("limited count must be >= 1 (got {})", spec.world_cap),
        );
    }
    // No-container rule (helpfile 61): a limited item may never be bagged.
    if spec.may_enter_container {
        return invalid(
            "may_enter_container",
            "limited items are too powerful to enter a container (must be false)".to_string(),
        );
    }
    // L11 keep gate: an offline-keep floor of 0 is meaningless.
    if spec.keep_level_floor < 1 {
        return invalid(
            "keep_level_floor",
            "offline-keep level floor must be >= 1 (CF canon is 11)".to_string(),
        );
    }
    // The anti-hoard valve and the illusion display are intrinsic to a limited
    // item; a limited record that disables them contradicts helpfile 61.
    if !spec.crumble.crumble_on_overspread {
        return invalid(
            "crumble.crumble_on_overspread",
            "limited items crumble when overspread (must be true, helpfile 61)".to_string(),
        );
    }
    validate_text("crumble.overspread_note", &spec.crumble.overspread_note)?;
    if !spec.illusion.illusion_over_limit {
        return invalid(
            "illusion.illusion_over_limit",
            "over-limit carriers display illusion copies (must be true, helpfile 61)".to_string(),
        );
    }
    validate_text("illusion.illusion_note", &spec.illusion.illusion_note)?;

    if spec.source_issue == 0 {
        return invalid("source_issue", "must be non-zero".to_string());
    }
    if spec.canon_issues.is_empty() || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the source issue".to_string());
    }
    if !spec
        .mechanics_source_url
        .starts_with("https://carrionfields.net/")
    {
        return invalid(
            "mechanics_source_url",
            "must point to the approved CF factual mechanics source".to_string(),
        );
    }
    if spec.runtime_enforcement_notes.is_empty() {
        return invalid(
            "runtime_enforcement_notes",
            "must record which enforcement remains runtime scope".to_string(),
        );
    }
    if spec.implementation_notes.is_empty() {
        return invalid(
            "implementation_notes",
            "must include source notes".to_string(),
        );
    }
    Ok(())
}

fn spec_hash(spec: &LimitedItemSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize limited item spec: {error}"),
        })
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes".to_string());
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash".to_string(),
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty".to_string());
    }
    Ok(())
}

fn invalid(field: &str, reason: impl Into<String>) -> Result<(), EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.to_string(),
        reason: reason.into(),
    })
}
