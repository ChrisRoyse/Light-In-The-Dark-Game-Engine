//! #1763: hermetic unit tests for the content-addressed verification cache. No real vault is
//! touched -- every input is synthesized and every receipt lives in an auto-cleaned temp dir --
//! so these run in milliseconds inside the routine gate.

use std::path::Path;

use litd_engine::test_temp::TestTempDir;

use super::{FileFingerprint, plan, verification_key};

fn fingerprint(relative_path: &str, size: u64, mtime_nanos: u128) -> FileFingerprint {
    FileFingerprint {
        relative_path: relative_path.to_string(),
        size,
        mtime_nanos,
    }
}

#[test]
fn key_is_stable_for_identical_inputs() {
    let manifest = Path::new("/vault/manifests/download.jsonl");
    let bytes = b"{\"record_type\":\"download\"}\n";
    let first = verification_key(manifest, bytes, &[fingerprint("a.fbx", 10, 100)]);
    let second = verification_key(manifest, bytes, &[fingerprint("a.fbx", 10, 100)]);

    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
    println!("edge=stable_key key={first}");
}

#[test]
fn key_changes_when_file_mtime_or_size_or_manifest_changes() {
    let manifest = Path::new("/vault/manifests/download.jsonl");
    let bytes = b"manifest-v1";
    let base = verification_key(manifest, bytes, &[fingerprint("a.fbx", 10, 100)]);
    let mtime_moved = verification_key(manifest, bytes, &[fingerprint("a.fbx", 10, 200)]);
    let size_moved = verification_key(manifest, bytes, &[fingerprint("a.fbx", 11, 100)]);
    let manifest_moved =
        verification_key(manifest, b"manifest-v2", &[fingerprint("a.fbx", 10, 100)]);

    assert_ne!(
        base, mtime_moved,
        "mtime change must move the key (re-hash trigger)"
    );
    assert_ne!(base, size_moved, "size change must move the key");
    assert_ne!(
        base, manifest_moved,
        "manifest byte change must move the key"
    );
    println!(
        "edge=key_invalidation base={base} mtime_moved={mtime_moved} size_moved={size_moved} manifest_moved={manifest_moved}"
    );
}

#[test]
fn length_framing_prevents_concatenation_aliasing() {
    let manifest = Path::new("/vault/manifests/download.jsonl");
    let bytes = b"m";
    // "ab" + "c" must not collide with "a" + "bc": length-prefixed absorb guarantees this.
    let left = verification_key(
        manifest,
        bytes,
        &[fingerprint("ab", 1, 0), fingerprint("c", 1, 0)],
    );
    let right = verification_key(
        manifest,
        bytes,
        &[fingerprint("a", 1, 0), fingerprint("bc", 1, 0)],
    );

    assert_ne!(left, right);
    println!("edge=no_aliasing left={left} right={right}");
}

#[test]
fn missing_receipt_is_a_miss_then_write_makes_it_a_hit() {
    let vault = TestTempDir::new("verify-cache-hit");
    let manifest = vault.path().join("manifests").join("download.jsonl");
    let bytes = b"content-addressed";
    let prints = vec![fingerprint("sources/a.fbx", 42, 7)];

    let cold = plan(vault.path(), &manifest, bytes, &prints);
    println!(
        "edge=cold_plan needs_full_hash={} (expected true)",
        cold.needs_full_hash()
    );
    assert!(
        cold.needs_full_hash(),
        "first plan for fresh inputs must miss"
    );
    cold.record_success(prints.len());

    let warm = plan(vault.path(), &manifest, bytes, &prints);
    println!(
        "edge=warm_plan needs_full_hash={} (expected false)",
        warm.needs_full_hash()
    );
    assert!(
        !warm.needs_full_hash(),
        "after record_success identical inputs must hit"
    );
}

#[test]
fn a_moved_mtime_after_a_hit_forces_a_fresh_miss() {
    let vault = TestTempDir::new("verify-cache-invalidate");
    let manifest = vault.path().join("manifests").join("download.jsonl");
    let bytes = b"content-addressed";

    let original = vec![fingerprint("sources/a.fbx", 42, 7)];
    plan(vault.path(), &manifest, bytes, &original).record_success(original.len());
    assert!(!plan(vault.path(), &manifest, bytes, &original).needs_full_hash());

    // Same size, DIFFERENT mtime -- the shape a content change leaves behind. Must re-miss so the
    // full sha256 sweep re-runs and can catch the change.
    let churned = vec![fingerprint("sources/a.fbx", 42, 999)];
    let after = plan(vault.path(), &manifest, bytes, &churned);
    println!(
        "edge=churned needs_full_hash={} (expected true)",
        after.needs_full_hash()
    );
    assert!(
        after.needs_full_hash(),
        "a moved mtime must invalidate the receipt"
    );
}

#[test]
fn a_truncated_receipt_is_not_trusted() {
    let vault = TestTempDir::new("verify-cache-truncated");
    let manifest = vault.path().join("manifests").join("download.jsonl");
    let bytes = b"content-addressed";
    let prints = vec![fingerprint("sources/a.fbx", 42, 7)];

    let key = verification_key(&manifest, bytes, &prints);
    let receipt = vault
        .path()
        .join(super::CACHE_DIR_NAME)
        .join(format!("{key}.receipt"));
    std::fs::create_dir_all(receipt.parent().unwrap()).unwrap();
    // A receipt whose recorded key does NOT match its filename key -- a partial/garbage write.
    std::fs::write(&receipt, "deadbeef\n").unwrap();

    let after = plan(vault.path(), &manifest, bytes, &prints);
    println!(
        "edge=truncated needs_full_hash={} (expected true) receipt={}",
        after.needs_full_hash(),
        receipt.display()
    );
    assert!(
        after.needs_full_hash(),
        "a receipt whose body key mismatches its name must be rejected"
    );
}
