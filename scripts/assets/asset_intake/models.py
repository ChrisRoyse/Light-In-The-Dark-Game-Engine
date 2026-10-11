from __future__ import annotations

import dataclasses
from pathlib import Path


@dataclasses.dataclass(frozen=True)
class SourcePolicy:
    source: str
    license_name: str
    license_url: str
    api_url: str

@dataclasses.dataclass(frozen=True)
class DownloadItem:
    source: str
    asset_id: str
    asset_type: str
    title: str
    source_url: str
    license_name: str
    license_url: str
    api_url: str
    url: str
    destination: Path
    declared_size: int
    md5: str | None
    bundle_key: str
    extraction_key: str | None = None

@dataclasses.dataclass(frozen=True)
class Candidate:
    source: str
    asset_id: str
    asset_type: str
    title: str
    source_url: str
    license_name: str
    license_url: str
    score: int
    tags: tuple[str, ...]
    items: tuple[DownloadItem, ...]
    rejected_reason: str | None = None

@dataclasses.dataclass(frozen=True)
class DownloadResult:
    item: DownloadItem
    status: str
    bytes_on_disk: int
    sha256: str
    md5_actual: str | None

@dataclasses.dataclass(frozen=True)
class DriveEntry:
    file_id: str
    name: str
    kind: str

@dataclasses.dataclass(frozen=True)
class DriveFile:
    file_id: str
    name: str
    path_parts: tuple[str, ...]
    url: str
    declared_size: int
    content_type: str
