use super::*;

#[test]
fn live_unit_label_uses_real_owner_id_and_hp() {
    assert_eq!(unit_label(0, 17, 42_000, 75_000), "P1 Unit 17  42/75 HP");
    assert_eq!(
        unit_label(2, 4_096, 125_999, 200_000),
        "P3 Unit 4096  125/200 HP"
    );
}

#[test]
fn player_label_colors_are_opaque_and_distinct() {
    let first = player_color(0);
    let second = player_color(1);
    assert_eq!(first.a, 255);
    assert_eq!(second.a, 255);
    assert_ne!(first, second);
    assert_eq!(player_color(4), first);
}

#[test]
fn empty_snapshot_contains_no_fabricated_text() {
    let snapshot = RuntimeTextSnapshot::default();
    assert!(snapshot.labels.is_empty());
    assert!(snapshot.damage_numbers.is_empty());
    assert_eq!(snapshot.session_tick, 0);
}
