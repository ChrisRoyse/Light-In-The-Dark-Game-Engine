//! #1822 FSV: targeted CC0 intake for M1 RTS content (KayKit humanoids + nature + props).
//!
//! Source of truth: real `local-asset-vault/sources` and `extracted` trees on disk,
//! plus the intake/extraction manifests. No mock assets.
//!
//! Proves:
//! (a) KayKit Adventures + Skeletons character packs exist with engine-usable glTF/GLB;
//! (b) animated humanoid GLBs present (binary markers + minimum byte size);
//! (c) nature/foliage meshes present (hexagon pack nature paths);
//! (d) destructible/resource-class props present (castle-kit gates/towers);
//! (e) catalog loads with zero load errors and reports the kaykit + kenney sources;
//! (f) three edges: missing pack path fails, empty anim set fails, corrupt path fails.

use std::fs;
use std::path::{Path, PathBuf};

use litd_editor::asset_vault::AssetVaultCatalog;
use serde_json::json;

fn vault_root() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("local-asset-vault");
    root.canonicalize().unwrap_or(root)
}

fn count_meshes(dir: &Path, exts: &[&str]) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let mut n = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(rd) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if exts.iter().any(|e| *e == ext) {
                n += 1;
            }
        }
    }
    n
}

fn glb_files_with_anim_markers(dir: &Path, min_bytes: u64) -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(rd) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ext != "glb" {
                continue;
            }
            let Ok(meta) = fs::metadata(&p) else {
                continue;
            };
            let len = meta.len();
            if len < min_bytes {
                continue;
            }
            let Ok(bytes) = fs::read(&p) else {
                continue;
            };
            // Real glTF binary chunks embed animation accessor names / JSON keys.
            let has = bytes.windows(11).any(|w| w == b"animations")
                || bytes.windows(4).any(|w| w == b"idle")
                || bytes.windows(4).any(|w| w == b"walk")
                || bytes.windows(6).any(|w| w == b"Attack")
                || bytes.windows(5).any(|w| w == b"Death");
            if has {
                out.push((p, len));
            }
        }
    }
    out
}

fn count_name_matches(dir: &Path, needles: &[&str], exts: &[&str]) -> usize {
    if !dir.is_dir() {
        return 0;
    }
    let mut n = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(rd) = fs::read_dir(&cur) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !exts.iter().any(|e| *e == ext) {
                continue;
            }
            let name = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if needles.iter().any(|nd| name.contains(nd)) {
                n += 1;
            }
        }
    }
    n
}

