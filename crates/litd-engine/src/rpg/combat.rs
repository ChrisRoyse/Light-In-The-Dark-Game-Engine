use crate::EngineError;

use super::{
    CombatInput, CombatReadback, CombatRules,
    combat_readback::build_combat_readback,
    combat_resolve::resolve_combat,
    combat_validate::{validate_combat_input, validate_combat_rules},
};

pub fn resolve_combat_plan(
    input: &CombatInput,
    rules: &CombatRules,
) -> Result<CombatReadback, EngineError> {
    validate_combat_rules(rules)?;
    validate_combat_input(input, rules)?;
    let resolution = resolve_combat(input, rules)?;
    build_combat_readback(input, rules, resolution)
}
