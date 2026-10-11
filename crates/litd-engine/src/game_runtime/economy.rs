//! Player resource accounts, supply/food, and food-based upkeep tiers (#837).
//!
//! Balances are unsigned integers keyed by resource id and stored in a `BTreeMap` so
//! iteration order (state-hash order) is deterministic regardless of insertion order. The
//! upkeep tier -- and therefore the income multiplier applied to harvested gold -- is a pure
//! function of current food usage against the authored tier table.

use std::collections::BTreeMap;

use litd_hash::Fnv1a64;

use crate::EngineError;

use super::lowered::{LoweredEconomyRuleset, LoweredUpkeepTier};

fn econ_error(field: &str, reason: impl Into<String>) -> EngineError {
    EngineError::GamePackageInvalidConfig {
        field: format!("economy.{field}"),
        reason: reason.into(),
    }
}

/// A player's resource accounts plus supply bookkeeping. All spending is checked; there is no
/// path that produces a negative balance or a free unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerAccounts {
    balances: BTreeMap<String, u64>,
    used_food: u32,
    supply_cap: u32,
}

impl PlayerAccounts {
    /// Seed accounts from the lowered ruleset's per-resource starting balances.
    pub fn from_ruleset(ruleset: &LoweredEconomyRuleset) -> Self {
        let mut balances = BTreeMap::new();
        for resource in &ruleset.resources {
            balances.insert(resource.key.clone(), resource.starting_balance);
        }
        Self {
            balances,
            used_food: 0,
            supply_cap: 0,
        }
    }

    pub fn balance(&self, key: &str) -> u64 {
        self.balances.get(key).copied().unwrap_or(0)
    }

    pub fn used_food(&self) -> u32 {
        self.used_food
    }

    pub fn supply_cap(&self) -> u32 {
        self.supply_cap
    }

    /// Grant resources (trigger API / harvest deposit). Unknown keys and overflow
    /// fail loud; clamping would make authored rewards diverge silently.
    pub fn grant(&mut self, key: &str, amount: u64) -> Result<u64, EngineError> {
        let next = self.checked_grant_balance(key, amount)?;
        let slot = self
            .balances
            .get_mut(key)
            .expect("checked grant requires existing resource");
        *slot = next;
        Ok(*slot)
    }

    /// Preflight a grant without mutation. Multi-resource transactions use this
    /// for every leg before committing the first balance.
    pub fn checked_grant_balance(&self, key: &str, amount: u64) -> Result<u64, EngineError> {
        let current = self
            .balances
            .get(key)
            .copied()
            .ok_or_else(|| econ_error("grant.key", format!("unknown resource {key}")))?;
        current.checked_add(amount).ok_or_else(|| {
            econ_error(
                "grant.overflow",
                format!("granting {amount} {key} would overflow balance {current}"),
            )
        })
    }

    /// Spend resources. Fails loud (typed) if the account cannot cover the cost -- never a
    /// silent free purchase.
    pub fn spend(&mut self, key: &str, amount: u64) -> Result<u64, EngineError> {
        let slot = self
            .balances
            .get_mut(key)
            .ok_or_else(|| econ_error("spend.key", format!("unknown resource {key}")))?;
        let next = slot.checked_sub(amount).ok_or_else(|| {
            econ_error(
                "spend.insufficient",
                format!("cannot spend {amount} {key}; have {slot}"),
            )
        })?;
        *slot = next;
        Ok(next)
    }

    pub fn can_afford(&self, key: &str, amount: u64) -> bool {
        self.balance(key) >= amount
    }

    /// Raise the supply cap when a structure/unit that provides food is completed.
    pub fn add_supply_cap(&mut self, provided: u32) -> Result<u32, EngineError> {
        let next = self.checked_supply_cap_grant(provided)?;
        self.commit_checked_supply_cap(next);
        Ok(self.supply_cap)
    }

