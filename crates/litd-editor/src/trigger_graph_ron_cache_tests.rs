use super::TriggerGraphRonBytesCache;
use crate::trigger_editor::{trigger_graph_stable_ron, unit_dies_create_unit_graph};

#[test]
fn bytes_memoized_per_generation_and_selection() {
    // #1461 item 3 FSV: the byte count is served from cache for a stable (generation, index) key
    // and recomputed only when that key changes. Real ECA graphs, no mocks; the source of truth
    // is trigger_graph_stable_ron's own length.
    let graph_a = unit_dies_create_unit_graph(true, true);
    let mut graph_b = unit_dies_create_unit_graph(true, false); // different content (no sleep)
    graph_b.trigger_id = "trg_issue1461_b".to_string();

    let bytes_a = trigger_graph_stable_ron(&graph_a).unwrap().len();
    let bytes_b = trigger_graph_stable_ron(&graph_b).unwrap().len();
    assert_ne!(
        bytes_a, bytes_b,
        "the two graphs must differ in size for a meaningful test"
    );

    let mut cache = TriggerGraphRonBytesCache::default();

    // First access at (gen 0, index 0) computes graph_a's size.
    assert_eq!(cache.bytes(0, 0, &graph_a).unwrap(), bytes_a);

    // Same (gen 0, index 0) key: pass graph_b, but the cache must NOT recompute — it returns the
    // cached graph_a size. This is the direct proof that no per-frame serialization happens.
    assert_eq!(
        cache.bytes(0, 0, &graph_b).unwrap(),
        bytes_a,
        "an unchanged (generation, index) key must not re-serialize"
    );

    // Changing the selected index recomputes for the graph actually passed.
    assert_eq!(cache.bytes(0, 1, &graph_b).unwrap(), bytes_b);

    // Bumping the generation recomputes (here back to graph_a).
    assert_eq!(cache.bytes(1, 0, &graph_a).unwrap(), bytes_a);
}
