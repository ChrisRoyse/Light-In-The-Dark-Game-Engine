from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import datetime as dt
import json
import logging
import shutil
from pathlib import Path
from typing import Any

from .config import (
    BYTES_PER_GB,
    DEFAULT_USER_AGENT,
    DEFAULT_VAULT_ROOT,
    KAYKIT_REPO_ALLOWLIST,
    KENNEY_PACK_SLUGS,
    POLICIES,
)
from .models import Candidate, DownloadResult
from .restore import (
    build_targeted_candidates,
    default_floor_selector_bin,
    parse_targeted_list,
    run_floor_selector,
    select_floor_candidates,
    verify_restore_on_disk,
    write_catalog_manifest,
)
from .source_ambientcg import ambientcg_candidates
from .source_game_kits import kaykit_candidates, kenney_candidates, quaternius_candidates
from .source_polyhaven import polyhaven_candidates
from .storage import (
    append_new_manifest_records,
    dir_size,
    download_item,
    make_vault_state_summary,
    result_record,
    select_candidates,
    verify_zip_and_extract,
)
from .utils import setup_logging, source_policy, utc_now_iso

# Default backstop cap. A full-corpus pull (~25GB+) must be explicitly opted into
# with --full-corpus; the default stays far below the old 300 GiB (issue #1596).
FULL_CORPUS_BACKSTOP_GB = 8.0
FULL_CORPUS_CAP_GB = 300.0


def build_candidates(args: argparse.Namespace) -> tuple[list[Candidate], list[dict[str, Any]]]:
    candidates: list[Candidate] = []
    rejections: list[dict[str, Any]] = []
    sources = args.source or ["polyhaven", "ambientcg"]
    for name in sources:
        source_policy(name)
    if "polyhaven" in sources:
        source_candidates, source_rejections = polyhaven_candidates(args, args.user_agent)
        candidates.extend(source_candidates)
        rejections.extend(source_rejections)
    if "ambientcg" in sources:
        source_candidates, source_rejections = ambientcg_candidates(args, args.user_agent)
        candidates.extend(source_candidates)
        rejections.extend(source_rejections)
    if "kenney" in sources:
        source_candidates, source_rejections = kenney_candidates(args, args.user_agent)
        candidates.extend(source_candidates)
        rejections.extend(source_rejections)
    if "kaykit" in sources:
        source_candidates, source_rejections = kaykit_candidates(args, args.user_agent)
        candidates.extend(source_candidates)
        rejections.extend(source_rejections)
    if "quaternius" in sources:
        source_candidates, source_rejections = quaternius_candidates(args, args.user_agent)
        candidates.extend(source_candidates)
        rejections.extend(source_rejections)
    return candidates, rejections

def make_summary(
    *,
    args: argparse.Namespace,
    log_path: Path,
    selected: list[Candidate],
    selection_rejections: list[dict[str, Any]],
    source_rejections: list[dict[str, Any]],
    download_records: list[dict[str, Any]],
    extraction_records: list[dict[str, Any]],
    manifest_path: Path,
    extraction_manifest_path: Path,
) -> dict[str, Any]:
    vault_root = Path(args.vault_root)
    by_source: dict[str, int] = {}
    by_type: dict[str, int] = {}
    for record in download_records:
        by_source[record["source"]] = by_source.get(record["source"], 0) + 1
        by_type[record["asset_type"]] = by_type.get(record["asset_type"], 0) + 1
    return {
        "timestamp_utc": utc_now_iso(),
        "vault_root": str(vault_root),
        "source_of_truth": {
            "download_manifest": str(manifest_path),
            "extraction_manifest": str(extraction_manifest_path),
            "asset_files_root": str(vault_root / "sources"),
            "extracted_files_root": str(vault_root / "extracted"),
        },
        "log_path": str(log_path),
        "allowed_sources": {name: dataclasses.asdict(policy) for name, policy in sorted(POLICIES.items())},
        "requested_sources": args.source or ["polyhaven", "ambientcg"],
        "byte_cap": int(args.max_gb * BYTES_PER_GB),
        "selected_asset_count": len(selected),
        "selected_declared_bytes": sum(sum(item.declared_size for item in candidate.items) for candidate in selected),
        "download_record_count": len(download_records),
        "download_bytes_on_disk": sum(int(record["bytes_on_disk"]) for record in download_records),
        "extraction_record_count": len(extraction_records),
        "extraction_bytes_on_disk": sum(int(record["bytes_on_disk"]) for record in extraction_records),
        "by_source_download_records": by_source,
        "by_asset_type_download_records": by_type,
        "source_rejection_count": len(source_rejections),
        "selection_rejection_count": len(selection_rejections),
        "source_rejections_sample": source_rejections[:25],
        "selection_rejections_sample": selection_rejections[:25],
        "vault_sources_bytes_after_run": dir_size(vault_root / "sources"),
        "vault_extracted_bytes_after_run": dir_size(vault_root / "extracted"),
    }

