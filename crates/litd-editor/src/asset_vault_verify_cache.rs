//! #1763: content-addressed verification cache for the local asset vault.
//!
//! The problem this kills: `load_manifest_records` used to re-open and full-SHA256-hash EVERY
//! file named in the manifest on EVERY load. The real vault holds >=15,000 download files
//! totalling >=10 GiB (issue1242 floors), so a single cold load reads >10 GiB off disk -- and on
//! Windows every one of those 15k+ file opens triggers a Defender content scan. That one load
//! exceeded the 900s pre-push `test-gate:litd-editor` budget, so ANY push from a fresh worktree
//! (which shares the primary checkout's vault but starts with a cold OS page cache) timed out,
//! even when the change did not touch litd-editor at all (#1763, blocked PR #1762).
//!
//! The fix, following Cargo's own fingerprint doctrine and Gradle's content-addressable build
//! cache (the "mtime+size fingerprint, re-hash only on mismatch" hybrid): the expensive per-file
//! CONTENT sha256 sweep is now gated behind a derived, content-addressed receipt. The routine
//! load still ALWAYS does the cheap per-record metadata checks (exists / is-file / byte-length /
//! path consistency) so a missing or truncated fixture still ERRORs immediately and loudly. The
//! full content sweep runs only when the derived verification key -- SHA256 over the manifest
//! bytes plus every file's (relative_path, size, mtime) -- has no matching receipt. On success
//! the receipt is written into a cache directory beside the vault, so it is SHARED across every
//! linked worktree of the same primary checkout. A fresh worktree therefore reuses the receipt
//! the primary checkout already produced and never pays the multi-gigabyte re-hash.
//!
//! Integrity is preserved, not weakened: the key derives from the manifest (which pins each
//! expected sha256 and size) AND each file's on-disk (size, mtime). Any real content change
//! moves at least the mtime -- the standard cache-invalidation signal used by cargo, make, and
//! ninja -- so the key changes, the receipt misses, and the full sha256 sweep re-runs and catches
//! it. A truncation or replacement that also changes size is caught even earlier by the always-on
//! metadata pass. The cache is strictly a skip for work already proven for identical inputs.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

/// Cache-format version. Bump to invalidate every previously written receipt when the key
/// derivation or verification semantics change.
const RECEIPT_FORMAT: &str = "litd-asset-vault-verify-v1";
/// Directory name, beside the vault root, holding verification receipts. Leading dot keeps it out
/// of the way of the manifests/sources/extracted asset roots and any human browsing the vault.
const CACHE_DIR_NAME: &str = ".litd-verify-cache";

/// The on-disk state of one manifested file that feeds the content-addressed key.
pub(super) struct FileFingerprint {
    pub relative_path: String,
    pub size: u64,
    pub mtime_nanos: u128,
}

/// Nanoseconds since the Unix epoch for a file mtime, or 0 when the platform cannot report one
/// (older/exotic filesystems). A missing mtime only makes the key coarser -- the manifest bytes
/// and file sizes still participate -- so correctness is preserved; only the skip is less precise.
pub(super) fn mtime_nanos(modified: Option<SystemTime>) -> u128 {
    modified
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|delta| delta.as_nanos())
        .unwrap_or(0)
}

/// A decision about whether the expensive content sweep can be skipped, plus everything needed to
/// record success afterwards. Produced by [`plan`].
pub(super) struct VerificationPlan {
    key: String,
    receipt_path: PathBuf,
    hit: bool,
}

impl VerificationPlan {
    /// `true` when NO valid receipt exists for these exact inputs, so the caller must run the full
    /// per-file sha256 sweep. `false` means an identical input set was already fully verified.
    pub(super) fn needs_full_hash(&self) -> bool {
        !self.hit
    }

