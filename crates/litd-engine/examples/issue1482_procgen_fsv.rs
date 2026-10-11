//! Full-State-Verification harness for procedural geometry generation (#1482).
//!
//! Source of truth: the frozen mesh bytes + content hash produced by
//! `litd_engine::procgen`, persisted through the content-addressed store and
//! read back off disk. This harness runs with `harness = false` so `main` runs
//! under `cargo run --example issue1482_procgen_fsv` (and `cargo test
//! --example`) — it exits non-zero and prints a structured error on any
//! mismatch (no false-green).

use std::fs;

use litd_engine::{
    EngineError,
    fsv_artifacts::begin_issue_run,
    procgen::{
        HeightfieldSpec, PrimitiveSpec, ProcGenStore, ScatterPrototype, ScatterSpec,
        generate_heightfield, generate_primitive, generate_primitive_frozen, generate_scatter,
    },
};
use litd_hash::sha256_hex;
use serde_json::json;

fn main() -> Result<(), EngineError> {
    litd_engine::init_logging();
    // Immutable per-run artifact dir under target/fsv/issue1482_procgen (no raw
    // env::temp_dir(), #1535). The store lives inside this run's directory.
    let run = begin_issue_run("issue1482_procgen").map_err(|error| io_err("begin_run", error))?;
    let root = run
        .artifact_dir("workspace")
        .map_err(|error| io_err("artifact_dir", error))?;

    // (1) 2x2x2 box via the engine API: known counts + known vertex positions.
    let box_spec = PrimitiveSpec::Box {
        size: [2.0, 2.0, 2.0],
    };
    let mesh = generate_primitive(&box_spec, "fsv.box.2x2x2")?;
    assert_eq(mesh.vertices.len() as u64, 24, "box.vertex_count")?;
    assert_eq(mesh.indices.len() as u64, 36, "box.index_count")?;
    let frozen = mesh.freeze()?;
    frozen.verify()?;
    let sample_positions = frozen.sample_positions(3);
    expect_position(&sample_positions, 0, [1.0, -1.0, -1.0])?;
    expect_position(&sample_positions, 1, [1.0, -1.0, 1.0])?;
    expect_position(&sample_positions, 2, [1.0, 1.0, 1.0])?;
    for corner in frozen.vertices() {
        for axis in 0..3 {
            if (corner.position[axis].abs() - 1.0).abs() > 1.0e-5 {
                return Err(mismatch(
                    "box.corner_on_unit_cube",
                    "1.0",
                    "off-cube vertex",
                ));
            }
        }
    }

    // (2) Determinism: regenerate the same spec -> byte-identical frozen output.
    let frozen_again = generate_primitive_frozen(&box_spec, "fsv.box.2x2x2")?;
    if frozen.content_hash != frozen_again.content_hash
        || frozen.vertex_bytes != frozen_again.vertex_bytes
        || frozen.index_bytes != frozen_again.index_bytes
    {
        return Err(mismatch(
            "box.determinism",
            &frozen.content_hash,
            &frozen_again.content_hash,
        ));
    }

    // (3) Persist through the content-addressed store, read back BY HASH.
    let store = ProcGenStore::open(root.join("store"))?;
    let stored = store.store_mesh(&frozen)?;
    let loaded = store.load_mesh(&frozen.content_hash)?;
    loaded.verify()?;
    if loaded.vertex_bytes != frozen.vertex_bytes || loaded.content_hash != frozen.content_hash {
        return Err(mismatch(
            "store.roundtrip",
            &frozen.content_hash,
            &loaded.content_hash,
        ));
    }
    // Independent on-disk readback of the raw vertex blob.
    let disk_vertices =
        fs::read(&stored.vertex_path).map_err(|error| io_err("disk.read", error))?;
    if sha256_hex(&disk_vertices) != frozen.vertex_sha256 {
        return Err(mismatch(
            "store.disk_vertex_sha256",
            &frozen.vertex_sha256,
            &sha256_hex(&disk_vertices),
        ));
    }

    // (4) Terrain heightfield + deterministic scatter (scope items b, c).
    let terrain = generate_heightfield(&HeightfieldSpec {
        tile_id: "fsv.terrain".to_string(),
        width: 5,
        depth: 5,
        cell_size: 1.0,
        heights: (0..25).map(|i| (i % 5) as f32 * 0.5).collect(),
        lod: 1,
        skirt: 0.5,
        splat_bands: Vec::new(),
    })?;
    terrain.mesh.freeze()?.verify()?;
    let scatter_a = generate_scatter(&scatter_fixture(1234))?;
    let scatter_b = generate_scatter(&scatter_fixture(1234))?;
    if scatter_a.content_hash != scatter_b.content_hash {
        return Err(mismatch(
            "scatter.determinism",
            &scatter_a.content_hash,
            &scatter_b.content_hash,
        ));
    }

    // Edge cases: each must fail LOUDLY.
    let zero_box = generate_primitive(
        &PrimitiveSpec::Box {
            size: [0.0, 2.0, 2.0],
        },
        "bad",
    )
    .is_err();
    let degenerate_tile = generate_heightfield(&HeightfieldSpec {
        tile_id: "bad".to_string(),
        width: 1,
        depth: 1,
        cell_size: 1.0,
        heights: vec![0.0],
        lod: 0,
        skirt: 0.0,
        splat_bands: Vec::new(),
    })
    .is_err();
    let bad_subdiv = generate_primitive(
        &PrimitiveSpec::Sphere {
            radius: 1.0,
            segments: 2,
            rings: 8,
        },
        "bad",
    )
    .is_err();
    if !(zero_box && degenerate_tile && bad_subdiv) {
        return Err(mismatch(
            "edge_cases.all_rejected",
            "true",
            &format!(
                "zero_box={zero_box} degenerate_tile={degenerate_tile} bad_subdiv={bad_subdiv}"
            ),
        ));
    }

    let report = json!({
        "issue": 1482,
        "source_of_truth": {
            "module": "crates/litd-engine/src/procgen.rs",
            "store_root": store.root().display().to_string(),
            "box_content_hash": frozen.content_hash,
            "box_vertex_sha256": frozen.vertex_sha256,
            "box_index_sha256": frozen.index_sha256,
        },
        "box_2x2x2": {
            "vertex_count": frozen.vertex_count,
            "index_count": frozen.index_count,
            "triangle_count": frozen.triangle_count,
            "vertex_stride": frozen.vertex_stride,
            "bounds_min": frozen.bounds_min,
            "bounds_max": frozen.bounds_max,
            "sample_positions": sample_positions,
        },
        "determinism": {
            "regenerated_content_hash": frozen_again.content_hash,
            "byte_identical": true,
        },
        "store_roundtrip": {
            "manifest_path": stored.manifest_path.display().to_string(),
            "loaded_content_hash": loaded.content_hash,
            "loaded_vertex_count": loaded.vertices().len(),
            "loaded_index_count": loaded.indices().len(),
        },
        "heightfield": {
            "lod_stride": terrain.lod_stride,
            "grid": [terrain.grid_width, terrain.grid_depth],
            "has_skirt": terrain.has_skirt,
            "vertex_count": terrain.mesh.vertices.len(),
        },
        "scatter": {
            "seed": scatter_a.seed,
            "instance_count": scatter_a.instance_count,
            "content_hash": scatter_a.content_hash,
        },
        "edge_cases": {
            "zero_size_box_rejected": zero_box,
            "degenerate_1x1_heightfield_rejected": degenerate_tile,
            "invalid_subdivision_rejected": bad_subdiv,
        },
    });
    // Write the report through the immutable-run helper, which reads it back off
    // disk, re-hashes it, and updates the latest-manifest pointer (independent
    // readback proof, #1344/#800).
    let readback = run
        .write_report(&report)
        .map_err(|error| io_err("report.write", error))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&report).unwrap_or_default()
    );
    println!(
        "OK issue1482 procgen FSV: report={} sha256={} manifest={}",
        readback.path.display(),
        readback.sha256,
        readback.latest_manifest_path.display(),
    );
    Ok(())
}

