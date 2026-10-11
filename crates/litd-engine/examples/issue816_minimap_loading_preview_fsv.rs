//! FSV for #816: minimap generation + loading screen + map preview.
//!
//! The minimap is EDITOR authoring output baked as DATA (RTS ships it, #1783;
//! never a default RPG HUD map, #944 -- the payload records rpg_runtime_hud=false).
//! Generate a minimap by box-downsampling an authored source colour grid, prove the
//! generation is DETERMINISTIC (same bytes -> same fingerprint) and matches an
//! independent reference downsample, then author the minimap + loading-screen
//! surface, persist through the real content path (pending_measurement, no ORT),
//! lower it, and read the row back with a fresh vault handle. Three edges prove the
//! generation + authoring invariants fail loud.

use std::error::Error;

use litd_engine::{
    calyx_content::{
        ENGINE_CONTENT_MEASUREMENT_STATE_PENDING, EngineCalyxContentStore, EngineContentKind,
        EngineContentMeasurementConfig,
    },
    calyx_persistence::{EngineCalyxDatabase, EngineCalyxDatabaseConfig},
    fsv_artifacts::begin_issue_run,
    world_environment::{
        LoadingScreen, MinimapAuthoring, MinimapSource, PreviewImageRef, Rgba8,
        WorldEnvironmentError, minimap::generate_minimap, record::persist_and_lower_environment,
    },
};
use serde_json::json;

#[path = "world_environment_fsv_common/fixture.rs"]
mod fixture;

const SRC_W: u32 = 4;
const SRC_H: u32 = 4;

/// A deterministic 4x4 authored source grid: a red/blue diagonal split.
fn source_grid() -> Vec<Rgba8> {
    let mut grid = Vec::with_capacity((SRC_W * SRC_H) as usize);
    for y in 0..SRC_H {
        for x in 0..SRC_W {
            if x + y < 4 {
                grid.push(Rgba8::new(200, 40, 40, 255));
            } else {
                grid.push(Rgba8::new(30, 60, 200, 255));
            }
        }
    }
    grid
}

/// Independent reference: box-average the 4x4 grid to 2x2 by hand.
fn reference_2x2(grid: &[Rgba8]) -> Vec<u32> {
    let mut out = Vec::new();
    for by in 0..2u32 {
        for bx in 0..2u32 {
            let (mut r, mut g, mut b, mut a) = (0u32, 0u32, 0u32, 0u32);
            for dy in 0..2u32 {
                for dx in 0..2u32 {
                    let x = bx * 2 + dx;
                    let y = by * 2 + dy;
                    let t = grid[(y * SRC_W + x) as usize];
                    r += u32::from(t.r);
                    g += u32::from(t.g);
                    b += u32::from(t.b);
                    a += u32::from(t.a);
                }
            }
            out.push(
                Rgba8::new((r / 4) as u8, (g / 4) as u8, (b / 4) as u8, (a / 4) as u8)
                    .packed_rgba(),
            );
        }
    }
    out
}

