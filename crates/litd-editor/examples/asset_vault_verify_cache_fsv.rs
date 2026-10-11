//! #1763 FSV: prove the content-addressed asset-vault verification cache makes a repeat load
//! skip the expensive per-file sha256 sweep, while a missing/truncated/corrupted fixture still
//! ERRORs loudly and immediately (never hangs).
//!
//! Source of truth: a SYNTHETIC vault built in an auto-cleaned temp dir (real files + a real
//! manifest with correct sha256/sizes) driven through the REAL public
//! `AssetVaultCatalog::load_from_manifest` path, plus the on-disk verification receipt the loader
//! writes. Synthetic so this proves the mechanism without the real multi-gigabyte vault -- the
//! very cost #1763 is about.
//!
//! Run: `cargo run -p litd-editor --example asset_vault_verify_cache_fsv --features fsv`

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use litd_editor::asset_vault::AssetVaultCatalog;
use litd_engine::test_temp::TestTempDir;
use litd_hash::sha256_hex;
use serde_json::json;

/// Enough files/bytes that the sha256 sweep is unmistakably measurable versus a metadata-only
/// cache hit, while staying tiny relative to the real vault (the minimal-sufficient sizing rule).
const FILE_COUNT: usize = 40;
const FILE_BYTES: usize = 1024 * 1024;
const CACHE_DIR_NAME: &str = ".litd-verify-cache";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    litd_engine::init_logging();
    let (fsv_run, root) = litd_editor::fsv_support::artifact_dir_from_env(
        "LITD_EDITOR_ASSET_VAULT_VERIFY_CACHE_FSV_ROOT",
        "issue1763_asset_vault_verify_cache",
        "manual",
    )?;
    let artifact_path = root.join("verify_cache_readback.json");
    let stats_path = root.join("stats.txt");

    let happy = happy_cold_then_warm()?;
    let missing = edge_missing_fixture()?;
    let grown = edge_size_change_fixture()?;
    let corrupted = edge_same_size_corruption()?;

    let artifact = json!({
        "issue": 1763,
        "source_of_truth": "synthetic local-asset-vault (temp) manifest + physical files + on-disk verification receipt",
        "happy_path": happy.to_json(),
        "edges": {
            "missing_fixture": missing.to_json(),
            "size_change_fixture": grown.to_json(),
            "same_size_corruption": corrupted.to_json()
        }
    });
    fs::write(&artifact_path, serde_json::to_vec_pretty(&artifact)?)?;
    let report_readback = fsv_run.record_json_file(&artifact_path)?;

    fs::write(
        &stats_path,
        format!(
            "issue=1763\n\
             source_of_truth=synthetic vault manifest, physical bytes, verification receipt\n\
             files={FILE_COUNT} file_bytes={FILE_BYTES}\n\
             cold_ms={:.3} warm_ms={:.3} speedup={:.1}x\n\
             cold_receipts_before={} cold_receipts_after={} warm_receipts_after={}\n\
             cold_records={} warm_records={}\n\
             edge_missing_error={}\n\
             edge_size_change_error={}\n\
             edge_same_size_corruption_error={}\n\
             artifact_path={}\n\
             artifact_sha256={}\n",
            happy.cold_ms,
            happy.warm_ms,
            happy.speedup(),
            happy.receipts_before_cold,
            happy.receipts_after_cold,
            happy.receipts_after_warm,
            happy.cold_records,
            happy.warm_records,
            missing.error_code,
            grown.error_code,
            corrupted.error_code,
            artifact_path.display(),
            report_readback.sha256,
        ),
    )?;

    // Hard verdicts -- the harness FAILS if any is false, so a false-green is impossible.
    assert!(
        happy.receipts_before_cold == 0 && happy.receipts_after_cold == 1,
        "cold load must create exactly one receipt (miss -> full sweep -> record)"
    );
    assert!(
        happy.receipts_after_warm == 1,
        "warm load must reuse the same receipt, not add another"
    );
    assert_eq!(
        happy.cold_records, happy.warm_records,
        "warm load must return the identical record set"
    );
    assert!(
        happy.warm_ms * 2.0 < happy.cold_ms,
        "warm (cache-hit) load must be >=2x faster than the cold sweep: cold={:.3}ms warm={:.3}ms",
        happy.cold_ms,
        happy.warm_ms
    );
    assert!(
        missing.error_code.contains("file.exists"),
        "a missing fixture must ERROR immediately with file.exists, got {}",
        missing.error_code
    );
    assert!(
        grown.error_code.contains("file.bytes_on_disk"),
        "a resized fixture must ERROR immediately with file.bytes_on_disk, got {}",
        grown.error_code
    );
    assert!(
        corrupted.error_code.contains("file.sha256"),
        "same-size corruption must move the mtime, miss the cache, re-sweep, and ERROR with \
         file.sha256, got {}",
        corrupted.error_code
    );

    println!(
        "issue1763 verify-cache FSV OK cold_ms={:.3} warm_ms={:.3} speedup={:.1}x stats={}",
        happy.cold_ms,
        happy.warm_ms,
        happy.speedup(),
        stats_path.display()
    );
    Ok(())
}