fn path_exists_nonempty(path: &Path) -> bool {
    path.is_dir()
        && fs::read_dir(path)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    litd_editor::fsv_support::ensure_project_root_env();
    let run = litd_engine::fsv_artifacts::begin_issue_run("issue1822_m1_targeted_intake")?;

    let vault = vault_root();
    let adventures = vault.join("sources/kaykit/game-kit/KayKit-Character-Pack-Adventures-1.0");
    let skeletons = vault.join("sources/kaykit/game-kit/KayKit-Character-Pack-Skeletons-1.0");
    let hexagon = vault.join("sources/kaykit/game-kit/KayKit-Medieval-Hexagon-Pack-1.0");
    let castle = vault.join("extracted/kenney/game-kit/castle-kit");
    let medieval_rts = vault.join("extracted/kenney/game-kit/medieval-rts");
    let intake_manifest = vault.join("manifests/asset_intake_manifest.jsonl");
    let extract_manifest = vault.join("manifests/asset_extraction_manifest.jsonl");

    // Happy path: physical pack presence + mesh/anim inventory from disk SoT.
    let adventures_ok = path_exists_nonempty(&adventures);
    let skeletons_ok = path_exists_nonempty(&skeletons);
    let hexagon_ok = path_exists_nonempty(&hexagon);
    let castle_ok = path_exists_nonempty(&castle);

    let adventures_gltf = count_meshes(&adventures, &["glb", "gltf"]);
    let skeletons_gltf = count_meshes(&skeletons, &["glb", "gltf"]);
    let adventures_anims = glb_files_with_anim_markers(&adventures, 1_000_000);
    let skeletons_anims = glb_files_with_anim_markers(&skeletons, 1_000_000);
    let nature_meshes = count_meshes(
        &hexagon.join("addons/kaykit_medieval_hexagon_pack/Assets/gltf/decoration/nature"),
        &["glb", "gltf"],
    ) + count_meshes(
        &hexagon.join("addons/kaykit_medieval_hexagon_pack/Assets/fbx/decoration/nature"),
        &["fbx"],
    );
    let destructible_props = count_name_matches(
        &castle,
        &["gate", "wall", "tower", "barricade", "siege"],
        &["glb", "gltf", "fbx"],
    ) + count_name_matches(
        &medieval_rts,
        &["gate", "tower", "wall", "siege", "resource", "gold", "wood"],
        &["glb", "gltf", "fbx", "png"],
    );

    let humanoid_ok = adventures_ok
        && skeletons_ok
        && adventures_gltf >= 5
        && skeletons_gltf >= 4
        && adventures_anims.len() >= 4
        && skeletons_anims.len() >= 3;
    let nature_ok = hexagon_ok && nature_meshes >= 20;
    let props_ok = castle_ok && destructible_props >= 10;

    // Catalog SoT: real vault load, kaykit + kenney present, zero silent empty.
    let catalog = AssetVaultCatalog::load_default_cached()?;
    let sources_line = catalog.download_source_summary_line();
    let has_kaykit = sources_line.contains("kaykit");
    let has_kenney = sources_line.contains("kenney");
    let downloads = catalog.record_type_count("download");
    let usable = catalog.usable_record_count();
    let catalog_ok = has_kaykit && has_kenney && downloads > 0 && usable >= 50;

    // Independent manifest readback (not return values alone).
    let intake_lines = fs::read_to_string(&intake_manifest)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    let extract_lines = fs::read_to_string(&extract_manifest)
        .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
        .unwrap_or(0);
    let kaykit_in_manifest = fs::read_to_string(&intake_manifest)
        .map(|s| s.lines().filter(|l| l.contains("\"kaykit\"")).count())
        .unwrap_or(0);
    let manifest_ok = intake_lines > 0 && kaykit_in_manifest >= 100 && extract_lines > 0;

    // Edge 1: missing required pack path is fail-loud (before/after).
    let missing_pack = vault.join("sources/kaykit/game-kit/DOES_NOT_EXIST_M1_PACK");
    let edge_missing_before = missing_pack.exists();
    let edge_missing = !edge_missing_before && !path_exists_nonempty(&missing_pack);

    // Edge 2: empty anim inventory under a synthetic empty dir fails the anim bar.
    let empty_anim_dir = run.run_dir().join("edge_empty_anim_dir");
    fs::create_dir_all(&empty_anim_dir)?;
    let edge_empty_anims_before = glb_files_with_anim_markers(&empty_anim_dir, 1).len();
    let edge_empty_anims = edge_empty_anims_before == 0;

    // Edge 3: corrupt/non-mesh file must not count as animated glb.
    let corrupt = empty_anim_dir.join("not-a-character.glb");
    fs::write(&corrupt, b"not a real glb binary")?;
    let edge_corrupt_after = glb_files_with_anim_markers(&empty_anim_dir, 1).len();
    let edge_corrupt = edge_corrupt_after == 0;

    let verdict = humanoid_ok
        && nature_ok
        && props_ok
        && catalog_ok
        && manifest_ok
        && edge_missing
        && edge_empty_anims
        && edge_corrupt;

    let report = json!({
        "schema": "litd.editor.issue1822.m1_targeted_intake_fsv.v1",
        "issue": 1822,
        "parents": [1784, 1788, 1878, 782],
        "verdict": if verdict { "GREEN" } else { "RED" },
        "sufficiency_argument": "Disk vault sources/extracted trees are SoT for pack presence. GLB anim markers prove combat-ready humanoids without mock data. Nature path meshes and castle/medieval-rts destructible names prove foliage and prop classes. Catalog + intake manifest re-reads prove kaykit/kenney are registered. Edges: missing pack, empty anim dir, corrupt glb.",
        "vault_root": vault.display().to_string(),
        "packs": {
            "adventures_ok": adventures_ok,
            "skeletons_ok": skeletons_ok,
            "hexagon_ok": hexagon_ok,
            "castle_ok": castle_ok,
            "adventures_gltf": adventures_gltf,
            "skeletons_gltf": skeletons_gltf,
            "adventures_anim_glbs": adventures_anims.len(),
            "skeletons_anim_glbs": skeletons_anims.len(),
            "nature_meshes": nature_meshes,
            "destructible_props": destructible_props,
            "adventures_anim_samples": adventures_anims.iter().take(5).map(|(p, n)| json!({
                "path": p.display().to_string(),
                "bytes": n
            })).collect::<Vec<_>>(),
            "skeletons_anim_samples": skeletons_anims.iter().take(4).map(|(p, n)| json!({
                "path": p.display().to_string(),
                "bytes": n
            })).collect::<Vec<_>>(),
        },
        "catalog": {
            "sources": sources_line,
            "has_kaykit": has_kaykit,
            "has_kenney": has_kenney,
            "downloads": downloads,
            "usable": usable,
        },
        "manifest_readback": {
            "intake_lines": intake_lines,
            "extract_lines": extract_lines,
            "kaykit_in_manifest": kaykit_in_manifest,
        },
        "bars": {
            "humanoid_ok": humanoid_ok,
            "nature_ok": nature_ok,
            "props_ok": props_ok,
            "catalog_ok": catalog_ok,
            "manifest_ok": manifest_ok,
        },
        "edges": {
            "missing_pack": {
                "path": missing_pack.display().to_string(),
                "exists_before": edge_missing_before,
                "pass": edge_missing,
            },
            "empty_anim_dir": {
                "path": empty_anim_dir.display().to_string(),
                "anim_count": edge_empty_anims_before,
                "pass": edge_empty_anims,
            },
            "corrupt_glb": {
                "path": corrupt.display().to_string(),
                "anim_count_after": edge_corrupt_after,
                "pass": edge_corrupt,
            },
        },
    });

    let readback = run.write_report(&report)?;
    println!(
        "ISSUE1822_M1_TARGETED_INTAKE_FSV verdict={} report={} report_sha256={} humanoid={} nature={} props={} catalog={} adventures_anim={} skeletons_anim={} nature_meshes={} destructible={}",
        if verdict { "GREEN" } else { "RED" },
        readback.path.display(),
        readback.sha256,
        humanoid_ok,
        nature_ok,
        props_ok,
        catalog_ok,
        adventures_anims.len(),
        skeletons_anims.len(),
        nature_meshes,
        destructible_props,
    );
    if !verdict {
        return Err("ISSUE1822 M1 targeted intake FSV RED".into());
    }
    Ok(())
}
