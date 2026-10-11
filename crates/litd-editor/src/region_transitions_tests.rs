use super::*;
use crate::history::{EditorDocument, EditorRegion};
use litd_engine::math::Vec3;

fn doc_with_regions(regions: Vec<EditorRegion>) -> EditorDocument {
    let mut document =
        EditorDocument::new("calyx://world/region-test", "region-test-rev-1").unwrap();
    for region in regions {
        document.upsert_region(region).unwrap();
    }
    document
}

fn region(id: &str, center: Vec3, half: Vec3) -> EditorRegion {
    EditorRegion::new(id, format!("Name {id}"), center, half)
}

#[test]
fn enter_single_region_emits_one_enter() {
    let document = doc_with_regions(vec![region(
        "region.a",
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(1.0, 4.0, 1.0),
    )]);
    let report =
        evaluate_region_transitions(&document, Vec3::new(5.0, 0.0, 5.0), Vec3::ZERO).unwrap();
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].region_id, "region.a");
    assert_eq!(report.events[0].kind, RegionTransitionKind::Enter);
    assert_eq!(report.next_region_ids, vec!["region.a".to_string()]);
    assert!(report.previous_region_ids.is_empty());
}

#[test]
fn leave_single_region_emits_one_leave() {
    let document = doc_with_regions(vec![region(
        "region.a",
        Vec3::ZERO,
        Vec3::new(1.0, 4.0, 1.0),
    )]);
    let report =
        evaluate_region_transitions(&document, Vec3::ZERO, Vec3::new(5.0, 0.0, 5.0)).unwrap();
    assert_eq!(report.events.len(), 1);
    assert_eq!(report.events[0].kind, RegionTransitionKind::Leave);
    assert_eq!(report.events[0].region_id, "region.a");
}

#[test]
fn overlapping_regions_both_fire_deterministically() {
    // Two regions overlapping at the origin; moving from outside both to the shared
    // overlap must emit two ENTER events, sorted by region_id.
    let document = doc_with_regions(vec![
        region(
            "region.b",
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(1.0, 4.0, 1.0),
        ),
        region(
            "region.a",
            Vec3::new(-0.5, 0.0, 0.0),
            Vec3::new(1.0, 4.0, 1.0),
        ),
    ]);
    let report =
        evaluate_region_transitions(&document, Vec3::new(9.0, 0.0, 9.0), Vec3::ZERO).unwrap();
    assert_eq!(report.events.len(), 2);
    assert_eq!(report.events[0].region_id, "region.a");
    assert_eq!(report.events[1].region_id, "region.b");
    assert!(
        report
            .events
            .iter()
            .all(|event| event.kind == RegionTransitionKind::Enter)
    );
    // Byte-identical on a second run (determinism).
    let again =
        evaluate_region_transitions(&document, Vec3::new(9.0, 0.0, 9.0), Vec3::ZERO).unwrap();
    assert_eq!(report, again);
}

#[test]
fn leaves_ordered_before_enters() {
    // Move from region.a into region.b: one leave then one enter, leave first.
    let document = doc_with_regions(vec![
        region(
            "region.a",
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 4.0, 1.0),
        ),
        region(
            "region.b",
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(1.0, 4.0, 1.0),
        ),
    ]);
    let report =
        evaluate_region_transitions(&document, Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0)).unwrap();
    assert_eq!(report.events.len(), 2);
    assert_eq!(report.events[0].kind, RegionTransitionKind::Leave);
    assert_eq!(report.events[0].region_id, "region.a");
    assert_eq!(report.events[1].kind, RegionTransitionKind::Enter);
    assert_eq!(report.events[1].region_id, "region.b");
}

#[test]
fn staying_inside_emits_no_events() {
    let document = doc_with_regions(vec![region(
        "region.a",
        Vec3::ZERO,
        Vec3::new(4.0, 4.0, 4.0),
    )]);
    let report = evaluate_region_transitions(
        &document,
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, -1.0),
    )
    .unwrap();
    assert!(report.events.is_empty());
    assert_eq!(report.previous_region_ids, report.next_region_ids);
}

#[test]
fn boundary_point_is_inside() {
    let region = region("region.a", Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
    assert!(point_in_region(&region, Vec3::new(1.0, 0.0, 0.0)));
    assert!(!point_in_region(&region, Vec3::new(1.0001, 0.0, 0.0)));
}

#[test]
fn non_finite_endpoint_fails_loud() {
    let document = doc_with_regions(vec![region(
        "region.a",
        Vec3::ZERO,
        Vec3::new(1.0, 1.0, 1.0),
    )]);
    let error = evaluate_region_transitions(&document, Vec3::new(f32::NAN, 0.0, 0.0), Vec3::ZERO)
        .unwrap_err();
    match error {
        EngineError::UiRenderInvalidConfig { field, .. } => {
            assert_eq!(field, "editor.region_transitions.endpoints");
        }
        other => panic!("expected invalid config, got {other:?}"),
    }
}