struct HappyPath {
    cold_ms: f64,
    warm_ms: f64,
    cold_records: usize,
    warm_records: usize,
    receipts_before_cold: usize,
    receipts_after_cold: usize,
    receipts_after_warm: usize,
}

impl HappyPath {
    fn speedup(&self) -> f64 {
        if self.warm_ms <= 0.0 {
            f64::INFINITY
        } else {
            self.cold_ms / self.warm_ms
        }
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "cold_ms": self.cold_ms,
            "warm_ms": self.warm_ms,
            "speedup": self.speedup(),
            "cold_records": self.cold_records,
            "warm_records": self.warm_records,
            "receipts_before_cold": self.receipts_before_cold,
            "receipts_after_cold": self.receipts_after_cold,
            "receipts_after_warm": self.receipts_after_warm
        })
    }
}

fn happy_cold_then_warm() -> Result<HappyPath, Box<dyn std::error::Error>> {
    let vault = TestTempDir::new("issue1763-verify-cache-happy");
    let manifest = build_synthetic_vault(vault.path())?;
    let cache_dir = vault.path().join(CACHE_DIR_NAME);

    let receipts_before_cold = receipt_count(&cache_dir);
    let cold_start = Instant::now();
    let cold = AssetVaultCatalog::load_from_manifest(&manifest, "download")?;
    let cold_ms = cold_start.elapsed().as_secs_f64() * 1000.0;
    let receipts_after_cold = receipt_count(&cache_dir);

    let warm_start = Instant::now();
    let warm = AssetVaultCatalog::load_from_manifest(&manifest, "download")?;
    let warm_ms = warm_start.elapsed().as_secs_f64() * 1000.0;
    let receipts_after_warm = receipt_count(&cache_dir);

    println!(
        "happy before_receipts={receipts_before_cold} cold_ms={cold_ms:.3} after_cold_receipts={receipts_after_cold} warm_ms={warm_ms:.3} after_warm_receipts={receipts_after_warm}"
    );
    Ok(HappyPath {
        cold_ms,
        warm_ms,
        cold_records: cold.record_count(),
        warm_records: warm.record_count(),
        receipts_before_cold,
        receipts_after_cold,
        receipts_after_warm,
    })
}

struct EdgeAudit {
    error_code: String,
    before_state: String,
    action: String,
    after_state: String,
}

impl EdgeAudit {
    fn to_json(&self) -> serde_json::Value {
        json!({
            "error_code": self.error_code,
            "before_state": self.before_state,
            "action": self.action,
            "after_state": self.after_state
        })
    }
}

fn edge_missing_fixture() -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let vault = TestTempDir::new("issue1763-verify-cache-missing");
    let manifest = build_synthetic_vault(vault.path())?;
    AssetVaultCatalog::load_from_manifest(&manifest, "download")?; // warm receipt first
    let victim = vault.path().join("sources/synthetic/asset_000.bin");
    let before_state = format!("exists={}", victim.exists());
    fs::remove_file(&victim)?;
    let error_code = expect_error(&manifest, "missing fixture unexpectedly loaded");
    let after_state = format!("exists={}", victim.exists());
    println!(
        "edge=missing_fixture before={before_state} action=remove after={after_state} error={error_code}"
    );
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("remove {}", victim.display()),
        after_state,
    })
}

