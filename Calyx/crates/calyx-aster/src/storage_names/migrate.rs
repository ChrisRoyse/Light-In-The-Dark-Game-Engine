//! One-time on-disk migration of legacy router-flush SST names into the
//! commit domain (issue #1138).
//!
//! A pre-#1138 binary wrote router memtable flushes as `{ordinal:020}.sst`
//! ([`SstName::RouterLegacy`]), whose number is a per-CF flush *ordinal*, not
//! a commit seq. When such a directory also holds commit-domain files (durable
//! batches `{seq:020}-{index:04}.sst`, compaction outputs), a legacy ordinal
//! can numerically exceed a commit seq. Newest-wins ordering across the two
//! incomparable domains is then undefined, so [`ensure_unambiguous_sst_order`]
//! fails the open closed rather than risk serving stale rows.
//!
//! [`ensure_unambiguous_sst_order`]: super::ensure_unambiguous_sst_order
//!
//! This module removes the ambiguity *by construction* instead of merely
//! detecting it: it renames every legacy `{ordinal:020}.sst` to the
//! commit-anchored flush shape `flush-{0:020}-{ordinal:04}.sst`. Watermark `0`
//! is the maximum understatement of the file's true commit watermark, which
//! the writer contract (see [`crate::cf::CfRouter::flush_cf_at`]) guarantees is
//! always safe: the file sorts at the very start of the commit domain, so it
//! can never shadow a newer durable batch, and every row it holds keeps its
//! commit-domain durable home (the coverage invariant enforced fail-closed
//! since #1132/#1139). The migrated file classifies as [`SstName::Flush`],
//! which does not participate in the `RouterLegacy`-vs-commit-domain ambiguity
//! check at all, so a migrated directory has a single total SST order and can
//! never trip #1138 again.
//!
//! Design note (RocksDB parity): RocksDB keeps exactly one monotonic file
//! number domain and derives newest-wins order from the MANIFEST, so two
//! incomparable numbering domains never coexist for ordering. This migration
//! collapses Aster's two domains into the single commit domain to restore that
//! property for directories written before the domain split was fixed.
//!
//! The rename is atomic (`fs::rename`), idempotent (a directory with no legacy
//! files is a no-op), and content-preserving — no SST bytes are rewritten,
//! only the file name changes.

use super::{SstName, classify_sst, flush_sst_file_name};
use calyx_core::{CalyxError, Result};
use std::fs;
use std::path::Path;

/// Renames every legacy `{ordinal:020}.sst` router-flush file in `dir` into
/// the commit-anchored `flush-{0:020}-{ordinal:04}.sst` shape (issue #1138),
/// returning the number of files migrated.
///
/// A directory that holds no legacy files — already migrated, or written by a
/// post-#1138 binary — is a no-op that returns `Ok(0)`. A non-existent
/// directory is likewise `Ok(0)`. Fails closed (without renaming anything) if
/// a target name already exists, since that would collide with a distinct
/// canonical file, or if a legacy ordinal exceeds the platform `usize` range.
pub fn migrate_legacy_flush_names(dir: &Path) -> Result<usize> {
    if !dir.exists() {
        return Ok(0);
    }
    // Collect first, then rename, so a failure to plan any single file aborts
    // the whole migration before mutating the directory.
    let mut renames = Vec::new();
    for entry in fs::read_dir(dir).map_err(|error| {
        CalyxError::disk_pressure(format!("read CF dir for #1138 migration: {error}"))
    })? {
        let path = entry
            .map_err(|error| {
                CalyxError::disk_pressure(format!("read CF entry for #1138 migration: {error}"))
            })?
            .path();
        let Some(SstName::RouterLegacy { ordinal }) = classify_sst(&path)? else {
            continue;
        };
        let ordinal = usize::try_from(ordinal).map_err(|_| {
            CalyxError::aster_corrupt_shard(format!(
                "legacy flush ordinal {ordinal} in {} exceeds the platform usize range; \
                 cannot migrate to the commit-anchored flush name (issue #1138)",
                path.display()
            ))
        })?;
        let target = dir.join(flush_sst_file_name(0, ordinal));
        renames.push((path, target));
    }
    let mut migrated = 0;
    for (source, target) in renames {
        if target.exists() {
            return Err(CalyxError::aster_corrupt_shard(format!(
                "cannot migrate legacy flush {} to commit-anchored name {}: target already exists \
                 (issue #1138)",
                source.display(),
                target.display()
            )));
        }
        fs::rename(&source, &target).map_err(|error| {
            CalyxError::disk_pressure(format!(
                "rename legacy flush {} -> {} (issue #1138): {error}",
                source.display(),
                target.display()
            ))
        })?;
        migrated += 1;
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sst::write_sst;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("calyx-aster-migrate-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_legacy(dir: &Path, ordinal: u64, key: &[u8], value: &[u8]) {
        write_sst(dir.join(format!("{ordinal:020}.sst")), [(key, value)]).unwrap();
    }

    #[test]
    fn renames_every_legacy_flush_to_commit_anchored_shape() {
        let dir = scratch("rename-all");
        write_legacy(&dir, 1, b"a", b"1");
        write_legacy(&dir, 507, b"b", b"507");
        // A commit-domain durable batch must be left untouched.
        write_sst(
            dir.join("00000000000000000001-0000.sst"),
            [(b"a".as_slice(), b"d".as_slice())],
        )
        .unwrap();

        let migrated = migrate_legacy_flush_names(&dir).unwrap();
        assert_eq!(migrated, 2);

        assert!(dir.join("flush-00000000000000000000-0001.sst").exists());
        assert!(dir.join("flush-00000000000000000000-0507.sst").exists());
        assert!(!dir.join("00000000000000000001.sst").exists());
        assert!(!dir.join("00000000000000000507.sst").exists());
        // The durable batch is not a legacy flush and stays as-is.
        assert!(dir.join("00000000000000000001-0000.sst").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn migration_preserves_classified_order_key() {
        // The migrated flush (watermark 0) must sort strictly before the
        // durable batch at seq 1, so newest-wins hands the durable value out.
        let dir = scratch("order-key");
        write_legacy(&dir, 9, b"k", b"legacy");
        write_sst(
            dir.join("00000000000000000001-0000.sst"),
            [(b"k".as_slice(), b"durable".as_slice())],
        )
        .unwrap();
        migrate_legacy_flush_names(&dir).unwrap();

        let flush = super::super::sst_order_key(&dir.join("flush-00000000000000000000-0009.sst"))
            .unwrap()
            .unwrap();
        let durable = super::super::sst_order_key(&dir.join("00000000000000000001-0000.sst"))
            .unwrap()
            .unwrap();
        assert!(
            flush < durable,
            "migrated flush must sort older than durable batch"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_idempotent_and_noop_on_clean_dir() {
        let dir = scratch("idempotent");
        write_legacy(&dir, 3, b"k", b"v");
        assert_eq!(migrate_legacy_flush_names(&dir).unwrap(), 1);
        // Second run finds no legacy files.
        assert_eq!(migrate_legacy_flush_names(&dir).unwrap(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_is_noop() {
        let dir =
            std::env::temp_dir().join(format!("calyx-aster-migrate-absent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(migrate_legacy_flush_names(&dir).unwrap(), 0);
    }
}