    /// Persist a receipt after the full sweep passed. Best-effort: a read-only or otherwise
    /// unwritable vault must NOT fail the load -- it only means the next load re-hashes. We log one
    /// line either way so the skip/miss behaviour is observable in gate output.
    pub(super) fn record_success(&self, record_count: usize) {
        match write_receipt(&self.receipt_path, &self.key, record_count) {
            Ok(()) => println!(
                "ASSET_VAULT_VERIFY_CACHE wrote receipt key={} records={} path={}",
                &self.key,
                record_count,
                self.receipt_path.display()
            ),
            Err(error) => println!(
                "ASSET_VAULT_VERIFY_CACHE receipt_write_skipped key={} reason={error} path={}",
                &self.key,
                self.receipt_path.display()
            ),
        }
    }
}

/// Derive the content-addressed verification key from the manifest bytes and every file's on-disk
/// fingerprint, then report whether a matching receipt already exists beside the vault.
pub(super) fn plan(
    vault_root: &Path,
    manifest_path: &Path,
    manifest_bytes: &[u8],
    fingerprints: &[FileFingerprint],
) -> VerificationPlan {
    let key = verification_key(manifest_path, manifest_bytes, fingerprints);
    let receipt_path = vault_root
        .join(CACHE_DIR_NAME)
        .join(format!("{key}.receipt"));
    let hit = receipt_is_valid(&receipt_path, &key);
    if hit {
        println!(
            "ASSET_VAULT_VERIFY_CACHE hit key={key} files={} manifest={}",
            fingerprints.len(),
            manifest_path.display()
        );
    } else {
        println!(
            "ASSET_VAULT_VERIFY_CACHE miss key={key} files={} manifest={} (full sha256 sweep will run)",
            fingerprints.len(),
            manifest_path.display()
        );
    }
    VerificationPlan {
        key,
        receipt_path,
        hit,
    }
}

/// SHA256 over the format tag, the manifest identity + bytes, and each file's
/// (relative_path, size, mtime). Field lengths are framed so no concatenation can alias another.
fn verification_key(
    manifest_path: &Path,
    manifest_bytes: &[u8],
    fingerprints: &[FileFingerprint],
) -> String {
    let mut hasher = Sha256::new();
    absorb(&mut hasher, RECEIPT_FORMAT.as_bytes());
    absorb(&mut hasher, manifest_path.to_string_lossy().as_bytes());
    absorb(&mut hasher, manifest_bytes);
    hasher.update((fingerprints.len() as u64).to_le_bytes());
    for fingerprint in fingerprints {
        absorb(&mut hasher, fingerprint.relative_path.as_bytes());
        hasher.update(fingerprint.size.to_le_bytes());
        hasher.update(fingerprint.mtime_nanos.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Length-prefixed absorb so `"ab" + "c"` never hashes the same as `"a" + "bc"`.
fn absorb(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// A receipt is valid only if it exists AND its single line is exactly the key it is named for.
/// The redundant in-file key defends against a truncated/partial write being trusted.
fn receipt_is_valid(receipt_path: &Path, key: &str) -> bool {
    match fs::read_to_string(receipt_path) {
        Ok(contents) => contents
            .lines()
            .next()
            .map(|line| line.trim() == key)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Write the receipt atomically-enough for a cache: create the dir, write a temp file, rename it
/// into place so a concurrent reader never sees a half-written receipt.
fn write_receipt(receipt_path: &Path, key: &str, record_count: usize) -> std::io::Result<()> {
    let dir = receipt_path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "receipt has no parent dir",
        )
    })?;
    fs::create_dir_all(dir)?;
    let temp_path = dir.join(format!("{key}.receipt.tmp"));
    {
        let mut file = fs::File::create(&temp_path)?;
        writeln!(file, "{key}")?;
        writeln!(
            file,
            "{RECEIPT_FORMAT} records={record_count} verified=full-sha256"
        )?;
        file.sync_all()?;
    }
    fs::rename(&temp_path, receipt_path)
}

#[cfg(test)]
#[path = "asset_vault_verify_cache_tests.rs"]
mod tests;
