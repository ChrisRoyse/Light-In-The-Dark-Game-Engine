//! Limited-item pilot fixtures (#1686): the CF rarity mechanics from helpfile 61
//! (transcribed 2026-07-12) attached to named limited items, with names/lore
//! originalized per #984 and every MECHANIC kept factual. The scarcity rules --
//! no-container, the L11 offline-keep gate, overspread crumble, and the
//! illusionary display -- are the transcribed CF facts; the per-item combat
//! stats live in a separate `ItemSpec` and are PBF/ledger scope (#973/#1688),
//! since limited items are excluded from the public itemsearch database.

use crate::rpg::{
    CF_LIMITED_KEEP_LEVEL_FLOOR, CrumbleRule, IllusionRule, LimitedItemSpec,
    RPG_LIMITED_ITEM_SPEC_SCHEMA,
};

pub const SUNDERED_CROWN_LIMITED_ID: &str = "limited.sundered_crown";
pub const RIMEBOUND_WARBLADE_LIMITED_ID: &str = "limited.rimebound_warblade";

/// A world-capped regalia piece: illustrates the illusionary-display wrinkle
/// (an over-limit pretender shows a stat-less illusion of the real crown).
pub(crate) fn sundered_crown_limited_spec() -> LimitedItemSpec {
    LimitedItemSpec {
        schema: RPG_LIMITED_ITEM_SPEC_SCHEMA.to_string(),
        limited_id: SUNDERED_CROWN_LIMITED_ID.to_string(),
        item_id: "item.limited.sundered_crown".to_string(),
        display_name: "the Sundered Crown of the Ash Throne".to_string(),
        // Regalia are among the scarcest limited items; a single-digit cap.
        world_cap: 3,
        may_enter_container: false,
        keep_level_floor: CF_LIMITED_KEEP_LEVEL_FLOOR,
        crumble: CrumbleRule {
            crumble_on_overspread: true,
            overspread_note: "When too many crowns circulate, an instance crumbles under \
                 certain circumstances (helpfile 61 anti-hoard valve; pairs with 7-day \
                 decay #978)."
                .to_string(),
        },
        illusion: IllusionRule {
            illusion_over_limit: true,
            illusion_note: "Over the limit, a carrier bears only an illusion of the crown -- \
                 no stats -- preserving the look while the ledger rebalances (is that REALLY \
                 the crown? #1622 veil vs peek)."
                .to_string(),
        },
        hoarding_is_violation: true,
        source_issue: 1686,
        canon_issues: vec![973, 974, 978, 989, 1686],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?id=61".to_string(),
        runtime_enforcement_notes: vec![
            "The global scarcity ledger (#973) is the count authority; this record is the \
             authored cap, not the live tally."
                .to_string(),
            "Sub-L11 offline-keep purge, the actual overspread crumble trigger, and the live \
             illusion swap are game-sim runtime scope (#975 always-lootable, #907 hoard \
             analytics)."
                .to_string(),
        ],
        implementation_notes: vec![
            "Scarcity mechanics transcribed from CF helpfile 61 (LIMITED / ILLUSIONARY); \
             container-forbidden, L11 keep gate, overspread crumble, illusion display are \
             factual."
                .to_string(),
            "Name/lore originalized per #984; the world_cap is an authored per-item cap \
             (limited items are excluded from itemsearch, so exact counts are PBF/ledger scope)."
                .to_string(),
        ],
    }
}

/// A world-capped weapon: illustrates the no-container rule (too powerful to be
/// bagged, always on the wielder's person, always lootable #975).
pub(crate) fn rimebound_warblade_limited_spec() -> LimitedItemSpec {
    LimitedItemSpec {
        schema: RPG_LIMITED_ITEM_SPEC_SCHEMA.to_string(),
        limited_id: RIMEBOUND_WARBLADE_LIMITED_ID.to_string(),
        item_id: "item.limited.rimebound_warblade".to_string(),
        display_name: "the Rimebound Warblade".to_string(),
        world_cap: 5,
        may_enter_container: false,
        keep_level_floor: CF_LIMITED_KEEP_LEVEL_FLOOR,
        crumble: CrumbleRule {
            crumble_on_overspread: true,
            overspread_note: "Overspread warblades occasionally crumble (helpfile 61); the \
                 pressure valve against hoarding stockpiles."
                .to_string(),
        },
        illusion: IllusionRule {
            illusion_over_limit: true,
            illusion_note: "An over-limit claimant wields a stat-less illusion of the blade \
                 until the ledger frees a real instance."
                .to_string(),
        },
        hoarding_is_violation: true,
        source_issue: 1686,
        canon_issues: vec![973, 974, 975, 989, 1686],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?id=61".to_string(),
        runtime_enforcement_notes: vec![
            "No-container enforcement (rejecting a put into a bag) is a runtime command guard; \
             this record asserts the item's container-forbidden property."
                .to_string(),
            "Ledger count (#973), sub-L11 purge, crumble trigger, and illusion swap are \
             game-sim runtime scope."
                .to_string(),
        ],
        implementation_notes: vec![
            "Scarcity mechanics transcribed from CF helpfile 61; container-forbidden weapon is \
             always on the wielder's person and always lootable (#975)."
                .to_string(),
            "Name/lore originalized per #984; combat stats are a separate ItemSpec (PBF-sourced, \
             #1688) -- limited weapons are not in the public itemsearch database."
                .to_string(),
        ],
    }
}
