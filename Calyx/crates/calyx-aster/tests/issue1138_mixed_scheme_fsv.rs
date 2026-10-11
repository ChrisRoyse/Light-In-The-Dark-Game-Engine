//! Full-State-Verification for issue #1138 against a REAL on-disk store.
//!
//! Point `CALYX_FSV_STORE` at a vault directory (the one that blocked the
//! editor: `.../litd-editor-shell-state-aster`). The test proves, over the
//! real bytes on disk:
//!
//!   1. BEFORE — the raw `cf/graph` layout (507 legacy `{ordinal:020}.sst`
//!      pairs + 507 commit-domain `{seq:020}-0000.sst`) is rejected by
//!      `ensure_unambiguous_sst_order` with `CALYX_ASTER_SST_ORDER_AMBIGUOUS`
//!      — the exact error the editor hit at `editor.state.aster.open`.
//!   2. AFTER — opening the store with `CfRouter` migrates every legacy file
//!      into the watermark-0 commit-anchored `flush-*` shape, the open
//!      succeeds, no `RouterLegacy` file remains, and the reader yields a
//!      deterministic row set (identical across two independent opens),
//!      proving newest-wins ordering is now well-defined.
//!
//! Run with (copy first — the migration mutates the directory):
//!   CALYX_FSV_STORE=<copy> cargo test -p calyx-aster --test \
//!     issue1138_mixed_scheme_fsv -- --ignored --nocapture

use calyx_aster::cf::{CfRouter, ColumnFamily};
use calyx_aster::storage_names::{
    SstName, classify_sst, ensure_unambiguous_sst_order, migrate_legacy_flush_names,
};
use std::fs;
use std::path::{Path, PathBuf};

fn graph_dir(store: &Path) -> PathBuf {
    store.join("cf").join("graph")
}

fn list(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect()
}

fn count_class(dir: &Path, want: impl Fn(&SstName) -> bool) -> usize {
    list(dir)
        .iter()
        .filter_map(|path| classify_sst(path).ok().flatten())
        .filter(|name| want(name))
        .count()
}

#[test]
#[ignore = "FSV against a real store; set CALYX_FSV_STORE to a COPY of the vault"]
fn real_mixed_scheme_store_migrates_and_opens_deterministically() {
    let Ok(store) = std::env::var("CALYX_FSV_STORE") else {
        panic!("set CALYX_FSV_STORE to a copy of the blocked vault directory");
    };
    let store = PathBuf::from(store);
    let graph = graph_dir(&store);
    assert!(graph.is_dir(), "no cf/graph under {}", store.display());

    // --- BEFORE: the raw layout is the #1138 ambiguity. ---
    let legacy = count_class(&graph, |name| matches!(name, SstName::RouterLegacy { .. }));
    let commit = count_class(&graph, |name| {
        matches!(
            name,
            SstName::DurableBatch { .. } | SstName::Compacted { .. }
        )
    });
    println!("BEFORE: graph legacy={legacy} commit-domain={commit}");
    assert!(legacy > 0, "expected legacy files in the repro store");
    let before = list(&graph);
    let err = ensure_unambiguous_sst_order(before.iter().map(PathBuf::as_path))
        .expect_err("raw mixed-scheme graph layout must be ambiguous before the fix");
    assert_eq!(err.code, "CALYX_ASTER_SST_ORDER_AMBIGUOUS");
    println!("BEFORE: rejected -> {}: {}", err.code, err.message);

    // --- AFTER: open (migrates in-place) succeeds. ---
    let router = CfRouter::open(&store, 0).expect("store must open after #1138 migration");
    let rows = router
        .iter_cf(ColumnFamily::Graph)
        .expect("iterate migrated graph CF");
    println!("AFTER: opened; graph rows read = {}", rows.len());

    // No legacy files remain; each became a watermark-0 flush.
    let legacy_after = count_class(&graph, |name| matches!(name, SstName::RouterLegacy { .. }));
    let flush_after = count_class(&graph, |name| {
        matches!(name, SstName::Flush { watermark: 0, .. })
    });
    assert_eq!(
        legacy_after, 0,
        "no RouterLegacy file may survive migration"
    );
    assert_eq!(
        flush_after, legacy,
        "every legacy file must become a watermark-0 flush"
    );
    println!("AFTER: legacy={legacy_after} watermark0-flush={flush_after}");

    // Determinism: a second independent open yields the identical row set.
    let router2 = CfRouter::open(&store, 0).expect("reopen migrated store");
    let rows2 = router2.iter_cf(ColumnFamily::Graph).expect("iterate again");
    assert_eq!(
        rows, rows2,
        "newest-wins order must be deterministic across opens"
    );
    // Idempotent migration: the second open renamed nothing.
    assert_eq!(migrate_legacy_flush_names(&graph).unwrap(), 0);
    println!(
        "AFTER: deterministic across reopen ({} rows); migration idempotent",
        rows2.len()
    );
}
