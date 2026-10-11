use litd_engine::{EngineError, math::Transform};

use super::*;
use crate::history::EditorObject;

fn tech(id: &str, tier: u32, requires: &str, equivalents: &str) -> EditorObject {
    let mut object = EditorObject::new(id, "Tech", None, Transform::IDENTITY)
        .with_property("object.rawcode", "T001")
        .with_property("object.display_name", id)
        .with_property("tech.tier", tier.to_string());
    if !requires.is_empty() {
        object = object.with_property("tech.requires", requires);
    }
    if !equivalents.is_empty() {
        object = object.with_property("tech.equivalents", equivalents);
    }
    object
}

fn records(objects: &[EditorObject]) -> Vec<TechRequirementRecord> {
    objects
        .iter()
        .map(|o| build_tech_record(o).unwrap())
        .collect()
}

#[test]
fn build_and_validate_acyclic_tree_counts_edges() {
    let objects = [
        tech("tech.town_hall", 1, "", ""),
        tech("tech.barracks", 1, "tech.town_hall", ""),
        tech("tech.footman", 2, "tech.barracks", ""),
    ];
    let recs = records(&objects);
    let edges = validate_tech_graph(&recs).unwrap();
    assert_eq!(edges, 2);
}

#[test]
fn dependency_equivalents_resolve_a_required_alias() {
    // keep and castle both satisfy the "req.townhall_tier" alias; barracks requires the alias.
    let objects = [
        tech("tech.keep", 2, "", "req.townhall_tier"),
        tech("tech.castle", 3, "", "req.townhall_tier"),
        tech("tech.barracks", 1, "req.townhall_tier", ""),
    ];
    let recs = records(&objects);
    // Two providers of the alias -> two edges from barracks.
    assert_eq!(validate_tech_graph(&recs).unwrap(), 2);
}

#[test]
fn dangling_requirement_fails_loud() {
    let objects = [tech("tech.footman", 2, "tech.missing_barracks", "")];
    let recs = records(&objects);
    match validate_tech_graph(&recs) {
        Err(EngineError::UiRenderInvalidConfig { field, reason }) => {
            assert_eq!(field, "editor.object_editor.tech.requires");
            assert!(reason.contains("tech.missing_barracks"));
        }
        other => panic!("expected dangling-ref rejection, got {other:?}"),
    }
}

#[test]
fn requirement_cycle_is_rejected_naming_the_cycle() {
    let objects = [
        tech("tech.a", 1, "tech.c", ""),
        tech("tech.b", 1, "tech.a", ""),
        tech("tech.c", 1, "tech.b", ""),
    ];
    let recs = records(&objects);
    match validate_tech_graph(&recs) {
        Err(EngineError::ObjectRequirementCycle { cycle }) => {
            // Every node in the cycle must be named, and it must close on itself.
            assert!(cycle.contains("tech.a"));
            assert!(cycle.contains("tech.b"));
            assert!(cycle.contains("tech.c"));
            assert!(cycle.contains(" -> "));
        }
        other => panic!("expected ObjectRequirementCycle, got {other:?}"),
    }
}

#[test]
fn self_requirement_via_equivalent_is_a_cycle() {
    let objects = [tech("tech.loop", 1, "req.self", "req.self")];
    let recs = records(&objects);
    assert!(matches!(
        validate_tech_graph(&recs),
        Err(EngineError::ObjectRequirementCycle { .. })
    ));
}

#[test]
fn tier_out_of_range_is_rejected_at_extraction() {
    let object =
        tech("tech.bad", 0, "", "").with_property("tech.tier", (TECH_TIER_MAX + 1).to_string());
    assert!(build_tech_record(&object).is_err());
}