fn minimap_fixture(id: &str, blob: &str) -> MinimapAuthoring {
    MinimapAuthoring {
        id: id.to_string(),
        display_name: "Shore Assault Preview".to_string(),
        author: "issue816-fsv".to_string(),
        source: MinimapSource::TerrainAlbedo,
        resolution_px: 128,
        rts_shipped_as_data: true,
        loading_screen: LoadingScreen {
            title: "Shore Assault".to_string(),
            subtitle: "2v2 melee".to_string(),
            preview_image: PreviewImageRef {
                blob_sha256: blob.to_string(),
                width_px: 1024,
                height_px: 576,
            },
        },
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    litd_engine::init_logging();
    let run = begin_issue_run("issue816_minimap_loading_preview")?;
    let vault_dir = run.run_dir().join("engine-aster-vault");

    // Minimap generation: box-downsample 4x4 -> 2x2, deterministic + reference-checked.
    let grid = source_grid();
    let gen_a = generate_minimap(&grid, SRC_W, SRC_H, 2)?;
    let gen_b = generate_minimap(&grid, SRC_W, SRC_H, 2)?;
    let deterministic = gen_a.fingerprint == gen_b.fingerprint;
    let reference = reference_2x2(&grid);
    let matches_reference = gen_a.pixels == reference;

    let blob = litd_hash::sha256_hex(b"issue816-preview-blob");
    let database = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let store = EngineCalyxContentStore::with_measurement_config(
        database,
        EngineContentMeasurementConfig::from_preloaded_state(
            "addon-absent:issue816",
            fixture::unavailable_panel_state(),
        ),
    );

    let authoring = minimap_fixture("minimap-shore-issue816", &blob);
    let payload = authoring.to_payload()?;
    let readback = persist_and_lower_environment(
        &store,
        &authoring.author,
        "editor://worlds/shore/minimap",
        &authoring.display_name,
        &payload,
    )?;

    let fresh = EngineCalyxDatabase::open(EngineCalyxDatabaseConfig::for_vault(&vault_dir))?;
    let fresh_store = EngineCalyxContentStore::with_measurement_config(
        fresh,
        EngineContentMeasurementConfig::from_preloaded_state(
            "separate-readback:issue816",
            fixture::unavailable_panel_state(),
        ),
    );
    let on_disk = fresh_store.get_content_for_editor(EngineContentKind::Terrain, &authoring.id)?;
    let no_rpg_hud = on_disk.payload["rpg_runtime_hud"] == false;
    let rts_shipped = on_disk.payload["rts_shipped_as_data"] == true;

    let edges = json!({
        "bad_source_length_rejected": generate_minimap(&grid, 3, 3, 2).is_err(),
        "zero_resolution_rejected": generate_minimap(&grid, SRC_W, SRC_H, 0).is_err(),
        "bad_preview_blob_rejected": reject_bad_blob().is_err(),
    });

    let acceptance = json!({
        "minimap_generation_deterministic": deterministic,
        "minimap_matches_reference_downsample": matches_reference,
        "persisted_pending_without_ort":
            readback.row.measurement.measurement_state == ENGINE_CONTENT_MEASUREMENT_STATE_PENDING,
        "declared_gate_binding": readback.row.measurement.declared_learned_lenses >= 10,
        "lowered_field_count_positive": !readback.lowered.fields.is_empty(),
        "on_disk_sha_matches_writer": on_disk.database.sha256 == readback.row.database.sha256,
        "on_disk_no_rpg_runtime_hud": no_rpg_hud,
        "on_disk_rts_shipped_as_data": rts_shipped,
        "edge_bad_source_length_rejected": edges["bad_source_length_rejected"],
        "edge_zero_resolution_rejected": edges["zero_resolution_rejected"],
        "edge_bad_preview_blob_rejected": edges["bad_preview_blob_rejected"],
    });

    let report = json!({
        "schema": "litd.engine.issue816.minimap_loading_preview_fsv.v1",
        "issue": 816,
        "source_of_truth": {
            "report_path": run.report_path().display().to_string(),
            "engine_vault_dir": vault_dir.display().to_string(),
            "minimap_fingerprint": gen_a.fingerprint,
            "environment": { "ORT_DYLIB_PATH_present": std::env::var_os("ORT_DYLIB_PATH").is_some() },
            "manual_gate": "Read this report + the persisted Aster row; command success alone is not proof."
        },
        "sufficiency_argument": "A 4x4 -> 2x2 box downsample is the smallest input that exercises generation over a non-trivial (>1 pixel) minimap with a hand-computed reference to check against, plus determinism (same bytes -> same fingerprint). One minimap+loading-screen surface then exercises persist (pending_measurement), a durable readback that still records rpg_runtime_hud=false (#944) and rts_shipped_as_data=true (#1783), and lowering. Three edges prove the source-length, resolution, and preview-blob invariants fail loud.",
        "minimap_generation": {
            "resolution_px": gen_a.resolution_px,
            "fingerprint": gen_a.fingerprint,
            "deterministic": deterministic,
            "matches_reference": matches_reference
        },
        "persist": readback.evidence(),
        "edges": edges,
        "acceptance": acceptance
    });

    let written = run.write_report(&report)?;
    for (field, value) in written.json["acceptance"].as_object().expect("acceptance") {
        if value != true {
            return Err(format!("issue816 acceptance {field} was not true").into());
        }
    }
    println!(
        "ISSUE816_MINIMAP_FSV report={} sha256={} minimap_fp={} deterministic={} no_rpg_hud={}",
        written.path.display(),
        written.sha256,
        &gen_a.fingerprint[..12],
        deterministic,
        no_rpg_hud,
    );
    Ok(())
}

fn reject_bad_blob() -> Result<(), WorldEnvironmentError> {
    let mut a = minimap_fixture("edge-blob", "not-a-valid-sha");
    a.loading_screen.preview_image.blob_sha256 = "not-a-valid-sha".to_string();
    a.validate()
}
