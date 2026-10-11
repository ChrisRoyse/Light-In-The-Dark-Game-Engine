from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))

from asset_intake.config import DEFAULT_VAULT_ROOT
from asset_intake.storage import assert_under, make_vault_state_summary, sha256_file
from asset_intake.utils import utc_now_iso

ALIASES: tuple[tuple[str, str], ...] = (
    ("ultimate-modular-ruins", "ultimatemodularruins"),
    ("modular-dungeon", "medievaldungeon"),
)


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as handle:
        return [json.loads(line) for line in handle if line.strip()]


def write_jsonl_atomic(path: Path, records: list[dict[str, Any]]) -> None:
    temp_path = path.with_name(f"{path.name}.repairing")
    with temp_path.open("w", encoding="utf-8", newline="\n") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
    temp_path.replace(path)


def suffix_for(record: dict[str, Any], slug: str) -> str:
    prefix = f"sources/quaternius/game-kit/{slug}/"
    relative_path = str(record["relative_path"])
    if not relative_path.startswith(prefix):
        raise RuntimeError(f"Record path for {slug} does not match canonical prefix: {relative_path}")
    return relative_path[len(prefix):]


def records_for_slug(records: list[dict[str, Any]], slug: str) -> list[dict[str, Any]]:
    return [
        record
        for record in records
        if record.get("record_type") == "download"
        and record.get("source") == "quaternius"
        and record.get("asset_type") == "game-kit"
        and record.get("asset_id") == slug
    ]


def verify_record_file(vault_root: Path, record: dict[str, Any]) -> None:
    path = vault_root / str(record["relative_path"])
    assert_under(vault_root, path)
    if not path.exists():
        raise RuntimeError(f"Manifest record file is missing: {path}")
    expected_size = int(record["bytes_on_disk"])
    actual_size = path.stat().st_size
    if actual_size != expected_size:
        raise RuntimeError(f"Size mismatch for {path}: expected {expected_size}, got {actual_size}")
    expected_sha = str(record["sha256"])
    actual_sha = sha256_file(path)
    if actual_sha != expected_sha:
        raise RuntimeError(f"SHA-256 mismatch for {path}: expected {expected_sha}, got {actual_sha}")


def verify_pair(vault_root: Path, records: list[dict[str, Any]], old_slug: str, canonical_slug: str) -> dict[str, Any]:
    old_records = records_for_slug(records, old_slug)
    canonical_records = records_for_slug(records, canonical_slug)
    if not old_records:
        raise RuntimeError(f"No old alias manifest records found for {old_slug}")
    if not canonical_records:
        raise RuntimeError(f"No canonical manifest records found for {canonical_slug}")

    for record in old_records + canonical_records:
        verify_record_file(vault_root, record)

    old_by_suffix = {
        suffix_for(record, old_slug): (int(record["bytes_on_disk"]), str(record["sha256"]))
        for record in old_records
    }
    canonical_by_suffix = {
        suffix_for(record, canonical_slug): (int(record["bytes_on_disk"]), str(record["sha256"]))
        for record in canonical_records
    }
    missing_in_canonical = sorted(set(old_by_suffix) - set(canonical_by_suffix))
    missing_in_old = sorted(set(canonical_by_suffix) - set(old_by_suffix))
    if missing_in_canonical or missing_in_old:
        raise RuntimeError(
            f"Alias suffix mismatch {old_slug}->{canonical_slug}: "
            f"missing_in_canonical={missing_in_canonical[:10]} missing_in_old={missing_in_old[:10]}"
        )
    mismatches = [
        suffix
        for suffix, old_value in old_by_suffix.items()
        if old_value != canonical_by_suffix[suffix]
    ]
    if mismatches:
        raise RuntimeError(f"Alias hash/size mismatch {old_slug}->{canonical_slug}: {mismatches[:10]}")

    alias_dir = vault_root / "sources" / "quaternius" / "game-kit" / old_slug
    canonical_dir = vault_root / "sources" / "quaternius" / "game-kit" / canonical_slug
    for directory in (alias_dir, canonical_dir):
        assert_under(vault_root / "sources" / "quaternius" / "game-kit", directory)
        if not directory.is_dir():
            raise RuntimeError(f"Expected directory missing: {directory}")

    return {
        "old_slug": old_slug,
        "canonical_slug": canonical_slug,
        "record_count": len(old_records),
        "bytes": sum(int(record["bytes_on_disk"]) for record in old_records),
        "alias_dir": str(alias_dir),
        "canonical_dir": str(canonical_dir),
    }


