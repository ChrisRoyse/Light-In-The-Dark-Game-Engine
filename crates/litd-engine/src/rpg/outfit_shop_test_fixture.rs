//! Pilot fixtures for #1687: the CF newbie OUTFIT kit (both bands, helpfile 314)
//! and a CF-factual shopkeeper (haggle/fence/barter, helpfiles 501/1426/2262).
//! Names/lore originalized per #984; mechanics factual.

use crate::rpg::{
    BarterModel, FenceModel, HaggleModel, OutfitBand, OutfitGearPiece, OutfitKitSpec,
    OutfitMapGrant, OutfitProvision, RPG_OUTFIT_KIT_SCHEMA, RPG_SHOPKEEPER_SCHEMA, ShopHours,
    ShopStockEntry, ShopkeeperSpec,
};

/// The levels 1-10 outfit: soft leather gear + maps + provisions, ANYWHERE.
pub(crate) fn newbie_outfit_lowbie_spec() -> OutfitKitSpec {
    OutfitKitSpec {
        schema: RPG_OUTFIT_KIT_SCHEMA.to_string(),
        outfit_id: "outfit.ashen_reach.mayor_warehouse".to_string(),
        display_name: "the Warden's warehouse outfit".to_string(),
        band: OutfitBand::LowbieAnywhere,
        gear: vec![
            gear("light", "wood", "a simple torch"),
            gear("wield", "iron", "a plain shortsword"),
            gear("head", "leather", "a soft leather helmet"),
            gear("shield", "leather", "a soft leather shield"),
            gear("body", "leather", "a soft leather vest"),
        ],
        provisions: vec![
            provision("drink_container", "a basic waterskin"),
            provision("food", "a hunk of trail bread"),
        ],
        maps: vec![
            map("thera", "a map of Thera"),
            map("galadon", "a map of Galadon"),
            map("hometown", "a map of your hometown"),
        ],
        only_fills_empty_slots: true,
        breakage_prone: false,
        recovery_floor_issue: 976,
        source_issue: 1687,
        canon_issues: vec![822, 944, 976, 989, 1633, 1687],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?id=314".to_string(),
        implementation_notes: vec![
            "helpfile 314 OUTFIT GEAR: levels 1-10 outfit ANYWHERE -- soft leather gear (light, \
             weapon, helmet, shield, vest) from the Mayor's warehouses, maps of Thera/Galadon/\
             hometown, a drink container, and food. Only empty slots are filled."
                .to_string(),
            "Maps follow the #944 acquired-map doctrine (archive copies; no HUD/GPS). Light/weapon \
             materials are representative; the helpfile specifies soft leather for the armour set."
                .to_string(),
            "Full-loot recovery floor (#976): a player can always re-arm. Name originalized (#984)."
                .to_string(),
        ],
    }
}

/// The levels 11+ guild re-gear: shoddy, breakage-prone, guild-only, no maps.
pub(crate) fn newbie_outfit_guild_spec() -> OutfitKitSpec {
    OutfitKitSpec {
        schema: RPG_OUTFIT_KIT_SCHEMA.to_string(),
        outfit_id: "outfit.ashen_reach.guild_regear".to_string(),
        display_name: "the guild re-gear kit".to_string(),
        band: OutfitBand::GuildOnly,
        gear: vec![
            gear("light", "wood", "a guttering torch"),
            gear("wield", "iron", "a notched shortsword"),
            gear("head", "leather", "a shoddy leather cap"),
            gear("shield", "leather", "a shoddy leather buckler"),
            gear("body", "leather", "a shoddy leather jerkin"),
        ],
        provisions: vec![
            provision("drink_container", "a cracked waterskin"),
            provision("food", "a crust of stale bread"),
        ],
        maps: Vec::new(),
        only_fills_empty_slots: true,
        breakage_prone: true,
        recovery_floor_issue: 976,
        source_issue: 1687,
        canon_issues: vec![822, 976, 989, 1687],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?id=314".to_string(),
        implementation_notes: vec![
            "helpfile 314: from level 11, outfit only WITHIN YOUR OWN GUILD -- a set of simple \
             equipment and provisions. The gear is fairly shoddy and prone to eventual breakage."
                .to_string(),
            "No maps are re-granted at the guild band (helpfile 314 grants maps in the 1-10 \
             outfit only). Full-loot recovery floor (#976). Name originalized (#984)."
                .to_string(),
        ],
    }
}

