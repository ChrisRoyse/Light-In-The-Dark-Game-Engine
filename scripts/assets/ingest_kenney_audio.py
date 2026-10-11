#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import shutil
import urllib.request
from dataclasses import dataclass
from pathlib import Path

from asset_intake.models import DownloadItem
from asset_intake.storage import (
    append_new_manifest_records,
    assert_under,
    download_item,
    result_record,
    verify_zip_and_extract,
)
from asset_intake.utils import utc_now_iso


KENNEY_POLICY = {
    "source": "kenney",
    "asset_type": "audio",
    "license_name": "CC0-1.0",
    "license_url": "https://creativecommons.org/publicdomain/zero/1.0/",
    "api_url": "https://www.kenney.nl/assets/category:Audio",
}


@dataclass(frozen=True)
class AudioPack:
    slug: str
    title: str
    page_url: str


PACKS = [
    AudioPack("rpg-audio", "RPG Audio", "https://kenney.nl/assets/rpg-audio"),
    AudioPack("interface-sounds", "Interface Sounds", "https://kenney.nl/assets/interface-sounds"),
    AudioPack("ui-audio", "UI Audio", "https://kenney.nl/assets/ui-audio"),
]


def main() -> int:
    parser = argparse.ArgumentParser(description="Ingest official Kenney CC0 audio packs into the local asset vault.")
    parser.add_argument("--vault-root", default="local-asset-vault", type=Path)
    parser.add_argument("--replace-existing-extractions", action="store_true")
    args = parser.parse_args()

    vault_root = args.vault_root.resolve()
    manifest_dir = vault_root / "manifests"
    download_manifest = manifest_dir / "asset_intake_manifest.jsonl"
    extraction_manifest = manifest_dir / "asset_extraction_manifest.jsonl"
    summary_path = manifest_dir / "kenney_audio_summary.json"

    download_records: list[dict] = []
    extraction_records: list[dict] = []
    pack_summaries = []
    for pack in PACKS:
        item = build_download_item(vault_root, pack)
        extract_dir = vault_root / item.extraction_key
        if args.replace_existing_extractions and extract_dir.exists() and not (extract_dir / "_extraction_manifest.jsonl").exists():
            assert_under(vault_root, extract_dir)
            shutil.rmtree(extract_dir)
        result = download_item(vault_root, item, "LightInTheDarkGameEditor/kenney-audio-intake", retries=3)
        extracted = verify_zip_and_extract(vault_root, item)
        verify_downloaded_license(vault_root, item, extracted)
        download_records.append(result_record(vault_root, result))
        extraction_records.extend(extracted)
        pack_summaries.append(pack_summary(pack, result.bytes_on_disk, result.sha256, extracted))

    appended_downloads = append_new_manifest_records(download_manifest, download_records)
    appended_extractions = append_new_manifest_records(extraction_manifest, extraction_records)
    audio_records = [record for record in extraction_records if Path(record["relative_path"]).suffix.lower() in {".ogg", ".wav", ".flac"}]
    summary = {
        "timestamp_utc": utc_now_iso(),
        "source": "kenney",
        "issue": 1279,
        "download_manifest": str(download_manifest),
        "extraction_manifest": str(extraction_manifest),
        "appended_download_records": appended_downloads,
        "appended_extraction_records": appended_extractions,
        "pack_count": len(PACKS),
        "audio_file_count": len(audio_records),
        "license": KENNEY_POLICY["license_name"],
        "license_url": KENNEY_POLICY["license_url"],
        "packs": pack_summaries,
    }
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


def build_download_item(vault_root: Path, pack: AudioPack) -> DownloadItem:
    page = fetch_text(pack.page_url)
    if not has_cc0_marker(page):
        raise RuntimeError(f"Official Kenney page lacks a CC0 marker: {pack.page_url}")
    download_url = extract_zip_url(page, pack.slug)
    declared_size = content_length(download_url)
    destination = Path("sources") / "kenney" / "audio" / pack.slug / f"kenney_{pack.slug}.zip"
    assert_under(vault_root, vault_root / destination)
    return DownloadItem(
        source=KENNEY_POLICY["source"],
        asset_id=pack.slug,
        asset_type=KENNEY_POLICY["asset_type"],
        title=pack.title,
        source_url=pack.page_url,
        license_name=KENNEY_POLICY["license_name"],
        license_url=KENNEY_POLICY["license_url"],
        api_url=KENNEY_POLICY["api_url"],
        url=download_url,
        destination=destination,
        declared_size=declared_size,
        md5=None,
        bundle_key=f"kenney:audio:{pack.slug}:kenney_{pack.slug}.zip",
        extraction_key=str(Path("extracted") / "kenney" / "audio" / pack.slug),
    )


def fetch_text(url: str) -> str:
    request = urllib.request.Request(url, headers={"User-Agent": "LightInTheDarkGameEditor/kenney-audio-intake"})
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read().decode("utf-8", errors="replace")


def has_cc0_marker(page: str) -> bool:
    normalized = re.sub(r"\s+", " ", page).lower()
    return "creative commons cc0" in normalized or "cc0 license" in normalized


def extract_zip_url(page: str, slug: str) -> str:
    matches = re.findall(r"https://kenney\.nl/media/pages/assets/[^'\" ]+?kenney_[^'\" ]+?\.zip", page)
    expected = f"kenney_{slug}.zip"
    for url in matches:
        if url.endswith(expected):
            return url
    raise RuntimeError(f"Official Kenney page did not expose {expected}")


def content_length(url: str) -> int:
    request = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "LightInTheDarkGameEditor/kenney-audio-intake"})
    with urllib.request.urlopen(request, timeout=60) as response:
        value = response.headers.get("Content-Length")
    if not value or not value.isdigit() or int(value) <= 0:
        raise RuntimeError(f"Download URL lacks a positive Content-Length: {url}")
    return int(value)


def verify_downloaded_license(vault_root: Path, item: DownloadItem, records: list[dict]) -> None:
    license_records = [record for record in records if Path(record["relative_path"]).name.lower() == "license.txt"]
    if len(license_records) != 1:
        raise RuntimeError(f"Expected one License.txt in {item.asset_id}, found {len(license_records)}")
    text = (vault_root / license_records[0]["relative_path"]).read_text(encoding="utf-8", errors="replace")
    normalized = text.lower()
    if "creative commons zero" not in normalized or "cc0" not in normalized:
        raise RuntimeError(f"License.txt for {item.asset_id} does not prove CC0")


def pack_summary(pack: AudioPack, zip_bytes: int, zip_sha256: str, records: list[dict]) -> dict:
    suffixes = [Path(record["relative_path"]).suffix.lower() for record in records]
    audio_count = sum(1 for suffix in suffixes if suffix in {".ogg", ".wav", ".flac"})
    return {
        "slug": pack.slug,
        "title": pack.title,
        "source_url": pack.page_url,
        "zip_bytes": zip_bytes,
        "zip_sha256": zip_sha256,
        "extraction_records": len(records),
        "audio_file_count": audio_count,
    }


if __name__ == "__main__":
    raise SystemExit(main())