    pub fn checked_supply_cap_grant(&self, provided: u32) -> Result<u32, EngineError> {
        self.supply_cap.checked_add(provided).ok_or_else(|| {
            econ_error(
                "supply_cap.overflow",
                format!(
                    "granting {provided} supply would overflow cap {}",
                    self.supply_cap
                ),
            )
        })
    }

    pub(super) fn commit_checked_supply_cap(&mut self, supply_cap: u32) {
        self.supply_cap = supply_cap;
    }

    /// Reserve food for a trained/queued unit. Fails loud at the supply cap so training blocks
    /// observably rather than overflowing the cap (the FSV supply-block edge case).
    pub fn reserve_food(&mut self, food: u32) -> Result<u32, EngineError> {
        let next = self.checked_food_reservation(food)?;
        self.commit_checked_food_reservation(next);
        Ok(self.used_food)
    }

    /// Preflight a food reservation without mutation so a multi-leg training
    /// transaction can validate supply before debiting either resource.
    pub fn checked_food_reservation(&self, food: u32) -> Result<u32, EngineError> {
        let next = self.used_food.checked_add(food).ok_or_else(|| {
            econ_error(
                "supply.overflow",
                format!(
                    "reserving {food} food would overflow used food {}",
                    self.used_food
                ),
            )
        })?;
        if next > self.supply_cap {
            return Err(econ_error(
                "supply.blocked",
                format!("food {next} exceeds supply cap {}", self.supply_cap),
            ));
        }
        Ok(next)
    }

    pub(super) fn commit_checked_food_reservation(&mut self, used_food: u32) {
        self.used_food = used_food;
    }

    pub fn would_exceed_supply(&self, food: u32) -> bool {
        self.checked_food_reservation(food).is_err()
    }

    /// Fold accounts into the deterministic state hash (BTreeMap order is stable).
    pub fn hash_into(&self, hasher: &mut Fnv1a64) {
        hasher.update_u64_le(self.balances.len() as u64);
        for (key, value) in &self.balances {
            hasher.update(key.as_bytes());
            hasher.update_u64_le(*value);
        }
        hasher.update_u64_le(u64::from(self.used_food));
        hasher.update_u64_le(u64::from(self.supply_cap));
    }
}

/// Resolve the upkeep tier for a given food usage. Returns the first tier whose
/// `[food_min, food_max]` contains `food`; if none match (food above every tier) the last
/// tier is used. Tiers are pre-sorted by `food_min` on lowering.
pub fn upkeep_tier_for(tiers: &[LoweredUpkeepTier], food: u32) -> Option<&LoweredUpkeepTier> {
    if tiers.is_empty() {
        return None;
    }
    for tier in tiers {
        if food >= tier.food_min && food <= tier.food_max {
            return Some(tier);
        }
    }
    tiers.last()
}

/// Apply an upkeep tier's income multiplier (numerator/denominator) to a raw income amount.
/// Integer math only -- floor division keeps sim state float-free (#85).
pub fn apply_upkeep_multiplier(
    raw_income: u64,
    tier: &LoweredUpkeepTier,
) -> Result<u64, EngineError> {
    if tier.income_denominator == 0 {
        return Err(econ_error(
            "upkeep.denominator",
            format!("tier {} has a zero income denominator", tier.name),
        ));
    }
    let scaled = u128::from(raw_income)
        .checked_mul(u128::from(tier.income_numerator))
        .ok_or_else(|| {
            econ_error(
                "upkeep.overflow",
                format!(
                    "income {raw_income} times numerator {} exceeds exact arithmetic range",
                    tier.income_numerator
                ),
            )
        })?;
    let credited = scaled / u128::from(tier.income_denominator);
    u64::try_from(credited).map_err(|_| {
        econ_error(
            "upkeep.overflow",
            format!(
                "income {raw_income} at ratio {}/{} exceeds {}",
                tier.income_numerator,
                tier.income_denominator,
                u64::MAX
            ),
        )
    })
}
