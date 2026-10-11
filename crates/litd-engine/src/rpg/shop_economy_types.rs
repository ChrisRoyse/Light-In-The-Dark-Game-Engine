//! The Carrion Fields shopkeeper economy data model (#1687).
//!
//! CF-factual, transcribed from the helpfiles (mechanics = fact; the numeric
//! tuning coefficients are labelled model parameters, since CF publishes the
//! RULES of haggling/fencing, not the constants):
//!
//! - HAGGLE (helpfile 501): an automatic CHARISMA-governed skill to get a better
//!   price buying or selling. Thieves are natural masters; other classes may
//!   learn it; most merchants are already skilled hagglers (they resist).
//! - FENCE (helpfile 1426): thieves well-connected in cities can fence (sell,
//!   often stolen) goods of considerable value. Fencing happens ONLY in cities.
//! - BARTER (helpfile 2262): trade item-for-item without coin. 1 item barters
//!   for up to 8 food, 5 pills, 3 potions, or 3 scrolls; all other transactions
//!   are 1-for-1. Shopkeepers barter favorably for goods they deal in and refuse
//!   items of insufficient value. `practice/train/rehearse <x> for <item>` barters
//!   for guild advancement (#1677).
//! - COMMERCE (helpfile 2220): profitable trade earns commerce XP and merchant
//!   reputation, and can grant minor skills beyond one's guild.
//! - Shop hours follow the day/night clock (#848); a shopkeeper has a finite
//!   wealth pool and cannot buy beyond it.
//!
//! Names/lore are originalized per #984; the mechanics are factual.

use serde::{Deserialize, Serialize};

pub const RPG_SHOPKEEPER_SCHEMA: &str = "litd.rpg.shopkeeper.v1";

/// Which side of a transaction a price quote is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HaggleSide {
    /// The player is BUYING from the shop (haggle drives the price DOWN).
    PlayerBuys,
    /// The player is SELLING to the shop (haggle drives the payout UP).
    PlayerSells,
}

/// The HAGGLE model (helpfile 501). The direction/mastery/resistance rules are
/// factual; `max_swing_percent` and `charisma_pivot` are labelled model tuning
/// parameters (CF does not publish the constants).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HaggleModel {
    /// The governing stat -- always CHARISMA per helpfile 501 ("See also CHARISMA").
    pub governing_stat_id: String,
    /// Thieves are natural masters of haggling (helpfile 501).
    pub thief_mastery: bool,
    /// Merchants are already skilled hagglers and resist (helpfile 501).
    pub merchant_resists: bool,
    /// The largest price swing haggling can ever produce, in percent (model cap).
    pub max_swing_percent: u16,
    /// The CHARISMA value at which haggling starts to move the price (model
    /// pivot); below it an untrained low-CHA adventurer gets no swing.
    pub charisma_pivot: u8,
}

/// The FENCE model (helpfile 1426).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FenceModel {
    /// Fencing is a thief ability (helpfile 1426 / thief path #1622).
    pub thief_only: bool,
    /// Fencing takes place EXCLUSIVELY in cities (helpfile 1426).
    pub cities_only: bool,
    /// Fenced goods are often stolen.
    pub accepts_stolen: bool,
    /// Fencing is for items of considerable value; a floor gates trivial goods.
    pub min_item_value_coin: u32,
}

/// The BARTER model (helpfile 2262). The item-for-item ratios are FACTUAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BarterModel {
    /// 1 item barters for up to this many food (helpfile 2262: 8).
    pub food_per_item: u8,
    /// ...this many pills (helpfile 2262: 5).
    pub pills_per_item: u8,
    /// ...this many potions (helpfile 2262: 3).
    pub potions_per_item: u8,
    /// ...this many scrolls (helpfile 2262: 3).
    pub scrolls_per_item: u8,
    /// All other barter transactions are 1-item-for-1-item (helpfile 2262).
    pub default_ratio_one_to_one: bool,
    /// Shopkeepers refuse items of insufficient value (helpfile 2262).
    pub requires_sufficient_value: bool,
    /// `practice/train/rehearse <x> for <item>` barters for guild advancement
    /// (#1677 / helpfile 2262).
    pub supports_practice_barter: bool,
}

/// Shop operating hours on the CF day/night clock (#848), hours in `0..=23`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopHours {
    pub opens_hour: u8,
    pub closes_hour: u8,
    /// A shop that never closes ignores `opens_hour`/`closes_hour`.
    pub always_open: bool,
}

/// One stocked line. `base_price_coin` is representative (CF does not publish
/// per-shop coin prices); `deals_in` marks the shop's specialty for barter
/// favorability (helpfile 2262).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopStockEntry {
    pub item_id: String,
    pub restock_count: u16,
    pub base_price_coin: u32,
    pub deals_in: bool,
}

/// A Carrion Fields shopkeeper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopkeeperSpec {
    pub schema: String,
    pub shopkeeper_id: String,
    pub display_name: String,
    /// The city/area this keeper trades in (fencing is city-only, helpfile 1426).
    pub city_area_id: String,
    pub is_city: bool,
    /// Markup the shop charges when SELLING to the player, in percent of base
    /// (>= 100). Haggle reduces the effective markup.
    pub buy_markup_percent: u16,
    /// Fraction of base the shop PAYS when buying from the player, in percent
    /// (<= 100). Haggle raises the effective payout.
    pub sell_payout_percent: u16,
    /// The keeper's finite coin pool; it cannot buy beyond this (shop wealth cap).
    pub wealth_limit_coin: u32,
    pub stock: Vec<ShopStockEntry>,
    pub haggle: HaggleModel,
    /// Present when this keeper is a thief fence (helpfile 1426).
    pub fence: Option<FenceModel>,
    pub barter: BarterModel,
    pub hours: ShopHours,
    /// A cartographer sells area maps (#1633).
    pub is_cartographer: bool,
    /// Successful trade builds merchant reputation / commerce XP (helpfile 2220).
    pub commerce_reputation: bool,
    pub source_issue: u32,
    pub canon_issues: Vec<u32>,
    pub mechanics_source_url: String,
    pub implementation_notes: Vec<String>,
}

/// A single haggled price quote (the FSV surface for the shop loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HaggleQuote {
    pub side: HaggleSide,
    pub base_price_coin: u32,
    pub quoted_price_coin: u32,
    /// The signed swing applied vs the un-haggled price, in coin.
    pub swing_coin: i64,
    /// The swing percent that produced the quote (after cap + merchant resist).
    pub swing_percent: u16,
}

/// The FSV readback surface for a shopkeeper: counts + capability flags + hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShopkeeperReadback {
    pub schema: String,
    pub shopkeeper_id: String,
    pub stock_count: usize,
    pub is_city: bool,
    pub has_fence: bool,
    pub is_cartographer: bool,
    pub buy_markup_percent: u16,
    pub sell_payout_percent: u16,
    pub wealth_limit_coin: u32,
    pub shopkeeper_sha256: String,
}
