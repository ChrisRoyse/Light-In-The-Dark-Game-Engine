"""Fail-loud, atomic PRUNE for the CC0 asset vault (issue #1821).

The intake pipeline can only add assets. Removing an asset by hand leaves the
manifest records dangling, and ``AssetVaultCatalog`` fail-louds on any record
whose file is missing -- so a hand-delete breaks every catalog load until the
JSONL is repaired by hand (this is exactly what #1811 had to do outside the
tool). This module makes prune a first-class, safe operation.

Guarantees (all enforced BEFORE any mutation, so a rejected prune leaves the
vault byte-for-byte unchanged):

* Junction-safe by construction. Every target path is resolved and must land
  INSIDE the resolved vault root; any reparse point (junction/symlink) on the
  path from the vault root down to the target is refused. We only ever
  ``os.unlink`` individually named files -- never a recursive/mirroring delete
  that could purge THROUGH a junction (the #1569 vault-wipe class).
* Load-bearing fixtures are protected: Stan.gltf / Leela.gltf / gate.glb /
  wall.glb / Rock1.fbx (referenced by editor tests + FSV harnesses) can never
  be pruned.
* Atomic manifest/file consistency. Manifest records are rewritten FIRST (via a
  temp file + atomic ``os.replace``) to drop exactly the targeted (and any
  pre-existing dangling) records, THEN the files are deleted. So the catalog
  never observes a dangling record even on a partial failure -- a file that
  cannot be deleted is left as a benign orphan (no record) that a re-run cleans,
  reported LOUD. Nothing is dropped from a manifest whose file we are not also
  deleting.
* Vault tripwire (the #1569 junction-purge signature): the count of TOP-LEVEL
  vault entries is captured before and after and MUST be unchanged. A drop means
  a junction was purged through -- we abort LOUD.
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import stat as stat_mod
from pathlib import Path
from typing import Any

from .storage import _filesystem_path, _path_exists, make_vault_state_summary, read_jsonl_records
from .utils import setup_logging

# Basenames that ~60 editor tests + FSV harnesses load directly. Never prunable.
LOAD_BEARING_FIXTURES = frozenset(
    name.lower() for name in ("Stan.gltf", "Leela.gltf", "gate.glb", "wall.glb", "Rock1.fbx")
)
_FILE_ATTRIBUTE_REPARSE_POINT = 0x400
_MANIFEST_NAMES = ("asset_intake_manifest.jsonl", "asset_extraction_manifest.jsonl")


def _is_reparse_point(path: Path) -> bool:
    try:
        st = os.lstat(_filesystem_path(path))
    except OSError:
        return False
    attrs = getattr(st, "st_file_attributes", 0)
    return bool(attrs & _FILE_ATTRIBUTE_REPARSE_POINT) or stat_mod.S_ISLNK(st.st_mode)


def _real(path: Path) -> Path:
    return Path(os.path.realpath(_filesystem_path(path)))


def assert_prunable_within_vault(vault_root: Path, target: Path) -> Path:
    """Return the resolved absolute target, or raise LOUD if it is unsafe to prune.

    Rejects (a) a target that resolves outside the vault, (b) any reparse point
    (junction/symlink) on the chain from the vault root down to the target, and
    (c) a load-bearing test fixture. This is the junction-safety gate: we refuse
    to operate the moment a path could reach outside the vault.
    """
    absolute = target if target.is_absolute() else (vault_root / target)
    vault_real = _real(vault_root)
    target_real = _real(absolute)
    try:
        rel = target_real.relative_to(vault_real)
    except ValueError as exc:
        raise ValueError(
            f"Refusing to prune {target}: resolves to {target_real} which is OUTSIDE the vault {vault_real}"
        ) from exc
    if not rel.parts:
        raise ValueError(f"Refusing to prune the vault root itself: {target}")
    if absolute.name.lower() in LOAD_BEARING_FIXTURES:
        raise ValueError(
            f"Refusing to prune load-bearing test fixture {absolute.name} ({target}); "
            f"it is referenced by editor tests + FSV harnesses (#1821)"
        )
    probe = vault_real
    for part in rel.parts:
        probe = probe / part
        if _is_reparse_point(probe):
            raise ValueError(
                f"Refusing to prune {target}: reparse point (junction/symlink) at {probe} -- "
                f"pruning through a junction can purge outside the vault (#1569)"
            )
    return absolute


def top_level_count(vault_root: Path) -> int:
    """Count of direct children of the vault root -- the #1569 junction tripwire."""
    return sum(1 for _ in os.scandir(_filesystem_path(vault_root)))