fn scatter_fixture(seed: u64) -> ScatterSpec {
    ScatterSpec {
        scatter_id: "fsv.rocks".to_string(),
        seed,
        count: 128,
        area: [40.0, 40.0],
        prototypes: vec![
            ScatterPrototype {
                prototype_id: "rock.small".to_string(),
                weight: 3.0,
            },
            ScatterPrototype {
                prototype_id: "rock.large".to_string(),
                weight: 1.0,
            },
        ],
        min_scale: 0.5,
        max_scale: 2.0,
        random_yaw: true,
    }
}

fn expect_position(
    positions: &[[f32; 3]],
    index: usize,
    expected: [f32; 3],
) -> Result<(), EngineError> {
    let actual = positions.get(index).copied().unwrap_or([f32::NAN; 3]);
    for axis in 0..3 {
        if (actual[axis] - expected[axis]).abs() > 1.0e-5 {
            return Err(mismatch(
                "box.sample_position",
                &format!("{expected:?}"),
                &format!("{actual:?}"),
            ));
        }
    }
    Ok(())
}

fn assert_eq(actual: u64, expected: u64, field: &str) -> Result<(), EngineError> {
    if actual != expected {
        return Err(mismatch(field, &expected.to_string(), &actual.to_string()));
    }
    Ok(())
}

fn mismatch(field: &str, expected: &str, actual: &str) -> EngineError {
    EngineError::ProcGenReadbackMismatch {
        field: field.to_string(),
        expected: expected.to_string(),
        actual: actual.to_string(),
    }
}

fn io_err(op: &str, error: std::io::Error) -> EngineError {
    EngineError::ProcGenPersistenceFailed {
        op: op.to_string(),
        reason: error.to_string(),
    }
}