def build_report(vault_root: Path, records: list[dict[str, Any]]) -> dict[str, Any]:
    pair_reports = [verify_pair(vault_root, records, old, canonical) for old, canonical in ALIASES]
    old_slugs = {old for old, _ in ALIASES}
    records_after = [
        record
        for record in records
        if not (
            record.get("record_type") == "download"
            and record.get("source") == "quaternius"
            and record.get("asset_type") == "game-kit"
            and record.get("asset_id") in old_slugs
        )
    ]
    removed_records = len(records) - len(records_after)
    removed_bytes = sum(report["bytes"] for report in pair_reports)
    return {
        "timestamp_utc": utc_now_iso(),
        "vault_root": str(vault_root),
        "canonical_policy": "Quaternius official page slug from quaternius.com/packs/<slug>.html",
        "pairs": pair_reports,
        "manifest_records_before": len(records),
        "manifest_records_after": len(records_after),
        "removed_manifest_records": removed_records,
        "removed_source_bytes": removed_bytes,
    }


def apply_repair(vault_root: Path, manifest_path: Path, records: list[dict[str, Any]], report: dict[str, Any]) -> None:
    old_slugs = {old for old, _ in ALIASES}
    records_after = [
        record
        for record in records
        if not (
            record.get("record_type") == "download"
            and record.get("source") == "quaternius"
            and record.get("asset_type") == "game-kit"
            and record.get("asset_id") in old_slugs
        )
    ]
    backup_path = manifest_path.with_name(f"{manifest_path.name}.backup-{utc_now_iso().replace(':', '').replace('-', '')}")
    shutil.copy2(manifest_path, backup_path)
    write_jsonl_atomic(manifest_path, records_after)

    gamekit_root = vault_root / "sources" / "quaternius" / "game-kit"
    for old_slug in old_slugs:
        alias_dir = gamekit_root / old_slug
        assert_under(gamekit_root, alias_dir)
        if alias_dir.name != old_slug or not alias_dir.is_dir():
            raise RuntimeError(f"Refusing to remove unexpected alias directory: {alias_dir}")
        shutil.rmtree(alias_dir)

    manifest_dir = vault_root / "manifests"
    state_summary = make_vault_state_summary(
        vault_root=vault_root,
        manifest_path=manifest_path,
        extraction_manifest_path=manifest_dir / "asset_extraction_manifest.jsonl",
        latest_run_summary_path=manifest_dir / "asset_intake_summary.json",
    )
    (manifest_dir / "asset_vault_state_summary.json").write_text(
        json.dumps(state_summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    report["manifest_backup"] = str(backup_path)
    report["applied"] = True
    report["state_summary_after"] = state_summary


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Repair duplicate historical Quaternius slug aliases.")
    parser.add_argument("--vault-root", type=Path, default=DEFAULT_VAULT_ROOT)
    parser.add_argument("--apply", action="store_true", help="Apply the repair after equivalence validation.")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    try:
        args = parse_args(argv)
        vault_root = Path(args.vault_root)
        manifest_path = vault_root / "manifests" / "asset_intake_manifest.jsonl"
        records = read_jsonl(manifest_path)
        report = build_report(vault_root, records)
        report["applied"] = False
        if args.apply:
            apply_repair(vault_root, manifest_path, records, report)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0
    except Exception as exc:
        print(
            json.dumps(
                {
                    "status": "error",
                    "error_type": type(exc).__name__,
                    "error": str(exc),
                },
                indent=2,
                sort_keys=True,
            ),
            file=sys.stderr,
        )
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
