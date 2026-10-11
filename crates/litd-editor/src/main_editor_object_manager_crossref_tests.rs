use std::collections::BTreeMap;

use litd_editor::mcp_trigger_tools::{CrossRefObjectKind, EcaGraph, EcaNode};

use super::*;

fn node(node_id: &str, op: &str, order: u32, args: &[(&str, &str)], refs: &[&str]) -> EcaNode {
    EcaNode {
        node_id: node_id.to_string(),
        operation: op.to_string(),
        order,
        args: args
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        variable_refs: refs.iter().map(|r| r.to_string()).collect(),
    }
}

/// A guard-death trigger that references a unit instance, a region, a unit-type
/// and a global variable -- one of each cross-referenceable kind the panel shows.
fn guard_graph(trigger_id: &str) -> EcaGraph {
    EcaGraph {
        trigger_id: trigger_id.to_string(),
        display_name: format!("{trigger_id} display"),
        enabled: true,
        folder_path: Some("Combat".to_string()),
        initially_on: true,
        comment: None,
        events: vec![node(
            "evt",
            "unit_dies",
            0,
            &[("unit_id", "unit.issue736.guard")],
            &[],
        )],
        conditions: vec![node(
            "cond",
            "game_flag_true",
            1,
            &[("flag", "gate_open")],
            &["gate_open"],
        )],
        actions: vec![node(
            "act",
            "create_unit_in_region",
            2,
            &[
                ("unit_type_id", "guard_type"),
                ("region_id", "region.spawn"),
            ],
            &[],
        )],
        wasm_escape: None,
    }
}

#[test]
fn trigger_entry_lists_referenced_objects() {
    let triggers = vec![guard_graph("trg_guard")];
    let answer = object_manager_cross_ref(&triggers, "trg_guard", "Trigger");
    match answer {
        ObjectManagerCrossRef::TriggerReferences {
            trigger_id,
            objects,
        } => {
            assert_eq!(trigger_id, "trg_guard");
            assert!(objects.contains(&TriggerObjectRef {
                object_kind: CrossRefObjectKind::Unit,
                object_id: "unit.issue736.guard".to_string(),
            }));
            assert!(objects.contains(&TriggerObjectRef {
                object_kind: CrossRefObjectKind::Region,
                object_id: "region.spawn".to_string(),
            }));
            assert!(objects.contains(&TriggerObjectRef {
                object_kind: CrossRefObjectKind::UnitType,
                object_id: "guard_type".to_string(),
            }));
            assert!(objects.contains(&TriggerObjectRef {
                object_kind: CrossRefObjectKind::Variable,
                object_id: "gate_open".to_string(),
            }));
        }
        other => panic!("expected TriggerReferences, got {other:?}"),
    }
}

#[test]
fn unit_object_entry_lists_referencing_triggers() {
    let triggers = vec![guard_graph("trg_guard"), guard_graph("trg_guard_b")];
    let answer = object_manager_cross_ref(&triggers, "unit.issue736.guard", "Unit");
    match answer {
        ObjectManagerCrossRef::ObjectReferencedBy {
            object_kind,
            object_id,
            trigger_ids,
        } => {
            assert_eq!(object_kind, CrossRefObjectKind::Unit);
            assert_eq!(object_id, "unit.issue736.guard");
            assert_eq!(trigger_ids, vec!["trg_guard", "trg_guard_b"]);
        }
        other => panic!("expected ObjectReferencedBy, got {other:?}"),
    }
}

