from __future__ import annotations

import hashlib
import fnmatch
import json
import logging
import os
import shutil
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Any

from . import net
from .config import ASSET_TYPE_PRIORITY
from .models import Candidate, DownloadItem, DownloadResult
from .utils import safe_rel_path, utc_now_iso


def _open_source_stream(url: str, user_agent: str, timeout: int = 120):
    """Open a streaming response for a download URL.

    Google Drive URLs are routed through the confirm-token resolver so large
    packs (which serve an HTML virus-scan interstitial on the first request)
    download correctly; all other URLs use a plain request.
    """
    file_id = net.google_drive_file_id_from_url(url)
    if file_id is not None:
        return net.open_google_drive_stream(file_id, user_agent, timeout=timeout, context=url)
    request = urllib.request.Request(url, headers={"User-Agent": user_agent})
    return urllib.request.urlopen(request, timeout=timeout)


def _filesystem_path(path: Path) -> str | Path:
    if os.name != "nt":
        return path
    raw = os.fspath(path)
    if raw.startswith("\\\\?\\"):
        return raw
    absolute = path if path.is_absolute() else Path.cwd() / path
    try:
        normalized = str(absolute.resolve(strict=False))
    except OSError:
        normalized = str(absolute.absolute())
    normalized = normalized.replace("/", "\\")
    if normalized.startswith("\\\\?\\"):
        return normalized
    if normalized.startswith("\\\\"):
        return "\\\\?\\UNC\\" + normalized[2:]
    return "\\\\?\\" + normalized


def _path_exists(path: Path) -> bool:
    return os.path.exists(_filesystem_path(path))


def _path_stat(path: Path) -> os.stat_result:
    return os.stat(_filesystem_path(path))


def _mkdir(path: Path) -> None:
    os.makedirs(_filesystem_path(path), exist_ok=True)


def select_candidates(candidates: list[Candidate], max_bytes: int) -> tuple[list[Candidate], list[dict[str, Any]]]:
    selected: list[Candidate] = []
    rejected: list[dict[str, Any]] = []
    used = 0
    ranked = sorted(
        candidates,
        key=lambda candidate: (
            ASSET_TYPE_PRIORITY.get(candidate.asset_type, 0),
            candidate.score,
            -sum(item.declared_size for item in candidate.items),
            candidate.source,
            candidate.asset_id,
        ),
        reverse=True,
    )
    for candidate in ranked:
        declared = sum(item.declared_size for item in candidate.items)
        if declared <= 0:
            rejected.append(
                {
                    "source": candidate.source,
                    "asset_id": candidate.asset_id,
                    "asset_type": candidate.asset_type,
                    "title": candidate.title,
                    "score": candidate.score,
                    "declared_size": declared,
                    "reason": "non-positive size",
                }
            )
            continue
        if used + declared > max_bytes:
            rejected.append(
                {
                    "source": candidate.source,
                    "asset_id": candidate.asset_id,
                    "asset_type": candidate.asset_type,
                    "title": candidate.title,
                    "score": candidate.score,
                    "declared_size": declared,
                    "reason": "run byte cap would be exceeded",
                }
            )
            continue
        selected.append(candidate)
        used += declared
    return selected, rejected