/// A CF-factual city shopkeeper: haggle (helpfile 501), a thief fence contact
/// (helpfile 1426, city-only), factual barter ratios (helpfile 2262), a
/// cartographer stock (#1633), day hours (#848), and a finite wealth pool.
pub(crate) fn galdmere_merchant_spec() -> ShopkeeperSpec {
    ShopkeeperSpec {
        schema: RPG_SHOPKEEPER_SCHEMA.to_string(),
        shopkeeper_id: "shop.galdmere.emporium".to_string(),
        display_name: "the Galdmere emporium keeper".to_string(),
        city_area_id: "area.cf.galdmere".to_string(),
        is_city: true,
        // Sells to players at 200% of base, buys from players at 40% of base
        // (representative margins; CF publishes the haggle rule, not the constants).
        buy_markup_percent: 200,
        sell_payout_percent: 40,
        wealth_limit_coin: 50_000,
        stock: vec![
            ShopStockEntry {
                item_id: "item.cf.a_map_of_thera".to_string(),
                restock_count: 20,
                base_price_coin: 250,
                deals_in: true,
            },
            ShopStockEntry {
                item_id: "item.cf.a_plain_shortsword".to_string(),
                restock_count: 5,
                base_price_coin: 120,
                deals_in: true,
            },
            ShopStockEntry {
                item_id: "item.cf.a_healing_draught".to_string(),
                restock_count: 8,
                base_price_coin: 300,
                deals_in: false,
            },
        ],
        haggle: HaggleModel {
            governing_stat_id: "charisma".to_string(),
            thief_mastery: true,
            merchant_resists: true,
            max_swing_percent: 30,
            charisma_pivot: 13,
        },
        fence: Some(FenceModel {
            thief_only: true,
            cities_only: true,
            accepts_stolen: true,
            min_item_value_coin: 500,
        }),
        barter: BarterModel {
            food_per_item: 8,
            pills_per_item: 5,
            potions_per_item: 3,
            scrolls_per_item: 3,
            default_ratio_one_to_one: true,
            requires_sufficient_value: true,
            supports_practice_barter: true,
        },
        hours: ShopHours {
            opens_hour: 6,
            closes_hour: 21,
            always_open: false,
        },
        is_cartographer: true,
        commerce_reputation: true,
        source_issue: 1687,
        canon_issues: vec![822, 848, 1622, 1633, 1677, 1687],
        mechanics_source_url: "https://carrionfields.net/helpsearch.php?id=501".to_string(),
        implementation_notes: vec![
            "HAGGLE (helpfile 501): automatic CHARISMA skill; thieves are masters; merchants \
             resist. FENCE (helpfile 1426): thief-only, city-only, stolen goods of value."
                .to_string(),
            "BARTER (helpfile 2262): 1 item : 8 food / 5 pills / 3 potions / 3 scrolls, else 1:1; \
             practice/train/rehearse-for-item (#1677). COMMERCE (helpfile 2220): reputation/XP."
                .to_string(),
            "Coin margins/prices are representative model params (CF publishes the rules, not the \
             constants). Cartographer sells maps (#1633). Name originalized (#984)."
                .to_string(),
        ],
    }
}

fn gear(slot: &str, material: &str, label: &str) -> OutfitGearPiece {
    OutfitGearPiece {
        wear_slot_id: slot.to_string(),
        material_id: material.to_string(),
        label: label.to_string(),
    }
}

fn provision(kind: &str, label: &str) -> OutfitProvision {
    OutfitProvision {
        provision_kind: kind.to_string(),
        label: label.to_string(),
    }
}

fn map(region: &str, label: &str) -> OutfitMapGrant {
    OutfitMapGrant {
        map_region_id: region.to_string(),
        label: label.to_string(),
    }
}