fn edge_size_change_fixture() -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let vault = TestTempDir::new("issue1763-verify-cache-grow");
    let manifest = build_synthetic_vault(vault.path())?;
    AssetVaultCatalog::load_from_manifest(&manifest, "download")?;
    let victim = vault.path().join("sources/synthetic/asset_001.bin");
    let before_len = fs::metadata(&victim)?.len();
    let before_state = format!("len={before_len}");
    let mut bytes = fs::read(&victim)?;
    bytes.push(0x7f); // one extra byte -> size no longer matches the manifest
    fs::write(&victim, &bytes)?;
    let after_len = fs::metadata(&victim)?.len();
    let error_code = expect_error(&manifest, "resized fixture unexpectedly loaded");
    let after_state = format!("len={after_len}");
    println!(
        "edge=size_change before={before_state} action=append_byte after={after_state} error={error_code}"
    );
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("append 1 byte to {}", victim.display()),
        after_state,
    })
}

fn edge_same_size_corruption() -> Result<EdgeAudit, Box<dyn std::error::Error>> {
    let vault = TestTempDir::new("issue1763-verify-cache-corrupt");
    let manifest = build_synthetic_vault(vault.path())?;
    AssetVaultCatalog::load_from_manifest(&manifest, "download")?; // writes receipt
    let victim = vault.path().join("sources/synthetic/asset_002.bin");
    let before_hash = sha256_hex(&fs::read(&victim)?);
    let before_state = format!("sha256={before_hash}");
    let mut bytes = fs::read(&victim)?;
    bytes[0] ^= 0x5a; // same size, different content -> writing moves the mtime
    fs::write(&victim, &bytes)?;
    let after_hash = sha256_hex(&fs::read(&victim)?);
    let error_code = expect_error(
        &manifest,
        "same-size corruption slipped past the cache re-sweep",
    );
    let after_state = format!("sha256={after_hash}");
    println!(
        "edge=same_size_corruption before={before_state} action=flip_byte after={after_state} error={error_code}"
    );
    Ok(EdgeAudit {
        error_code,
        before_state,
        action: format!("flip 1 byte (same size) in {}", victim.display()),
        after_state,
    })
}

fn expect_error(manifest: &Path, on_ok: &str) -> String {
    match AssetVaultCatalog::load_from_manifest(manifest, "download") {
        Ok(_) => panic!("{on_ok}"),
        Err(error) => error.to_string(),
    }
}

fn receipt_count(cache_dir: &Path) -> usize {
    match fs::read_dir(cache_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("receipt")
            })
            .count(),
        Err(_) => 0,
    }
}

/// Build a real manifest + real files under `<vault>/sources/synthetic/`, returning the manifest
/// path. Every record carries its true sha256 and byte length so the loader accepts it.
fn build_synthetic_vault(vault_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let manifest_dir = vault_root.join("manifests");
    fs::create_dir_all(&manifest_dir)?;
    let sources_dir = vault_root.join("sources/synthetic");
    fs::create_dir_all(&sources_dir)?;

    let mut lines = String::new();
    for index in 0..FILE_COUNT {
        let relative_path = format!("sources/synthetic/asset_{index:03}.bin");
        let absolute_path = vault_root.join(&relative_path);
        let bytes = synthetic_bytes(index);
        fs::write(&absolute_path, &bytes)?;
        let record = json!({
            "record_type": "download",
            "source": "synthetic",
            "asset_id": format!("synthetic-{index:03}"),
            "asset_type": "model",
            "title": format!("Synthetic Asset {index:03}"),
            "license": "CC0-1.0",
            "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
            "source_url": "https://example.invalid/synthetic",
            "relative_path": relative_path,
            "absolute_path": absolute_path.display().to_string(),
            "bytes_on_disk": bytes.len() as u64,
            "sha256": sha256_hex(&bytes),
            "status": "downloaded"
        });
        lines.push_str(&record.to_string());
        lines.push('\n');
    }
    let manifest = manifest_dir.join("asset_intake_manifest.jsonl");
    fs::write(&manifest, lines)?;
    Ok(manifest)
}

/// Deterministic, index-varied content so each file has a distinct sha256.
fn synthetic_bytes(index: usize) -> Vec<u8> {
    let seed = (index as u8).wrapping_mul(31).wrapping_add(7);
    (0..FILE_BYTES)
        .map(|offset| seed.wrapping_add((offset & 0xff) as u8))
        .collect()
}
