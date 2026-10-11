// Full-corpus referential-integrity suite for the standalone area registry
// (#1704). Aggregates EVERY authored AreaSpec across all Thera batches -- the hub
// city (#1636 Galadon), the #1637-1643 cities, the #1644-1647 towns, the
// #1649-1660 wilderness/regions, and the #1657-1659 dungeons -- into one corpus,
// derives the registry from it (single source of truth), and proves every
// cross-area connection resolves to an authored area. Any future dangling edge
// fails this suite.

use crate::rpg::{
    AreaRegistry, AreaSpec, canonical_area_local_id, collect_dangling_area_refs,
    known_deferred_area_anchors, validate_area_corpus_referential_integrity,
    validate_area_corpus_referential_integrity_with_deferred, validate_area_spec,
};

/// The complete authored area corpus, assembled from every batch aggregator.
/// `pub(crate)` so the #1632 topology and #1633 road-network suites validate
/// their area references against the SAME single-source corpus (one registry
/// definition, no parallel id list).
pub(crate) fn full_authored_corpus() -> Vec<AreaSpec> {
    let mut specs = Vec::new();
    specs.push(crate::rpg_area_specs_test_fixture::galadon_area_spec());
    specs.extend(crate::rpg_area_specs_galadon_satellites::galadon_satellite_area_specs());
    specs.extend(crate::rpg_cities_thera_batch_1637_1643::all_city_area_specs());
    specs.extend(crate::rpg_towns_thera_batch_1644_1647::all_town_area_specs());
    specs.extend(crate::rpg_regions_thera_batch_1649_1660::all_region_area_specs());
    specs.extend(crate::rpg_dungeons_thera_batch_1657_1659::all_dungeon_area_specs());
    specs
}

#[test]
fn issue1704_full_corpus_referential_integrity_holds() {
    let corpus = full_authored_corpus();
    let deferred = known_deferred_area_anchors();

    // Diagnostic readout (printed under --nocapture): every external edge that
    // does NOT resolve to an authored area, so any unexpected new dangler is
    // visible in the log even before the assertion partitions it.
    let dangling = collect_dangling_area_refs(&corpus).expect("registry builds");
    for d in &dangling {
        let tag = if deferred.contains(&d.to_area_id.as_str()) {
            "DEFERRED"
        } else {
            "UNEXPECTED-DANGLING"
        };
        eprintln!(
            "{tag}: {} -[{}]-> {}",
            d.from_area_id, d.connection_id, d.to_area_id
        );
    }
    if std::env::var("DUMP_AUTHORED_IDS").is_ok() {
        let registry = AreaRegistry::from_specs(&corpus).expect("registry builds");
        for id in registry.ids() {
            eprintln!("AUTHORED_ID {id}");
        }
    }

    // Fails loud on any dangling edge that is neither authored nor a known-
    // deferred anchor, and on any stale ledger entry (see the registry doc).
    let readback = validate_area_corpus_referential_integrity_with_deferred(&corpus, deferred)
        .unwrap_or_else(|e| {
            panic!("full corpus failed referential integrity: {e}");
        });

    eprintln!(
        "issue1704 corpus: areas={} connections={} internal={} external={} resolved_external={} \
         deferred_external={} (deferred_ledger={}) variant_links={}",
        readback.area_count,
        readback.connection_count,
        readback.internal_connection_count,
        readback.external_connection_count,
        readback.resolved_external_connection_count,
        readback.deferred_external_connection_count,
        deferred.len(),
        readback.variant_link_count,
    );

    // #1725: the two authored Past<->present-day mirror pairs (Maethien,
    // Delar-Tol) each contribute a resolved region-variant link. This proves the
    // registry actually resolved the structural links, not merely that no
    // dangler tripped.
    assert!(
        readback.variant_link_count >= 2,
        "expected the two authored Past mirror pairs to resolve as variant links, got {}",
        readback.variant_link_count
    );

    // Every external edge is accounted for: it either resolves to an authored
    // area or is a tracked known-deferred anchor. Nothing falls through.
    assert_eq!(
        readback.resolved_external_connection_count + readback.deferred_external_connection_count,
        readback.external_connection_count,
        "every external connection must resolve or be a tracked deferred anchor",
    );
    // The overwhelming majority of the world graph must be really-authored, not
    // deferred -- guards against the ledger silently swallowing the corpus.
    assert!(
        readback.resolved_external_connection_count > readback.deferred_external_connection_count,
        "resolved edges ({}) must dominate deferred edges ({})",
        readback.resolved_external_connection_count,
        readback.deferred_external_connection_count,
    );
    // Sanity floors so an empty/degenerate corpus can never green this suite.
    assert!(
        readback.area_count >= 100,
        "expected the full Thera corpus (>=100 areas), got {}",
        readback.area_count
    );
    assert!(
        readback.external_connection_count >= 50,
        "expected a richly connected world graph, got {} external edges",
        readback.external_connection_count
    );
}