# Archive magic prefixes by file extension (issue #1593). A Google Drive large
# pack that fails the virus-scan confirm flow lands as an HTML interstitial page
# saved in place of the real archive; that page can match the declared byte size
# yet is NOT the binary. Magic-byte verification catches a payload that passed the
# size check but is not the expected archive, so a pack is NEVER recorded as
# ingested without byte-level proof it is the real file.
_ZIP_MAGICS = (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08")
_HTML_SNIFF_PREFIXES = (b"<!doctype", b"<html", b"<head", b"<!--", b"<?xml", b"<meta")


def verify_download_magic(part_path: Path, destination: Path, *, is_drive: bool, context: str) -> None:
    """Verify a freshly-downloaded payload is the expected binary, not HTML/junk.

    Applies to Google Drive downloads (where the #1593 interstitial hides) and to
    any ``.zip`` archive. Reads only the first bytes off disk. Raises LOUD naming
    the pack + first bytes when the payload is an HTML interstitial or a ``.zip``
    that lacks the PK local-file-header magic -- never a silent accept.
    """
    suffix = destination.suffix.lower()
    if not is_drive and suffix != ".zip":
        return
    with open(_filesystem_path(part_path), "rb") as handle:
        head = handle.read(16)
    lowered = head.lstrip().lower()
    if is_drive and any(lowered.startswith(prefix) for prefix in _HTML_SNIFF_PREFIXES):
        raise RuntimeError(
            f"Downloaded payload for {context} ({destination}) is HTML, not the expected binary "
            f"archive -- the Google Drive virus-scan interstitial was not resolved. "
            f"first-bytes={head!r}"
        )
    if suffix == ".zip" and not any(head.startswith(magic) for magic in _ZIP_MAGICS):
        raise RuntimeError(
            f"Downloaded payload for {context} ({destination}) is not a valid ZIP archive "
            f"(missing PK magic); refusing to record it as ingested. first-bytes={head!r}"
        )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(_filesystem_path(path), "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def hash_file(path: Path, include_md5: bool) -> tuple[str, str | None]:
    sha = hashlib.sha256()
    md5 = hashlib.md5() if include_md5 else None
    with open(_filesystem_path(path), "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            sha.update(chunk)
            if md5 is not None:
                md5.update(chunk)
    return sha.hexdigest(), md5.hexdigest() if md5 is not None else None

def assert_under(parent: Path, child: Path) -> None:
    parent_resolved = parent.resolve()
    child_absolute = child if child.is_absolute() else Path.cwd() / child
    probe = child_absolute
    missing_parts: list[str] = []
    while not probe.exists() and probe != probe.parent:
        missing_parts.append(probe.name)
        probe = probe.parent
    child_resolved = probe.resolve()
    for part in reversed(missing_parts):
        child_resolved = child_resolved / part
    try:
        child_resolved.relative_to(parent_resolved)
    except ValueError as exc:
        raise ValueError(f"Refusing path outside intended directory: {child}") from exc

def download_item(vault_root: Path, item: DownloadItem, user_agent: str, retries: int) -> DownloadResult:
    destination = vault_root / item.destination
    assert_under(vault_root, destination)
    _mkdir(destination.parent)

    if _path_exists(destination):
        actual_size = _path_stat(destination).st_size
        if actual_size != item.declared_size:
            raise RuntimeError(
                f"Existing file size mismatch for {destination}: expected {item.declared_size}, got {actual_size}"
            )
        sha, md5_actual = hash_file(destination, item.md5 is not None)
        if item.md5 is not None and md5_actual != item.md5:
            raise RuntimeError(f"Existing file md5 mismatch for {destination}: expected {item.md5}, got {md5_actual}")
        logging.info("existing verified %s", destination)
        return DownloadResult(item, "existing_verified", actual_size, sha, md5_actual)

    part = destination.with_name(destination.name + ".part")
    if _path_exists(part):
        raise RuntimeError(f"Partial download exists and must be inspected or removed manually: {part}")

    is_drive = net.google_drive_file_id_from_url(item.url) is not None
    last_error: Exception | None = None
    for attempt in range(1, retries + 1):
        sha = hashlib.sha256()
        md5 = hashlib.md5() if item.md5 is not None else None
        bytes_written = 0
        try:
            with _open_source_stream(item.url, user_agent, timeout=120) as response, open(
                _filesystem_path(part), "wb"
            ) as output:
                while True:
                    chunk = response.read(1024 * 1024)
                    if not chunk:
                        break
                    output.write(chunk)
                    sha.update(chunk)
                    if md5 is not None:
                        md5.update(chunk)
                    bytes_written += len(chunk)
            if bytes_written != item.declared_size:
                raise RuntimeError(
                    f"Downloaded size mismatch for {destination}: expected {item.declared_size}, got {bytes_written}"
                )
            md5_actual = md5.hexdigest() if md5 is not None else None
            if item.md5 is not None and md5_actual != item.md5:
                raise RuntimeError(f"Downloaded md5 mismatch for {destination}: expected {item.md5}, got {md5_actual}")
            verify_download_magic(part, destination, is_drive=is_drive, context=item.bundle_key)
            os.replace(_filesystem_path(part), _filesystem_path(destination))
            logging.info("downloaded %s (%s bytes)", destination, bytes_written)
            return DownloadResult(item, "downloaded", bytes_written, sha.hexdigest(), md5_actual)
        except Exception as exc:
            last_error = exc
            if _path_exists(part):
                os.unlink(_filesystem_path(part))
            transient, _status = net._classify_transient(exc)
            if isinstance(exc, urllib.error.HTTPError) and not transient:
                # Permanent client error (e.g. 404/403): no wasted retries, fail loud now.
                raise RuntimeError(
                    f"Failed to download {item.url}: permanent HTTP {exc.code} {exc.reason}"
                ) from exc
            logging.warning("download attempt %s/%s failed for %s: %s", attempt, retries, item.url, exc)
            if attempt < retries:
                delay = net._backoff_delay(attempt, net._retry_after_from_exc(exc))
                time.sleep(delay)
    raise RuntimeError(f"Failed to download {item.url} after {retries} attempts: {last_error}")

def write_jsonl(path: Path, records: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")

def result_record(vault_root: Path, result: DownloadResult) -> dict[str, Any]:
    item = result.item
    absolute = vault_root / item.destination
    return {
        "record_type": "download",
        "timestamp_utc": utc_now_iso(),
        "source": item.source,
        "asset_id": item.asset_id,
        "asset_type": item.asset_type,
        "title": item.title,
        "source_url": item.source_url,
        "license": item.license_name,
        "license_url": item.license_url,
        "api_url": item.api_url,
        "download_url": item.url,
        "bundle_key": item.bundle_key,
        "relative_path": str(item.destination).replace("\\", "/"),
        "absolute_path": str(absolute),
        "declared_size": item.declared_size,
        "bytes_on_disk": result.bytes_on_disk,
        "sha256": result.sha256,
        "md5_expected": item.md5,
        "md5_actual": result.md5_actual,
        "status": result.status,
        "extraction_key": item.extraction_key,
    }

def verify_zip_and_extract(vault_root: Path, item: DownloadItem) -> list[dict[str, Any]]:
    if item.extraction_key is None:
        return []
    archive = vault_root / item.destination
    extract_dir = vault_root / item.extraction_key
    marker = extract_dir / "_extraction_manifest.jsonl"
    if _path_exists(marker):
        records = [json.loads(line) for line in marker.read_text(encoding="utf-8").splitlines() if line.strip()]
        for record in records:
            path = vault_root / record["relative_path"]
            if not _path_exists(path):
                raise RuntimeError(f"Extraction manifest path missing: {path}")
            if _path_stat(path).st_size != int(record["bytes_on_disk"]):
                raise RuntimeError(f"Extraction size mismatch: {path}")
            actual_sha = sha256_file(path)
            if actual_sha != record["sha256"]:
                raise RuntimeError(f"Extraction sha256 mismatch for {path}: expected {record['sha256']}, got {actual_sha}")
        logging.info("existing extraction verified %s", extract_dir)
        return records
    if extract_dir.exists() and any(extract_dir.iterdir()):
        raise RuntimeError(f"Extraction directory exists without manifest: {extract_dir}")

    temp_dir = extract_dir.parent / f".x{os.getpid():x}"
    if _path_exists(temp_dir):
        raise RuntimeError(f"Temporary extraction directory exists: {temp_dir}")
    _mkdir(temp_dir)
    records: list[dict[str, Any]] = []
    try:
        with zipfile.ZipFile(archive) as zf:
            bad_member = zf.testzip()
            if bad_member is not None:
                raise RuntimeError(f"Zip CRC failed for {archive}: {bad_member}")
            for member in zf.infolist():
                if member.is_dir():
                    continue
                member_path = safe_rel_path(member.filename)
                target = temp_dir / member_path
                assert_under(temp_dir, target)
                _mkdir(target.parent)
                with zf.open(member) as source, open(_filesystem_path(target), "wb") as output:
                    shutil.copyfileobj(source, output)
                sha = sha256_file(target)
                rel_path = target.relative_to(vault_root).as_posix()
                records.append(
                    {
                        "record_type": "extraction",
                        "timestamp_utc": utc_now_iso(),
                        "source": item.source,
                        "asset_id": item.asset_id,
                        "asset_type": item.asset_type,
                        "title": item.title,
                        "archive_relative_path": item.destination.as_posix(),
                        "archive_absolute_path": str(archive),
                        "relative_path": rel_path,
                        "absolute_path": str(target),
                        "bytes_on_disk": _path_stat(target).st_size,
                        "sha256": sha,
                        "license": item.license_name,
                        "license_url": item.license_url,
                        "source_url": item.source_url,
                        "bundle_key": item.bundle_key,
                        "extraction_key": item.extraction_key,
                    }
                )
        with open(_filesystem_path(temp_dir / "_extraction_manifest.jsonl"), "w", encoding="utf-8") as handle:
            for record in records:
                final_record = dict(record)
                final_record["relative_path"] = final_record["relative_path"].replace(
                    temp_dir.relative_to(vault_root).as_posix(),
                    extract_dir.relative_to(vault_root).as_posix(),
                    1,
                )
                final_record["absolute_path"] = str(vault_root / final_record["relative_path"])
                handle.write(json.dumps(final_record, sort_keys=True) + "\n")
        _mkdir(extract_dir.parent)
        for attempt in range(1, 6):
            try:
                os.rename(_filesystem_path(temp_dir), _filesystem_path(extract_dir))
                break
            except PermissionError:
                if attempt == 5:
                    shutil.move(str(temp_dir), str(extract_dir))
                    break
                time.sleep(0.2 * attempt)
        final_records = [
            json.loads(line)
            for line in marker.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]
        logging.info("extracted %s files from %s", len(final_records), archive)
        return final_records
    except Exception:
        if _path_exists(temp_dir):
            shutil.rmtree(_filesystem_path(temp_dir))
        raise

def load_existing_manifest_keys(manifest_path: Path) -> set[tuple[str, str, str]]:
    keys: set[tuple[str, str, str]] = set()
    if not manifest_path.exists():
        return keys
    with manifest_path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            record = json.loads(line)
            if record.get("record_type") == "download":
                keys.add(("download", record["bundle_key"], record["relative_path"]))
            elif record.get("record_type") == "extraction":
                keys.add(("extraction", record["bundle_key"], record["relative_path"]))
    return keys

def append_new_manifest_records(manifest_path: Path, records: list[dict[str, Any]]) -> int:
    existing_keys = load_existing_manifest_keys(manifest_path)
    new_records = []
    for record in records:
        record_type = str(record.get("record_type", ""))
        key = (record_type, record["bundle_key"], record["relative_path"])
        if key not in existing_keys:
            new_records.append(record)
            existing_keys.add(key)
    write_jsonl(manifest_path, new_records)
    return len(new_records)

def dir_size(path: Path) -> int:
    if not _path_exists(path):
        return 0
    total = 0
    for root, _, files in os.walk(_filesystem_path(path)):
        for file_name in files:
            total += _path_stat(Path(root) / file_name).st_size
    return total

def file_count(path: Path, pattern: str = "*") -> int:
    if not _path_exists(path):
        return 0
    total = 0
    for _, _, files in os.walk(_filesystem_path(path)):
        total += sum(1 for file_name in files if fnmatch.fnmatch(file_name, pattern))
    return total

def read_jsonl_records(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                records.append(json.loads(line))
    return records

def count_by(records: list[dict[str, Any]], key: str) -> dict[str, int]:
    counts: dict[str, int] = {}
    for record in records:
        value = str(record.get(key, ""))
        counts[value] = counts.get(value, 0) + 1
    return dict(sorted(counts.items()))

def make_vault_state_summary(
    *,
    vault_root: Path,
    manifest_path: Path,
    extraction_manifest_path: Path,
    latest_run_summary_path: Path,
) -> dict[str, Any]:
    download_records = read_jsonl_records(manifest_path)
    extraction_records = read_jsonl_records(extraction_manifest_path)
    return {
        "timestamp_utc": utc_now_iso(),
        "vault_root": str(vault_root),
        "latest_run_summary": str(latest_run_summary_path),
        "source_of_truth": {
            "download_manifest": str(manifest_path),
            "extraction_manifest": str(extraction_manifest_path),
            "asset_files_root": str(vault_root / "sources"),
            "extracted_files_root": str(vault_root / "extracted"),
        },
        "download_manifest_records": len(download_records),
        "extraction_manifest_records": len(extraction_records),
        "download_records_by_source": count_by(download_records, "source"),
        "download_records_by_asset_type": count_by(download_records, "asset_type"),
        "source_file_count": file_count(vault_root / "sources"),
        "source_bytes_on_disk": dir_size(vault_root / "sources"),
        "extracted_file_count": file_count(vault_root / "extracted"),
        "extracted_bytes_on_disk": dir_size(vault_root / "extracted"),
        "part_file_count": file_count(vault_root / "sources", "*.part"),
    }