#[test]
fn region_and_variable_entries_are_cross_referenced() {
    let triggers = vec![guard_graph("trg_guard")];

    let region = object_manager_cross_ref(&triggers, "region.spawn", "Region");
    match region {
        ObjectManagerCrossRef::ObjectReferencedBy { trigger_ids, .. } => {
            assert_eq!(trigger_ids, vec!["trg_guard"]);
        }
        other => panic!("expected ObjectReferencedBy for region, got {other:?}"),
    }

    let variable = object_manager_cross_ref(&triggers, "gate_open", "Variable");
    match variable {
        ObjectManagerCrossRef::ObjectReferencedBy {
            object_kind,
            trigger_ids,
            ..
        } => {
            assert_eq!(object_kind, CrossRefObjectKind::Variable);
            assert_eq!(trigger_ids, vec!["trg_guard"]);
        }
        other => panic!("expected ObjectReferencedBy for variable, got {other:?}"),
    }
}

#[test]
fn object_referenced_by_no_trigger_is_empty_not_error() {
    let triggers = vec![guard_graph("trg_guard")];
    let answer = object_manager_cross_ref(&triggers, "region.unused", "Region");
    match answer {
        ObjectManagerCrossRef::ObjectReferencedBy { trigger_ids, .. } => {
            assert!(trigger_ids.is_empty());
        }
        other => panic!("expected empty ObjectReferencedBy, got {other:?}"),
    }
}

#[test]
fn asset_kind_is_not_cross_referenceable() {
    let triggers = vec![guard_graph("trg_guard")];
    let answer = object_manager_cross_ref(&triggers, "asset.guard_model", "Model");
    match answer {
        ObjectManagerCrossRef::NotCrossReferenceable { kind } => assert_eq!(kind, "Model"),
        other => panic!("expected NotCrossReferenceable, got {other:?}"),
    }
}

#[test]
fn unknown_trigger_id_surfaces_a_typed_error() {
    let triggers = vec![guard_graph("trg_guard")];
    let answer = object_manager_cross_ref(&triggers, "trg_missing", "Trigger");
    match answer {
        ObjectManagerCrossRef::Error(reason) => assert!(!reason.is_empty()),
        other => panic!("expected Error, got {other:?}"),
    }
}

#[test]
fn editing_a_trigger_changes_the_cross_reference() {
    let mut triggers = vec![guard_graph("trg_guard")];
    let before = object_manager_cross_ref(&triggers, "region.moved", "Region");
    assert!(matches!(
        before,
        ObjectManagerCrossRef::ObjectReferencedBy { ref trigger_ids, .. } if trigger_ids.is_empty()
    ));

    // Recompute-from-state, not a cached index: point the action at region.moved
    // and the same query now matches the trigger (#1348).
    triggers[0].actions[0]
        .args
        .insert("region_id".to_string(), "region.moved".to_string());
    let after = object_manager_cross_ref(&triggers, "region.moved", "Region");
    match after {
        ObjectManagerCrossRef::ObjectReferencedBy { trigger_ids, .. } => {
            assert_eq!(trigger_ids, vec!["trg_guard"]);
        }
        other => panic!("expected ObjectReferencedBy after edit, got {other:?}"),
    }
}

#[test]
fn kind_mapping_covers_wc3_object_kinds() {
    assert_eq!(
        cross_ref_object_kind("Unit"),
        Some(CrossRefObjectKind::Unit)
    );
    assert_eq!(
        cross_ref_object_kind("unit_type"),
        Some(CrossRefObjectKind::UnitType)
    );
    assert_eq!(
        cross_ref_object_kind("Region"),
        Some(CrossRefObjectKind::Region)
    );
    assert_eq!(
        cross_ref_object_kind("variable"),
        Some(CrossRefObjectKind::Variable)
    );
    assert_eq!(
        cross_ref_object_kind("Player"),
        Some(CrossRefObjectKind::Player)
    );
    assert_eq!(
        cross_ref_object_kind("timer"),
        Some(CrossRefObjectKind::Timer)
    );
    assert_eq!(
        cross_ref_object_kind("unit_group"),
        Some(CrossRefObjectKind::UnitGroup)
    );
    assert_eq!(cross_ref_object_kind("Model"), None);
    assert_eq!(
        cross_ref_kind_label(CrossRefObjectKind::UnitType),
        "unit-type"
    );
}