#[test]
fn issue1704_empty_corpus_is_vacuously_clean() {
    // Edge: empty input. No specs -> no edges -> trivially clean, zero counts.
    let readback = validate_area_corpus_referential_integrity(&[]).expect("empty is clean");
    assert_eq!(readback.area_count, 0);
    assert_eq!(readback.connection_count, 0);
    assert_eq!(readback.external_connection_count, 0);
}

#[test]
fn issue1704_injected_dangling_reference_fails_loud() {
    // Edge: inject one typo'd cross-area target into an otherwise clean corpus and
    // prove the registry fails closed with the exact structured diagnostic.
    let mut corpus = full_authored_corpus();
    let victim = corpus
        .iter_mut()
        .find(|s| !s.connections.is_empty())
        .expect("corpus has a spec with connections");
    let victim_area = victim.area_id.clone();
    victim.connections[0].to_area_id = "area.thera.__typo_nonexistent__".to_string();
    let bad_connection_id = victim.connections[0].connection_id.clone();

    // Even with the known-deferred ledger applied, an injected typo is NOT in the
    // ledger, so it is the sole UNEXPECTED dangler and must fail loud by name.
    let error = validate_area_corpus_referential_integrity_with_deferred(
        &corpus,
        known_deferred_area_anchors(),
    )
    .expect_err("must fail on a dangler");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("dangling cross-area connection"),
        "must fail as a dangling-connection error, not a stale-ledger error: {message}"
    );
    assert!(
        message.contains("area.thera.__typo_nonexistent__"),
        "diagnostic must name the dangling target, got: {message}"
    );
    assert!(
        message.contains(&bad_connection_id),
        "diagnostic must name the offending connection {bad_connection_id}, got: {message}"
    );
    assert!(
        message.contains(&victim_area),
        "diagnostic must name the source area {victim_area}, got: {message}"
    );
}

#[test]
fn issue1725_dangling_variant_reference_fails_loud() {
    // Edge (#1725): point a real area's region-variant link at an unauthored base
    // and prove the registry fails closed with the named variant diagnostic --
    // distinct from the connection-dangler path.
    let mut corpus = full_authored_corpus();
    let victim = corpus
        .iter_mut()
        .find(|s| s.variant_of_area_id.is_none())
        .expect("corpus has a base (non-variant) area");
    let victim_area = victim.area_id.clone();
    victim.variant_of_area_id = Some("area.thera.__missing_base__".to_string());
    victim.variant_label = Some("past".to_string());

    let error = validate_area_corpus_referential_integrity_with_deferred(
        &corpus,
        known_deferred_area_anchors(),
    )
    .expect_err("must fail on a dangling variant base");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    let message = error.to_string();
    assert!(
        message.contains("dangling area variant reference"),
        "must fail as a variant-reference error: {message}"
    );
    assert!(
        message.contains("area.thera.__missing_base__"),
        "diagnostic must name the missing base area, got: {message}"
    );
    assert!(
        message.contains(&victim_area),
        "diagnostic must name the offending variant area {victim_area}, got: {message}"
    );
    assert!(
        message.contains("variant:past"),
        "diagnostic must name the variant label, got: {message}"
    );
}

#[test]
fn issue1704_duplicate_area_id_fails_loud() {
    // Edge: two specs claiming the same area_id must be rejected (an ambiguous
    // registry would mask a real collision).
    let mut corpus = full_authored_corpus();
    let clone = corpus[0].clone();
    corpus.push(clone);
    let error = validate_area_corpus_referential_integrity(&corpus)
        .expect_err("duplicate area id must fail");
    assert_eq!(error.code(), "LITD-RPG-STATS-INVALID-CONFIG");
    assert!(error.to_string().contains("duplicate area id"));
}

#[test]
fn issue1749_every_authored_area_id_is_canonically_normalized() {
    // #1749: every authored area_id must already be in the ONE canonical id form
    // (`area.thera.<seg>[.<seg>...]`, each segment == its own normalization). This
    // is the corpus backstop behind the `canonical_area_local_id` helper: a future
    // batch that authors a non-normalized id (apostrophe dropped, stray hyphen,
    // uppercase, double underscore) fails loud here by name.
    for spec in full_authored_corpus() {
        let id = &spec.area_id;
        let segments: Vec<&str> = id.split('.').collect();
        assert!(
            segments.len() >= 3 && segments[0] == "area" && segments[1] == "thera",
            "area_id {id} must be namespaced under area.thera.*"
        );
        for seg in &segments {
            assert!(!seg.is_empty(), "area_id {id} has an empty id segment");
            assert_eq!(
                &canonical_area_local_id(seg),
                seg,
                "area_id {id} segment '{seg}' is not in canonical normalized form \
                 (expected '{}')",
                canonical_area_local_id(seg)
            );
        }
    }
}

#[test]
fn issue1704_every_spec_is_intra_area_valid() {
    // Independent cross-check: the corpus is also individually well-formed.
    for spec in full_authored_corpus() {
        validate_area_spec(&spec)
            .unwrap_or_else(|e| panic!("spec {} is not intra-area valid: {e}", spec.area_id));
    }
}
