use std::fs;

use litd_editor::{
    vfx_library::{
        VfxLibraryAssetInput, VfxLibrarySearchRequest, build_vfx_library_index,
        build_vfx_library_ledger, dedup_vfx_library, parse_vfx_library_ledger,
        read_vfx_library_ledger, render_vfx_library_preview, search_available_vfx_library,
        search_vfx_library, write_vfx_library_ledger,
    },
    vfx_text_generation::{VfxTextGenerationRequest, generate_vfx_text_proposal},
};
use litd_engine::render::vfx::write_effect_to_content_store;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_VFX_LIBRARY_FSV_ROOT",
        "issue750_vfx_library",
        "manual",
    )?;
    let store_root = root.join("store");
    let state_path = root.join("vfx-library-ledger.json");
    let aster_dir = root.join("aster");

    let source_before = source_state(&store_root, &aster_dir, "issue750-library-ledger");
    let inputs = vec![
        generated_input(
            &root,
            &store_root,
            "fire-a",
            "a fiery explosion with rising embers",
        )?,
        generated_input(
            &root,
            &store_root,
            "fire-b",
            "a fiery explosion with 32768 particles and rising embers",
        )?,
        generated_input(&root, &store_root, "lightning-a", "a lightning bolt strike")?,
    ];
    let after_ingest = source_state(&store_root, &aster_dir, "issue750-library-ledger");
    let index = build_vfx_library_index(&store_root, "issue750-library-index", inputs)?;
    let search = search_vfx_library(
        &index,
        &request("issue750-search-fire", "fire explosion embers"),
    )?;
    let no_match = search_vfx_library(
        &index,
        &request("issue750-search-zero", "healing water aura"),
    )?;
    let offline_error =
        search_available_vfx_library(None, &request("issue750-search-offline", "fire explosion"))
            .expect_err("offline search must fail closed")
            .to_string();
    let dedup = dedup_vfx_library(&index)?;
    let ledger = build_vfx_library_ledger(
        "issue750-library-ledger",
        index,
        vec![search.clone(), no_match.clone()],
        dedup.clone(),
    )?;
    let writeback = write_vfx_library_ledger(&state_path, &aster_dir, &ledger)?;
    let ledger_readback = read_vfx_library_ledger(&aster_dir, "issue750-library-ledger")?
        .ok_or("library ledger missing after write")?;
    let parsed = parse_vfx_library_ledger(&fs::read(&state_path)?)?;
    let final_state = source_state(&store_root, &aster_dir, "issue750-library-ledger");

    let report = json!({
        "issue": 750,
        "source_of_truth": {
            "content_store": store_root,
            "aster_ledger_dir": aster_dir,
            "json_mirror": state_path
        },
        "states": {
            "before": source_before,
            "after_ingest": after_ingest,
            "after_ledger": final_state
        },
        "happy_path": {
            "asset_count": parsed.index.asset_count,
            "search": search,
            "dedup_pairs": dedup,
            "ledger": {
                "seq": ledger_readback.seq,
                "graph_rows": ledger_readback.graph_rows,
                "sha256": ledger_readback.sha256,
                "bytes": ledger_readback.bytes.len(),
                "asset_count": ledger_readback.asset_count,
                "search_count": ledger_readback.search_count,
                "dedup_pair_count": ledger_readback.dedup_pair_count
            },
            "writeback_sha256": writeback.sha256
        },
        "edges": {
            "zero_matches": no_match,
            "offline_disabled": {
                "error": offline_error
            }
        }
    });

    let report_path = root.join("vfx-library-fsv-readback.json");
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&report_path, &report_bytes)?;
    let persisted = fs::read(&report_path)?;
    let report_readback = fsv_run.record_json_file(&report_path)?;
    println!(
        "issue750 vfx_library_fsv report={} report_bytes={} report_sha256={} ledger_seq={} ledger_sha256={} assets={} searches={} dedup_pairs={} top_hit={} zero_status={:?}",
        report_path.display(),
        persisted.len(),
        report_readback.sha256,
        ledger_readback.seq,
        ledger_readback.sha256,
        parsed.index.asset_count,
        parsed.search_reports.len(),
        parsed.dedup_pairs.len(),
        search
            .hits
            .first()
            .map(|hit| hit.asset_id.as_str())
            .unwrap_or("none"),
        no_match.status
    );
    Ok(())
}

fn generated_input(
    root: &std::path::Path,
    store_root: &std::path::Path,
    asset_id: &str,
    prompt: &str,
) -> Result<VfxLibraryAssetInput, Box<dyn std::error::Error>> {
    let proposal = generate_vfx_text_proposal(VfxTextGenerationRequest {
        request_id: format!("issue750-manual-{asset_id}"),
        prompt: prompt.to_string(),
        seed: 750,
        requested_by: "litd-editor-vfx-library-fsv".to_string(),
    })?;
    let writeback = write_effect_to_content_store(store_root, &proposal.effect)?;
    let preview = render_vfx_library_preview(
        &proposal.effect,
        root.join("previews").join(format!("{asset_id}.bmp")),
    )?;
    Ok(VfxLibraryAssetInput {
        asset_id: asset_id.to_string(),
        descriptor: writeback.descriptor,
        preview,
    })
}

fn request(id: &str, query: &str) -> VfxLibrarySearchRequest {
    VfxLibrarySearchRequest {
        query_id: id.to_string(),
        query: query.to_string(),
        limit: 8,
        requested_by: "litd-editor-vfx-library-fsv".to_string(),
    }
}

fn source_state(
    store_root: &std::path::Path,
    aster_dir: &std::path::Path,
    ledger_id: &str,
) -> serde_json::Value {
    json!({
        "store_objects": count_store_objects(store_root),
        "aster_dir_exists": aster_dir.exists(),
        "ledger_present": read_vfx_library_ledger(aster_dir, ledger_id)
            .ok()
            .flatten()
            .map(|readback| json!({
                "seq": readback.seq,
                "sha256": readback.sha256,
                "asset_count": readback.asset_count,
                "search_count": readback.search_count,
                "dedup_pair_count": readback.dedup_pair_count
            }))
    })
}

fn count_store_objects(root: &std::path::Path) -> usize {
    let object_root = root.join("particles").join("sha256");
    if !object_root.exists() {
        return 0;
    }
    let mut count = 0;
    let mut stack = vec![object_root];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                count += 1;
            }
        }
    }
    count
}
