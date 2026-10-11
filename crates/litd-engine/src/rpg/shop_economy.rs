//! Validation + readback + the haggle price computation for the CF shopkeeper
//! economy (#1687).
//!
//! `validate_shopkeeper` is the loud-fail gate. `haggle_quote` is the deterministic
//! price model: CHARISMA above the model pivot swings the price, thieves are
//! masters, untrained low-CHA adventurers get no swing, merchants resist, and the
//! swing is capped (helpfile 501). It is the FSV surface for the shop loop.

use litd_hash::sha256_hex;

use crate::EngineError;

use super::shop_economy_types::*;

/// The Carrion Fields maximum attainable CHARISMA (stat cap with racial bonus).
/// Haggle swing scales from the model pivot up to this cap.
const CF_MAX_CHARISMA: u8 = 25;

pub fn shopkeeper_readback(spec: &ShopkeeperSpec) -> Result<ShopkeeperReadback, EngineError> {
    validate_shopkeeper(spec)?;
    Ok(ShopkeeperReadback {
        schema: spec.schema.clone(),
        shopkeeper_id: spec.shopkeeper_id.clone(),
        stock_count: spec.stock.len(),
        is_city: spec.is_city,
        has_fence: spec.fence.is_some(),
        is_cartographer: spec.is_cartographer,
        buy_markup_percent: spec.buy_markup_percent,
        sell_payout_percent: spec.sell_payout_percent,
        wealth_limit_coin: spec.wealth_limit_coin,
        shopkeeper_sha256: shop_hash(spec)?,
    })
}

pub fn validate_shopkeeper(spec: &ShopkeeperSpec) -> Result<(), EngineError> {
    if spec.schema != RPG_SHOPKEEPER_SCHEMA {
        return invalid("schema", format!("expected {RPG_SHOPKEEPER_SCHEMA}"));
    }
    validate_token("shopkeeper_id", &spec.shopkeeper_id)?;
    validate_text("display_name", &spec.display_name)?;
    validate_token("city_area_id", &spec.city_area_id)?;
    if spec.source_issue == 0 || !spec.canon_issues.contains(&spec.source_issue) {
        return invalid("canon_issues", "must include the non-zero source issue");
    }
    if !spec
        .mechanics_source_url
        .starts_with("https://carrionfields.net/")
    {
        return invalid("mechanics_source_url", "must be an approved CF source");
    }
    // The shop sells DEARER than base and buys CHEAPER than base (margin sign).
    if spec.buy_markup_percent < 100 {
        return invalid(
            "buy_markup_percent",
            "a shop sells to players at >= 100% of base",
        );
    }
    if spec.sell_payout_percent > 100 {
        return invalid(
            "sell_payout_percent",
            "a shop buys from players at <= 100% of base",
        );
    }
    // Haggle model sanity.
    if spec.haggle.governing_stat_id != "charisma" {
        return invalid(
            "haggle.governing_stat_id",
            "helpfile 501: haggling is governed by CHARISMA",
        );
    }
    if spec.haggle.max_swing_percent == 0 || spec.haggle.max_swing_percent > 90 {
        return invalid(
            "haggle.max_swing_percent",
            "must be a meaningful cap in 1..=90",
        );
    }
    if spec.haggle.charisma_pivot >= CF_MAX_CHARISMA {
        return invalid(
            "haggle.charisma_pivot",
            format!("must be below the CF CHARISMA cap ({CF_MAX_CHARISMA})"),
        );
    }
    // Fence: city-only (helpfile 1426).
    if let Some(fence) = &spec.fence {
        if fence.cities_only && !spec.is_city {
            return invalid(
                "fence",
                "helpfile 1426: fencing is city-only but this shop is not in a city",
            );
        }
        if !fence.thief_only {
            return invalid(
                "fence.thief_only",
                "helpfile 1426: fencing is a thief ability",
            );
        }
    }
    // Barter ratios must be the factual helpfile-2262 values.
    let b = &spec.barter;
    if (
        b.food_per_item,
        b.pills_per_item,
        b.potions_per_item,
        b.scrolls_per_item,
    ) != (8, 5, 3, 3)
    {
        return invalid(
            "barter",
            "helpfile 2262 ratios are 1 item : 8 food / 5 pills / 3 potions / 3 scrolls",
        );
    }
    if !b.default_ratio_one_to_one {
        return invalid(
            "barter.default_ratio_one_to_one",
            "helpfile 2262: all other barter is 1-for-1",
        );
    }
    // Hours.
    if !spec.hours.always_open && (spec.hours.opens_hour > 23 || spec.hours.closes_hour > 23) {
        return invalid("hours", "day/night hours are 0..=23 (#848)");
    }
    for entry in &spec.stock {
        validate_token("stock.item_id", &entry.item_id)?;
    }
    if spec.implementation_notes.is_empty() {
        return invalid("implementation_notes", "must include source notes");
    }
    Ok(())
}

