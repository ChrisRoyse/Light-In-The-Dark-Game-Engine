use std::time::Instant;

use litd_engine::math::Transform;

use super::{ObjectManagerIndexCache, ObjectManagerProject, build_object_manager_index};
use crate::history::EditorObject;

/// Real Object Manager project grown to `object_count` synthetic units — no mocks; the
/// index is built over genuine EditorDocument objects exactly as the panel would.
fn large_project(object_count: usize) -> ObjectManagerProject {
    let mut project = super::issue736_sample_project().unwrap();
    for i in 0..object_count {
        project
            .document
            .upsert_object(EditorObject::new(
                format!("unit.synthetic.{i}"),
                "Unit",
                None,
                Transform::IDENTITY,
            ))
            .unwrap();
    }
    project
}

#[test]
fn cache_rebuilds_only_when_generation_advances() {
    // #1461 FSV: the memoized index must NOT rebuild while the generation is unchanged (a
    // mutation is invisible) and MUST rebuild when the generation advances. Source of truth:
    // the entry count of the index the cache returns.
    let mut project = large_project(1_000);
    let mut cache = ObjectManagerIndexCache::default();

    // First access at generation 0 builds and caches.
    let entries_gen0 = cache.get_or_build(0, &project).unwrap().entries.len();
    assert_eq!(cache.cached_generation(), Some(0));
    assert!(
        entries_gen0 >= 1_000,
        "expected >=1000 entries in a 1k-object project, got {entries_gen0}"
    );

    // Mutate the project but DO NOT advance the generation. The cache must return the SAME
    // index — the new object is invisible — which is the direct proof that no rebuild ran.
    project
        .document
        .upsert_object(EditorObject::new(
            "unit.synthetic.extra",
            "Unit",
            None,
            Transform::IDENTITY,
        ))
        .unwrap();
    let entries_stale_gen = cache.get_or_build(0, &project).unwrap().entries.len();
    assert_eq!(
        entries_stale_gen, entries_gen0,
        "an unchanged generation must not rebuild: the mutation must stay invisible"
    );
    assert_eq!(cache.cached_generation(), Some(0));

    // Advance the generation: the cache rebuilds and the extra object now appears.
    let entries_next_gen = cache.get_or_build(1, &project).unwrap().entries.len();
    assert_eq!(cache.cached_generation(), Some(1));
    assert_eq!(
        entries_next_gen,
        entries_gen0 + 1,
        "an advanced generation must rebuild and reflect the mutation"
    );
    println!(
        "issue1461 object-manager cache: entries gen0={entries_gen0} stale_gen0={entries_stale_gen} gen1={entries_next_gen}"
    );
}

#[test]
fn cached_index_matches_a_fresh_build() {
    // The memoized index must be byte-for-byte the index a direct rebuild produces — caching
    // must never change WHAT is shown, only how often it is computed.
    let project = large_project(500);
    let mut cache = ObjectManagerIndexCache::default();
    let fresh = build_object_manager_index(&project).unwrap();
    let cached = cache.get_or_build(0, &project).unwrap();
    assert_eq!(
        *cached, fresh,
        "cached index must equal a fresh build for the same project"
    );
}

#[test]
fn cache_reuse_is_cheaper_than_rebuilding_every_frame() {
    // #1461 accept-gate evidence: emulate 300 immediate-mode frames on a synthetic large
    // project (1k objects, 500 assets-worth of ids) and compare the per-frame cost of the
    // OLD path (unconditional build) with the NEW path (cache reuse at a stable generation).
    // The cache reuse does O(1) work per frame, so it is dramatically cheaper; assert the
    // strict inequality (robust — the old path rebuilds three BTreeSets + walks refs + RON-
    // serializes the whole project every frame).
    let project = large_project(1_000);
    const FRAMES: usize = 300;

    let uncached_start = Instant::now();
    let mut last_uncached = 0usize;
    for _ in 0..FRAMES {
        last_uncached = build_object_manager_index(&project).unwrap().entries.len();
    }
    let uncached_elapsed = uncached_start.elapsed();

    let mut cache = ObjectManagerIndexCache::default();
    // Prime once (the real first-frame build), then measure steady-state reuse.
    cache.get_or_build(0, &project).unwrap();
    let cached_start = Instant::now();
    let mut last_cached = 0usize;
    for _ in 0..FRAMES {
        last_cached = cache.get_or_build(0, &project).unwrap().entries.len();
    }
    let cached_elapsed = cached_start.elapsed();

    assert_eq!(last_cached, last_uncached, "same index either way");
    assert!(
        cached_elapsed < uncached_elapsed,
        "cached reuse ({cached_elapsed:?}) must be cheaper than rebuilding every frame ({uncached_elapsed:?})"
    );
    println!(
        "issue1461 object-manager {FRAMES} frames: uncached={uncached_elapsed:?} cached={cached_elapsed:?} speedup={:.1}x",
        uncached_elapsed.as_secs_f64() / cached_elapsed.as_secs_f64().max(f64::MIN_POSITIVE)
    );
}
