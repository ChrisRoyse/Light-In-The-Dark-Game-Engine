#!/usr/bin/env python3
"""Ingest a sanctioned Quaternius archive acquired through browser/Itch/source-key flow."""

from __future__ import annotations

import argparse
from html.parser import HTMLParser
import json
import re
import shutil
import sys
import urllib.parse
import zipfile
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from asset_intake.config import DEFAULT_USER_AGENT, DEFAULT_VAULT_ROOT
from asset_intake.models import DownloadItem, DownloadResult
from asset_intake.storage import (
    append_new_manifest_records,
    assert_under,
    make_vault_state_summary,
    result_record,
    sha256_file,
    verify_zip_and_extract,
)
from asset_intake.utils import sanitize_component, utc_now_iso
from asset_intake.net import fetch_text
from quaternius_discovery import QUATERNIUS_BASE_URL, has_license_marker, pack_title

TARGET_SLUGS = {
    "3dcardkitfantasy",
    "downtowncitymegakit",
    "fantasypropsmegakit",
    "medievalvillagemegakit",
    "modularcharacteroutfitsfantasy",
    "modularscifimegakit",
    "scifiessentialskit",
    "stylizednaturemegakit",
    "universalanimationlibrary",
    "universalanimationlibrary2",
    "universalbasecharacters",
}

ASSET_EXTENSIONS = {".blend", ".fbx", ".glb", ".gltf", ".obj"}
IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".webp"}
URL_RE = re.compile(r"""https?://[^'"<>\s)]+""", re.IGNORECASE)


