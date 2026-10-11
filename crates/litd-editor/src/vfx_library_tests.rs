use std::fs;

use litd_engine::render::vfx::{count_effects_in_content_store, write_effect_to_content_store};

use super::*;
use crate::vfx_text_generation::{VfxTextGenerationRequest, generate_vfx_text_proposal};

#[test]
fn vfx_library_search_dedup_and_edges_read_real_state() {
    let (_fsv_run, root) =
        crate::fsv_support::artifact_dir("issue750_vfx_library", "unit").unwrap();
    let _ = fs::remove_dir_all(&root);
    let store_root = root.join("store");
    let state_path = root.join("ledger-aster");
    let aster_dir = root.join("aster");

    let inputs = vec![
        generated_input(
            &root,
            &store_root,
            "fire-a",
            "a fiery explosion with rising embers",
        ),
        generated_input(
            &root,
            &store_root,
            "fire-b",
            "a fiery explosion with 32768 particles and rising embers",
        ),
        generated_input(&root, &store_root, "lightning-a", "a lightning bolt strike"),
    ];
    let before_objects = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue750 library before store_objects={} graph_rows={}",
        before_objects.effect_count, before_objects.graph_rows
    );

    let index = build_vfx_library_index(&store_root, "issue750-library-index", inputs).unwrap();
    let search = search_vfx_library(
        &index,
        &request("issue750-search-fire", "fire explosion embers"),
    )
    .unwrap();
    let no_match = search_vfx_library(
        &index,
        &request("issue750-search-zero", "healing water aura"),
    )
    .unwrap();
    let dedup = dedup_vfx_library(&index).unwrap();
    let ledger = build_vfx_library_ledger(
        "issue750-library-ledger",
        index.clone(),
        vec![search.clone(), no_match.clone()],
        dedup.clone(),
    )
    .unwrap();
    let writeback = write_vfx_library_ledger(&state_path, &aster_dir, &ledger).unwrap();
    let readback = read_vfx_library_ledger(&aster_dir, "issue750-library-ledger")
        .unwrap()
        .expect("library ledger should exist");
    let parsed = parse_vfx_library_ledger(&readback.bytes).unwrap();
    let after_objects = count_effects_in_content_store(&store_root).unwrap();
    println!(
        "issue750 library after store_objects={} store_graph_rows={} assets={} top_hit={} status={:?} no_match={:?} dedup_pairs={} ledger_sha={}",
        after_objects.effect_count,
        after_objects.graph_rows,
        parsed.index.asset_count,
        search.hits[0].asset_id,
        search.status,
        no_match.status,
        dedup.len(),
        readback.sha256
    );

    assert_eq!(before_objects.effect_count, 3);
    assert_eq!(after_objects.effect_count, 3);
    assert_eq!(search.status, VfxLibrarySearchStatus::Matched);
    assert_eq!(search.hits[0].asset_id, "fire-a");
    assert_eq!(no_match.status, VfxLibrarySearchStatus::NoMatches);
    assert_eq!(writeback.bytes, readback.bytes);
    assert!(!state_path.exists());
    assert_eq!(parsed.index.asset_count, 3);
    assert!(dedup.iter().any(|pair| {
        pair.left_asset_id == "fire-a"
            && pair.right_asset_id == "fire-b"
            && pair.verdict == VfxDedupVerdict::NearDuplicate
    }));
    assert!(dedup.iter().any(|pair| {
        pair.left_asset_id == "fire-a"
            && pair.right_asset_id == "lightning-a"
            && pair.verdict == VfxDedupVerdict::Distinct
    }));
}

#[test]
fn vfx_library_offline_search_fails_closed() {
    let error = search_available_vfx_library(None, &request("issue750-offline", "fire explosion"))
        .expect_err("missing library index must fail closed");
    println!("issue750 offline error={error}");
    assert!(error.to_string().contains("editor.vfx_library.offline"));
}

fn generated_input(
    root: &Path,
    store_root: &Path,
    asset_id: &str,
    prompt: &str,
) -> VfxLibraryAssetInput {
    let proposal = generate_vfx_text_proposal(VfxTextGenerationRequest {
        request_id: format!("issue750-{asset_id}"),
        prompt: prompt.to_string(),
        seed: 750,
        requested_by: "litd-editor-vfx-library-test".to_string(),
    })
    .unwrap();
    let writeback = write_effect_to_content_store(store_root, &proposal.effect).unwrap();
    let preview = render_vfx_library_preview(
        &proposal.effect,
        root.join("previews").join(format!("{asset_id}.bmp")),
    )
    .unwrap();
    VfxLibraryAssetInput {
        asset_id: asset_id.to_string(),
        descriptor: writeback.descriptor,
        preview,
    }
}

fn request(id: &str, query: &str) -> VfxLibrarySearchRequest {
    VfxLibrarySearchRequest {
        query_id: id.to_string(),
        query: query.to_string(),
        limit: 8,
        requested_by: "litd-editor-vfx-library-test".to_string(),
    }
}