/// Compute a haggled price quote (helpfile 501). Deterministic:
///
/// - CHARISMA at or below the model pivot yields NO swing.
/// - Swing scales linearly from the pivot to the CF CHARISMA cap, reaching the
///   model's `max_swing_percent` at the cap.
/// - A thief (natural master) realises the full swing; a non-thief who has
///   learned the skill realises half (merchants resist amateurs harder); an
///   untrained non-thief realises none ("guard his treasure closely").
/// - Buying drives the price DOWN, selling drives the payout UP.
pub fn haggle_quote(
    spec: &ShopkeeperSpec,
    base_price_coin: u32,
    side: HaggleSide,
    charisma: u8,
    has_haggle_skill: bool,
    is_thief: bool,
) -> HaggleQuote {
    let markup = match side {
        HaggleSide::PlayerBuys => spec.buy_markup_percent,
        HaggleSide::PlayerSells => spec.sell_payout_percent,
    };
    let unhaggled = ((base_price_coin as u64) * (markup as u64) / 100) as u32;

    let swing_percent = swing_percent(&spec.haggle, charisma, has_haggle_skill, is_thief);
    let delta = ((unhaggled as u64) * (swing_percent as u64) / 100) as u32;
    let quoted = match side {
        HaggleSide::PlayerBuys => unhaggled.saturating_sub(delta),
        HaggleSide::PlayerSells => unhaggled.saturating_add(delta),
    };
    HaggleQuote {
        side,
        base_price_coin,
        quoted_price_coin: quoted,
        swing_coin: quoted as i64 - unhaggled as i64,
        swing_percent,
    }
}

fn swing_percent(model: &HaggleModel, charisma: u8, has_skill: bool, is_thief: bool) -> u16 {
    if charisma <= model.charisma_pivot {
        return 0;
    }
    if !has_skill && !is_thief {
        return 0;
    }
    let span = (CF_MAX_CHARISMA - model.charisma_pivot) as u32;
    let above = (charisma.min(CF_MAX_CHARISMA) - model.charisma_pivot) as u32;
    let full = (model.max_swing_percent as u32) * above / span;
    let realised = if is_thief { full } else { full / 2 };
    realised.min(model.max_swing_percent as u32) as u16
}

fn validate_token(field: &str, value: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 96 {
        return invalid(field, "must be non-empty and <= 96 bytes");
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return invalid(
            field,
            "must use ASCII letters, digits, dot, underscore, or dash",
        );
    }
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<(), EngineError> {
    if value.trim().is_empty() {
        return invalid(field, "must be non-empty");
    }
    Ok(())
}

fn shop_hash(spec: &ShopkeeperSpec) -> Result<String, EngineError> {
    serde_json::to_vec(spec)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| EngineError::SerializationFailed {
            format: "json",
            reason: format!("serialize shopkeeper: {error}"),
        })
}

fn invalid<T>(field: impl Into<String>, reason: impl std::fmt::Display) -> Result<T, EngineError> {
    Err(EngineError::RpgStatsInvalidConfig {
        field: field.into(),
        reason: reason.to_string(),
    })
}