class AttributeUrlCollector(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.urls: set[str] = set()

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        for _, value in attrs:
            if not value or "itch.io" not in value.lower():
                continue
            self.urls.update(URL_RE.findall(value))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vault-root", type=Path, default=DEFAULT_VAULT_ROOT)
    parser.add_argument("--slug", required=True, help="One of the #1252 Quaternius target slugs.")
    parser.add_argument("--archive", type=Path, required=True, help="Local zip downloaded through sanctioned flow.")
    parser.add_argument("--expected-filename", help="Fail if the local archive filename differs.")
    parser.add_argument("--expected-size", type=int, help="Fail if archive byte length differs.")
    parser.add_argument("--expected-sha256", help="Fail if archive SHA-256 differs.")
    parser.add_argument("--user-agent", default=DEFAULT_USER_AGENT)
    parser.add_argument("--extract-zips", action=argparse.BooleanOptionalAction, default=True)
    args = parser.parse_args()
    args.slug = sanitize_component(args.slug.lower())
    if args.slug not in TARGET_SLUGS:
        parser.error(f"--slug must be one of: {', '.join(sorted(TARGET_SLUGS))}")
    if args.expected_size is not None and args.expected_size <= 0:
        parser.error("--expected-size must be positive")
    if args.expected_sha256 and not re.fullmatch(r"[0-9a-fA-F]{64}", args.expected_sha256):
        parser.error("--expected-sha256 must be 64 hex characters")
    return args


def official_page_url(slug: str) -> str:
    return urllib.parse.urljoin(QUATERNIUS_BASE_URL, f"packs/{slug}.html")


def itch_url(page_url: str, page_html: str) -> str:
    collector = AttributeUrlCollector()
    collector.feed(page_html)
    links = {urllib.parse.urljoin(page_url, link.replace("&amp;", "&")) for link in collector.urls}
    product_links = sorted(
        link
        for link in links
        if urllib.parse.urlparse(link).netloc.lower() == "quaternius.itch.io"
    )
    if not product_links:
        raise RuntimeError(f"No official quaternius.itch.io product link found on {page_url}")
    return product_links[0]


def validate_archive(path: Path) -> dict[str, int]:
    if not path.exists():
        raise FileNotFoundError(f"Archive does not exist: {path}")
    if not path.is_file():
        raise RuntimeError(f"Archive path is not a file: {path}")
    if path.suffix.lower() != ".zip":
        raise RuntimeError(f"Archive must be a .zip file: {path}")
    asset_count = 0
    image_count = 0
    license_count = 0
    with zipfile.ZipFile(path) as archive:
        bad_member = archive.testzip()
        if bad_member is not None:
            raise RuntimeError(f"Zip CRC failed for {path}: {bad_member}")
        file_count = 0
        for member in archive.infolist():
            if member.is_dir():
                continue
            file_count += 1
            suffix = Path(member.filename).suffix.lower()
            asset_count += int(suffix in ASSET_EXTENSIONS)
            image_count += int(suffix in IMAGE_EXTENSIONS)
            license_count += int("license" in Path(member.filename).name.lower())
    if file_count == 0:
        raise RuntimeError(f"Archive has no files: {path}")
    if asset_count == 0:
        raise RuntimeError(f"Archive has no recognized asset files: {path}")
    if image_count == 0:
        raise RuntimeError(f"Archive has no preview/texture image files: {path}")
    return {"file_count": file_count, "asset_file_count": asset_count, "image_file_count": image_count, "license_file_count": license_count}


def verify_expectations(args: argparse.Namespace, archive_size: int, archive_sha: str) -> None:
    if args.expected_filename and args.archive.name != args.expected_filename:
        raise RuntimeError(f"Filename mismatch: expected {args.expected_filename}, got {args.archive.name}")
    if args.expected_size is not None and archive_size != args.expected_size:
        raise RuntimeError(f"Size mismatch for {args.archive}: expected {args.expected_size}, got {archive_size}")
    if args.expected_sha256 and archive_sha.lower() != args.expected_sha256.lower():
        raise RuntimeError(f"SHA-256 mismatch for {args.archive}: expected {args.expected_sha256}, got {archive_sha}")


def copy_verified_archive(vault_root: Path, source: Path, destination: Path, expected_size: int, expected_sha: str) -> str:
    assert_under(vault_root, destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists():
        if destination.stat().st_size != expected_size:
            raise RuntimeError(f"Existing destination size mismatch: {destination}")
        actual_sha = sha256_file(destination)
        if actual_sha != expected_sha:
            raise RuntimeError(f"Existing destination SHA-256 mismatch for {destination}: {actual_sha}")
        return "existing_verified"
    part = destination.with_name(destination.name + ".part")
    if part.exists():
        raise RuntimeError(f"Partial ingest exists and must be inspected manually: {part}")
    with source.open("rb") as src, part.open("wb") as dst:
        shutil.copyfileobj(src, dst)
    if part.stat().st_size != expected_size:
        part.unlink(missing_ok=True)
        raise RuntimeError(f"Copied size mismatch for {destination}")
    actual_sha = sha256_file(part)
    if actual_sha != expected_sha:
        part.unlink(missing_ok=True)
        raise RuntimeError(f"Copied SHA-256 mismatch for {destination}: {actual_sha}")
    part.replace(destination)
    return "copied_from_manual_acquisition"


def main() -> int:
    args = parse_args()
    vault_root = args.vault_root
    manifest_dir = vault_root / "manifests"
    manifest_path = manifest_dir / "asset_intake_manifest.jsonl"
    extraction_manifest_path = manifest_dir / "asset_extraction_manifest.jsonl"
    summary_path = manifest_dir / "asset_manual_quaternius_summary.json"
    vault_state_summary_path = manifest_dir / "asset_vault_state_summary.json"

    page_url = official_page_url(args.slug)
    page_html = fetch_text(page_url, args.user_agent)
    if not has_license_marker(page_html):
        raise RuntimeError(f"Official page lacks CC0/free-commercial-use marker: {page_url}")
    title = pack_title(page_html, page_url)
    artifact_url = itch_url(page_url, page_html)

    archive = args.archive.resolve()
    archive_stats = validate_archive(archive)
    archive_size = archive.stat().st_size
    archive_sha = sha256_file(archive)
    verify_expectations(args, archive_size, archive_sha)

    dest_name = sanitize_component(archive.name)
    archive_stem = sanitize_component(archive.stem)
    destination = Path("sources", "quaternius", "manual-game-kit", args.slug, dest_name)
    status = copy_verified_archive(vault_root, archive, vault_root / destination, archive_size, archive_sha)
    item = DownloadItem(
        source="quaternius",
        asset_id=args.slug,
        asset_type="game-kit",
        title=title,
        source_url=page_url,
        license_name="CC0-1.0",
        license_url=page_url,
        api_url=artifact_url,
        url=f"manual-file://{archive.as_posix()}",
        destination=destination,
        declared_size=archive_size,
        md5=None,
        bundle_key=f"quaternius:manual-game-kit:{args.slug}:{dest_name}",
        extraction_key=str(Path("extracted", "quaternius", "manual-game-kit", args.slug, archive_stem)),
    )
    result = DownloadResult(item=item, status=status, bytes_on_disk=archive_size, sha256=archive_sha, md5_actual=None)
    record = result_record(vault_root, result)
    record.update(
        {
            "acquisition_method": "manual_sanctioned_browser_or_source_key",
            "official_page_url": page_url,
            "artifact_url": artifact_url,
            "artifact_filename": archive.name,
            "source_local_path": str(archive),
            "archive_file_count": archive_stats["file_count"],
            "archive_asset_file_count": archive_stats["asset_file_count"],
            "archive_image_file_count": archive_stats["image_file_count"],
            "archive_license_file_count": archive_stats["license_file_count"],
            "license_marker_text": "official Quaternius page contains CC0/free commercial use marker",
            "manual_acquisition_timestamp_utc": utc_now_iso(),
        }
    )
    extraction_records: list[dict[str, object]] = []
    try:
        if args.extract_zips:
            extraction_records = verify_zip_and_extract(vault_root, item)
            append_new_manifest_records(extraction_manifest_path, extraction_records)
    except Exception:
        if status == "copied_from_manual_acquisition":
            (vault_root / destination).unlink(missing_ok=True)
        raise
    appended_downloads = append_new_manifest_records(manifest_path, [record])
    summary = {
        "timestamp_utc": utc_now_iso(),
        "slug": args.slug,
        "title": title,
        "official_page_url": page_url,
        "artifact_url": artifact_url,
        "archive": str(archive),
        "vault_destination": str(vault_root / destination),
        "sha256": archive_sha,
        "bytes_on_disk": archive_size,
        "status": status,
        "download_manifest_records_appended": appended_downloads,
        "extraction_record_count": len(extraction_records),
        **archive_stats,
    }
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    vault_state = make_vault_state_summary(
        vault_root=vault_root,
        manifest_path=manifest_path,
        extraction_manifest_path=extraction_manifest_path,
        latest_run_summary_path=summary_path,
    )
    vault_state_summary_path.write_text(json.dumps(vault_state, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