def _rel_posix(vault_root: Path, absolute: Path) -> str:
    return absolute.relative_to(vault_root).as_posix()


def _atomic_write_jsonl(path: Path, records: list[dict[str, Any]]) -> None:
    tmp = path.with_name(path.name + ".prune-tmp")
    with open(_filesystem_path(tmp), "w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(_filesystem_path(tmp), _filesystem_path(path))


def _partition_records(
    vault_root: Path, records: list[dict[str, Any]], drop_rels: set[str], prune_dangling: bool
) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    """Split records into (keep, drop). A record is dropped if its file is a prune
    target, or (when prune_dangling) its file is already missing. A dangling record
    for a load-bearing fixture is a LOUD error, never silently dropped.
    """
    keep: list[dict[str, Any]] = []
    drop: list[dict[str, Any]] = []
    for record in records:
        rel = str(record.get("relative_path", ""))
        if rel in drop_rels:
            drop.append(record)
            continue
        if prune_dangling and not _path_exists(vault_root / rel):
            if Path(rel).name.lower() in LOAD_BEARING_FIXTURES:
                raise ValueError(
                    f"Load-bearing fixture {rel} has a dangling manifest record (file missing); "
                    f"restore the file rather than pruning the record (#1821)"
                )
            drop.append(record)
            continue
        keep.append(record)
    return keep, drop


def purge_stale_receipts(vault_root: Path) -> int:
    """Remove now-stale verify-cache receipts (their manifest-bytes key is dead).

    A cold catalog load rebuilds the receipt. Only unlinks ``*.receipt`` /
    ``*.receipt.tmp`` inside ``<vault>/.litd-verify-cache`` -- never recurses.
    """
    cache = vault_root / ".litd-verify-cache"
    if not _path_exists(cache) or _is_reparse_point(cache):
        return 0
    removed = 0
    for entry in os.scandir(_filesystem_path(cache)):
        if entry.is_file() and (entry.name.endswith(".receipt") or entry.name.endswith(".receipt.tmp")):
            os.unlink(entry.path)
            removed += 1
    return removed


def build_plan(vault_root: Path, paths: list[str], prune_dangling: bool) -> dict[str, Any]:
    """Validate every target and compute the drop-sets. Pure -- no mutation."""
    targets: list[Path] = []
    for raw in paths:
        absolute = assert_prunable_within_vault(vault_root, Path(raw))
        if not _path_exists(absolute):
            raise ValueError(f"Refusing to prune a path that does not exist: {absolute}")
        if _is_reparse_point(absolute):
            raise ValueError(f"Refusing to prune reparse point (junction/symlink): {absolute}")
        if not os.path.isfile(_filesystem_path(absolute)):
            raise ValueError(f"Refusing to prune a non-file (directory prune is unsupported): {absolute}")
        targets.append(absolute)
    if not targets and not prune_dangling:
        raise ValueError("Nothing to prune: pass at least one --path or --prune-dangling")

    drop_rels = {_rel_posix(vault_root, t) for t in targets}
    manifest_dir = vault_root / "manifests"
    manifests: dict[str, dict[str, Any]] = {}
    for name in _MANIFEST_NAMES:
        path = manifest_dir / name
        records = read_jsonl_records(path)
        keep, drop = _partition_records(vault_root, records, drop_rels, prune_dangling)
        manifests[name] = {"path": path, "records": records, "keep": keep, "drop": drop}

    # Guard: every explicit target file must have at least one manifest record, else the
    # user is deleting something the vault does not track -- refuse rather than orphan it.
    tracked = {
        str(r.get("relative_path", ""))
        for m in manifests.values()
        for r in m["records"]
    }
    untracked = sorted(rel for rel in drop_rels if rel not in tracked)
    if untracked:
        raise ValueError(
            f"Refusing to prune file(s) with no manifest record (untracked by the vault): {untracked}"
        )
    return {"targets": targets, "drop_rels": drop_rels, "manifests": manifests}


def apply_prune(vault_root: Path, plan: dict[str, Any]) -> dict[str, Any]:
    """Execute the validated plan. Manifests first (atomic), then file deletes."""
    pre_top = top_level_count(vault_root)

    # 1. Rewrite manifests to the keep-set (atomic per file). Dropping records BEFORE
    #    deleting files means the catalog never sees a dangling record.
    for name, info in plan["manifests"].items():
        if len(info["drop"]) > 0:
            _atomic_write_jsonl(info["path"], info["keep"])
            logging.info(
                "PRUNE manifest %s: kept %s, dropped %s record(s)",
                name,
                len(info["keep"]),
                len(info["drop"]),
            )

    # 2. Delete the target files individually (never recursive).
    deleted: list[str] = []
    failed: list[str] = []
    for target in plan["targets"]:
        try:
            os.unlink(_filesystem_path(target))
            deleted.append(_rel_posix(vault_root, target))
        except OSError as exc:
            failed.append(f"{target}: {exc}")

    receipts_removed = purge_stale_receipts(vault_root)

    # 3. Regenerate the state summary from the rewritten manifests.
    manifest_dir = vault_root / "manifests"
    summary = make_vault_state_summary(
        vault_root=vault_root,
        manifest_path=manifest_dir / "asset_intake_manifest.jsonl",
        extraction_manifest_path=manifest_dir / "asset_extraction_manifest.jsonl",
        latest_run_summary_path=manifest_dir / "asset_intake_summary.json",
    )
    summary_path = manifest_dir / "asset_vault_state_summary.json"
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    # 4. Tripwire: top-level entries must be unchanged (a drop == junction purge).
    post_top = top_level_count(vault_root)
    if post_top != pre_top:
        raise RuntimeError(
            f"VAULT TRIPWIRE FAILED: top-level entry count changed {pre_top} -> {post_top}; "
            f"a junction may have been purged through (#1569). Investigate before trusting the vault."
        )

    # 5. Post-condition: no kept record may dangle.
    dangling: list[str] = []
    for info in plan["manifests"].values():
        for record in info["keep"]:
            rel = str(record.get("relative_path", ""))
            if not _path_exists(vault_root / rel):
                dangling.append(rel)
    if dangling:
        raise RuntimeError(f"PRUNE left dangling records (file missing): {sorted(set(dangling))}")

    if failed:
        raise RuntimeError(
            "PRUNE could not delete some files (their records were already dropped, leaving benign "
            f"orphan files -- re-run to clean): {failed}"
        )

    return {
        "pre_top_level_count": pre_top,
        "post_top_level_count": post_top,
        "files_deleted": deleted,
        "receipts_removed": receipts_removed,
        "download_records_after": len(plan["manifests"]["asset_intake_manifest.jsonl"]["keep"]),
        "extraction_records_after": len(plan["manifests"]["asset_extraction_manifest.jsonl"]["keep"]),
        "download_records_dropped": len(plan["manifests"]["asset_intake_manifest.jsonl"]["drop"]),
        "extraction_records_dropped": len(plan["manifests"]["asset_extraction_manifest.jsonl"]["drop"]),
        "state_summary_path": str(summary_path),
    }


def parse_prune_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="asset_intake prune", description=__doc__)
    parser.add_argument("--vault-root", type=Path, required=True)
    parser.add_argument(
        "--path",
        action="append",
        default=[],
        help="Vault-relative (or in-vault absolute) file to prune; repeatable.",
    )
    parser.add_argument(
        "--prune-dangling",
        action="store_true",
        help="Also drop any manifest record whose file is already missing (cleanup).",
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Actually delete + rewrite. Without it, prune is a dry run (default).",
    )
    return parser.parse_args(argv)


def run(argv: list[str]) -> int:
    args = parse_prune_args(argv)
    vault_root: Path = args.vault_root
    if not vault_root.exists():
        raise RuntimeError(f"Vault root does not exist: {vault_root}")
    setup_logging(vault_root)
    plan = build_plan(vault_root, args.path, args.prune_dangling)
    drop_counts = {name: len(info["drop"]) for name, info in plan["manifests"].items()}
    logging.info(
        "PRUNE plan: %s file(s), manifest drops=%s, apply=%s",
        len(plan["targets"]),
        drop_counts,
        args.apply,
    )
    if not args.apply:
        print(
            json.dumps(
                {
                    "mode": "dry-run",
                    "vault_root": str(vault_root),
                    "files_to_delete": [_rel_posix(vault_root, t) for t in plan["targets"]],
                    "manifest_record_drops": drop_counts,
                },
                indent=2,
                sort_keys=True,
            )
        )
        return 0
    result = apply_prune(vault_root, plan)
    print(json.dumps({"mode": "applied", "vault_root": str(vault_root), **result}, indent=2, sort_keys=True))
    logging.info("PRUNE complete: %s", result)
    return 0
