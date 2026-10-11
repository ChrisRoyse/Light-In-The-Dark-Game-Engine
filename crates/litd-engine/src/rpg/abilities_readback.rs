use std::collections::{BTreeMap, BTreeSet};

use super::serialize_sha256;

use crate::EngineError;

use super::abilities_types::*;

pub(super) fn build_ability_readback(
    catalog: &AbilityCatalog,
    max_prerequisite_depth: usize,
) -> Result<AbilityCatalogReadback, EngineError> {
    let mut totals = ReadbackTotals::default();
    for ability in &catalog.abilities {
        totals.add_ability(ability);
    }
    let input_sha256 = serialize_sha256(catalog)?;
    let cost_resource_totals = totals.cost_resource_totals();
    let ability_ids = totals.sorted_ability_ids();
    let mut readback = AbilityCatalogReadback {
        schema: RPG_ABILITY_SCHEMA.to_string(),
        catalog_id: catalog.catalog_id.clone(),
        ability_count: catalog.abilities.len(),
        kind_counts: totals.kind_counts,
        activation_counts: totals.activation_counts,
        target_mode_counts: totals.target_mode_counts,
        effect_kind_counts: totals.effect_kind_counts,
        cost_resource_totals,
        prerequisite_edge_count: totals.prerequisite_edge_count,
        max_prerequisite_depth,
        school_count: totals.schools.len(),
        branch_count: totals.branches.len(),
        capstone_count: totals.capstone_count,
        ui_slot_count: totals.ui_slot_count,
        trigger_tag_count: totals.trigger_tag_count,
        ability_ids,
        input_sha256,
        result_sha256: String::new(),
    };
    readback.result_sha256 = serialize_sha256(&result_fingerprint_payload(&readback))?;
    Ok(readback)
}

#[derive(Default)]
struct ReadbackTotals {
    kind_counts: BTreeMap<String, usize>,
    activation_counts: BTreeMap<String, usize>,
    target_mode_counts: BTreeMap<String, usize>,
    effect_kind_counts: BTreeMap<String, usize>,
    cost_totals: BTreeMap<String, i64>,
    schools: BTreeSet<String>,
    branches: BTreeSet<String>,
    prerequisite_edge_count: usize,
    capstone_count: usize,
    ui_slot_count: usize,
    trigger_tag_count: usize,
    ability_ids: Vec<String>,
}

impl ReadbackTotals {
    fn add_ability(&mut self, ability: &AbilityDefinition) {
        for kind in AbilityKind::ALL {
            self.kind_counts
                .entry(kind.label().to_string())
                .or_insert(0);
        }
        *self
            .kind_counts
            .entry(ability.kind.label().to_string())
            .or_insert(0) += 1;
        *self
            .activation_counts
            .entry(ability.activation.label().to_string())
            .or_insert(0) += 1;
        *self
            .target_mode_counts
            .entry(ability.targeting.mode.label().to_string())
            .or_insert(0) += 1;
        for effect in &ability.effects {
            *self
                .effect_kind_counts
                .entry(effect.kind.label().to_string())
                .or_insert(0) += 1;
        }
        for cost in &ability.costs {
            *self
                .cost_totals
                .entry(cost.resource_id.clone())
                .or_insert(0) += i64::from(cost.amount);
        }
        if let Some(school) = &ability.category.school_id {
            self.schools.insert(school.clone());
        }
        if let Some(branch) = &ability.category.branch_id {
            self.branches.insert(branch.clone());
        }
        self.prerequisite_edge_count += ability.requirements.prerequisites.len();
        self.capstone_count += usize::from(ability.category.capstone);
        self.ui_slot_count += usize::from(ability.ui_slot.is_some());
        self.trigger_tag_count += ability.trigger_tags.len();
        self.ability_ids.push(ability.ability_id.clone());
    }

    fn cost_resource_totals(&self) -> Vec<ResourceCostTotal> {
        self.cost_totals
            .iter()
            .map(|(resource_id, amount)| ResourceCostTotal {
                resource_id: resource_id.clone(),
                amount: *amount,
            })
            .collect()
    }

    fn sorted_ability_ids(&self) -> Vec<String> {
        let mut ids = self.ability_ids.clone();
        ids.sort();
        ids
    }
}

fn result_fingerprint_payload(readback: &AbilityCatalogReadback) -> serde_json::Value {
    serde_json::json!({
        "schema": readback.schema,
        "catalog_id": readback.catalog_id,
        "ability_count": readback.ability_count,
        "kind_counts": readback.kind_counts,
        "activation_counts": readback.activation_counts,
        "target_mode_counts": readback.target_mode_counts,
        "effect_kind_counts": readback.effect_kind_counts,
        "cost_resource_totals": readback.cost_resource_totals,
        "prerequisite_edge_count": readback.prerequisite_edge_count,
        "max_prerequisite_depth": readback.max_prerequisite_depth,
        "school_count": readback.school_count,
        "branch_count": readback.branch_count,
        "capstone_count": readback.capstone_count,
        "ui_slot_count": readback.ui_slot_count,
        "trigger_tag_count": readback.trigger_tag_count,
        "ability_ids": readback.ability_ids,
        "input_sha256": readback.input_sha256,
    })
}