def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vault-root", type=Path, default=DEFAULT_VAULT_ROOT)
    parser.add_argument("--source", action="append", choices=sorted(POLICIES), help="Allowed source to use; repeatable.")
    parser.add_argument(
        "--max-gb",
        type=float,
        default=FULL_CORPUS_BACKSTOP_GB,
        help="Maximum declared download bytes for this run (backstop cap).",
    )
    parser.add_argument(
        "--full-corpus",
        action="store_true",
        help="Explicit opt-in to a full-corpus pull (raises the default cap far above the backstop).",
    )
    parser.add_argument(
        "--targeted-list",
        type=Path,
        default=None,
        help="Path to a JSON list of 'source:asset_id' targets for a minimal-sufficient restore.",
    )
    parser.add_argument(
        "--floor",
        action="store_true",
        help="Floor-targeted restore: fetch cheapest-first until every editor role floor is met, "
        "using the Rust selector (litd-editor-asset-floor-select) as the single classifier.",
    )
    parser.add_argument(
        "--floor-role",
        action="append",
        metavar="ROLE=COUNT",
        help="Override the floor for one role (repeatable); replaces the default floor set with "
        "exactly the named roles. Validated against the Rust classifier's known roles.",
    )
    parser.add_argument(
        "--floor-selector-bin",
        type=Path,
        default=None,
        help="Path to the built litd-editor-asset-floor-select binary; auto-resolved from the "
        "repo target/ dir when omitted.",
    )
    parser.add_argument("--min-free-gb", type=float, default=50.0, help="Minimum free disk space to preserve.")
    parser.add_argument("--download-workers", type=int, default=8)
    parser.add_argument("--retries", type=int, default=3)
    parser.add_argument("--user-agent", default=DEFAULT_USER_AGENT)
    parser.add_argument("--poly-model-limit", type=int, default=80)
    parser.add_argument("--poly-texture-limit", type=int, default=160)
    parser.add_argument("--poly-hdri-limit", type=int, default=40)
    parser.add_argument("--poly-model-resolution", default="2k")
    parser.add_argument("--poly-surface-resolution", default="4k")
    parser.add_argument("--poly-hdri-resolution", default="4k")
    parser.add_argument("--ambient-material-limit", type=int, default=250)
    parser.add_argument("--ambient-terrain-limit", type=int, default=39)
    parser.add_argument("--ambient-hdri-limit", type=int, default=30)
    parser.add_argument("--ambient-model-limit", type=int, default=20)
    parser.add_argument("--ambient-material-resolution", default="4K-JPG")
    parser.add_argument("--ambient-terrain-resolution", default="4K-JPG")
    parser.add_argument("--ambient-terrain-obj-resolution", default="1K")
    parser.add_argument("--ambient-hdri-resolution", default="4K-JPG")
    parser.add_argument("--ambient-model-resolution", default="LQ-2K-JPG")
    parser.add_argument("--include-ambient-terrain-obj", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--kenney-pack-limit", type=int, default=len(KENNEY_PACK_SLUGS))
    parser.add_argument("--kaykit-repo-limit", type=int, default=len(KAYKIT_REPO_ALLOWLIST))
    parser.add_argument("--quaternius-pack-limit", type=int, default=None)
    parser.add_argument(
        "--quaternius-pack-slug",
        action="append",
        help="Official Quaternius pack slug to ingest; repeatable and mutually exclusive with pack limit.",
    )
    parser.add_argument("--quaternius-pack-workers", type=int, default=8)
    parser.add_argument("--quaternius-drive-depth", type=int, default=4)
    parser.add_argument("--drive-metadata-workers", type=int, default=12)
    parser.add_argument("--extract-zips", dest="extract_zips", action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument("--extract-ambient-zips", dest="extract_zips", action=argparse.BooleanOptionalAction)
    args = parser.parse_args(argv)
    if args.max_gb <= 0:
        parser.error("--max-gb must be greater than zero")
    if args.targeted_list is not None and args.full_corpus:
        parser.error("--targeted-list and --full-corpus are mutually exclusive")
    if args.floor and args.targeted_list is not None:
        parser.error("--floor and --targeted-list are mutually exclusive")
    if args.floor and args.full_corpus:
        parser.error("--floor and --full-corpus are mutually exclusive")
    if args.floor_role and not args.floor:
        parser.error("--floor-role requires --floor")
    if args.full_corpus and args.max_gb <= FULL_CORPUS_BACKSTOP_GB:
        # Opt-in raises the effective cap; an explicit larger --max-gb is respected as-is.
        args.max_gb = FULL_CORPUS_CAP_GB
    if (
        not args.full_corpus
        and args.targeted_list is None
        and not args.floor
        and args.max_gb > FULL_CORPUS_BACKSTOP_GB
    ):
        parser.error(
            f"--max-gb {args.max_gb} exceeds the {FULL_CORPUS_BACKSTOP_GB} GiB backstop; "
            "pass --full-corpus to opt into a large pull or use --targeted-list for a minimal restore"
        )
    if args.download_workers < 1:
        parser.error("--download-workers must be at least 1")
    if args.retries < 1:
        parser.error("--retries must be at least 1")
    if args.kaykit_repo_limit < 0:
        parser.error("--kaykit-repo-limit must be at least 0")
    if args.quaternius_pack_limit is not None and args.quaternius_pack_limit < 0:
        parser.error("--quaternius-pack-limit must be at least 0")
    if args.quaternius_pack_slug and args.quaternius_pack_limit is not None:
        parser.error("--quaternius-pack-slug cannot be combined with --quaternius-pack-limit")
    if args.quaternius_pack_workers < 1:
        parser.error("--quaternius-pack-workers must be at least 1")
    if args.quaternius_drive_depth < 0:
        parser.error("--quaternius-drive-depth must be at least 0")
    if args.drive_metadata_workers < 1:
        parser.error("--drive-metadata-workers must be at least 1")
    return args

def main(argv: list[str]) -> int:
    if argv and argv[0] == "prune":
        from .prune import run as prune_run

        return prune_run(argv[1:])
    args = parse_args(argv)
    vault_root: Path = args.vault_root
    vault_root.mkdir(parents=True, exist_ok=True)
    log_path = setup_logging(vault_root)
    manifest_dir = vault_root / "manifests"
    manifest_path = manifest_dir / "asset_intake_manifest.jsonl"
    extraction_manifest_path = manifest_dir / "asset_extraction_manifest.jsonl"
    summary_path = manifest_dir / "asset_intake_summary.json"
    vault_state_summary_path = manifest_dir / "asset_vault_state_summary.json"

    logging.info("vault_root=%s", vault_root)
    logging.info("download cap %.2f GiB", args.max_gb)
    usage = shutil.disk_usage(vault_root.anchor or vault_root)
    if usage.free < int(args.min_free_gb * BYTES_PER_GB):
        raise RuntimeError(
            f"Free disk below guard before intake: {usage.free} bytes free, "
            f"minimum is {int(args.min_free_gb * BYTES_PER_GB)}"
        )

    targeted = args.targeted_list is not None
    verify_on_disk = targeted or args.floor
    if targeted:
        targets = parse_targeted_list(args.targeted_list)
        logging.info("targeted restore: %s requested item(s) from %s", len(targets), args.targeted_list)
        selected = build_targeted_candidates(targets, args.user_agent)
        selection_rejections = []
        source_rejections = []
        if not selected:
            raise RuntimeError(f"Targeted restore produced no candidates from {args.targeted_list}")
    elif args.floor:
        candidates, source_rejections = build_candidates(args)
        if not candidates:
            raise RuntimeError("No candidates were produced from the allowed sources for floor restore")
        catalog_path = manifest_dir / "asset_floor_catalog.jsonl"
        write_catalog_manifest(candidates, catalog_path)
        repo_root = Path(__file__).resolve().parents[3]
        selector_bin = args.floor_selector_bin or default_floor_selector_bin(repo_root)
        selection = run_floor_selector(selector_bin, catalog_path, args.floor_role or [])
        selected = select_floor_candidates(candidates, selection)
        selection_rejections = []
        logging.info(
            "floor restore: selector chose %s of %s candidate(s), %s declared bytes; role_counts=%s floors=%s",
            selection.get("selected_count"),
            selection.get("catalog_entry_count"),
            selection.get("selected_declared_bytes"),
            selection.get("role_counts"),
            selection.get("floors"),
        )
    else:
        candidates, source_rejections = build_candidates(args)
        if not candidates:
            raise RuntimeError("No candidates were produced from the allowed sources")
        selected, selection_rejections = select_candidates(candidates, int(args.max_gb * BYTES_PER_GB))
        if not selected:
            raise RuntimeError("No assets selected; byte cap or filters rejected every candidate")

    selected_declared = sum(sum(item.declared_size for item in candidate.items) for candidate in selected)
    if selected_declared > usage.free - int(args.min_free_gb * BYTES_PER_GB):
        raise RuntimeError(
            f"Selected downloads require {selected_declared} bytes, which would violate "
            f"the {args.min_free_gb:.2f} GiB free-space guard"
        )

    items = [item for candidate in selected for item in candidate.items]
    logging.info("selected %s assets, %s files, %s declared bytes", len(selected), len(items), selected_declared)

    results: list[DownloadResult] = []
    download_records: list[dict[str, Any]] = []
    download_errors: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.download_workers) as executor:
        future_to_item = {
            executor.submit(download_item, vault_root, item, args.user_agent, args.retries): item for item in items
        }
        for future in concurrent.futures.as_completed(future_to_item):
            item = future_to_item[future]
            try:
                result = future.result()
            except Exception as exc:
                message = f"{item.bundle_key} -> {item.url}: {exc}"
                logging.error("download failed after retries: %s", message)
                download_errors.append(message)
                continue
            results.append(result)
            record = result_record(vault_root, result)
            download_records.append(record)
            append_new_manifest_records(manifest_path, [record])

    download_records = sorted(download_records, key=lambda record: record["relative_path"])
    logging.info("verified %s download records in %s", len(download_records), manifest_path)
    if download_errors:
        error_path = manifest_dir / f"asset_intake_errors_{dt.datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
        error_path.write_text(json.dumps({"timestamp_utc": utc_now_iso(), "errors": download_errors}, indent=2), encoding="utf-8")
        raise RuntimeError(f"{len(download_errors)} downloads failed; details written to {error_path}")

    if verify_on_disk:
        mode = "floor" if args.floor else "targeted"
        total_items = sum(len(candidate.items) for candidate in selected)
        verified = verify_restore_on_disk(vault_root, selected, download_records)
        logging.info(
            "%s restore complete: %s candidate(s), %s of %s file(s) verified on disk",
            mode,
            len(selected),
            verified,
            total_items,
        )

    extraction_records: list[dict[str, Any]] = []
    if args.extract_zips:
        for item in sorted((result.item for result in results if result.item.extraction_key), key=lambda entry: entry.bundle_key):
            extraction_records.extend(verify_zip_and_extract(vault_root, item))
        appended_extractions = append_new_manifest_records(extraction_manifest_path, extraction_records)
        logging.info("wrote %s new extraction manifest records to %s", appended_extractions, extraction_manifest_path)

    summary = make_summary(
        args=args,
        log_path=log_path,
        selected=selected,
        selection_rejections=selection_rejections,
        source_rejections=source_rejections,
        download_records=download_records,
        extraction_records=extraction_records,
        manifest_path=manifest_path,
        extraction_manifest_path=extraction_manifest_path,
    )
    manifest_dir.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    vault_state_summary = make_vault_state_summary(
        vault_root=vault_root,
        manifest_path=manifest_path,
        extraction_manifest_path=extraction_manifest_path,
        latest_run_summary_path=summary_path,
    )
    vault_state_summary_path.write_text(json.dumps(vault_state_summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    logging.info("summary written to %s", summary_path)
    logging.info("vault state summary written to %s", vault_state_summary_path)
    logging.info(
        "complete: %s download records, %.3f GiB downloaded/verified",
        len(download_records),
        summary["download_bytes_on_disk"] / BYTES_PER_GB,
    )
    return 0
