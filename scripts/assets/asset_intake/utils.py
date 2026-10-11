from __future__ import annotations

import datetime as dt
import logging
import posixpath
import re
import sys
import urllib.parse
from pathlib import Path

from .config import POLICIES, VISUAL_TARGET_KEYWORDS
from .models import SourcePolicy


def utc_now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat(timespec="seconds")

def setup_logging(vault_root: Path) -> Path:
    logs_dir = vault_root / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    log_path = logs_dir / f"asset_intake_{dt.datetime.now().strftime('%Y%m%d_%H%M%S')}.log"
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            logging.StreamHandler(sys.stdout),
            logging.FileHandler(log_path, encoding="utf-8"),
        ],
    )
    return log_path

def sanitize_component(value: str) -> str:
    safe = re.sub(r"[^A-Za-z0-9._-]+", "_", value.strip())
    safe = safe.strip("._")
    if not safe:
        raise ValueError(f"Cannot make a safe path component from {value!r}")
    return safe

def safe_rel_path(raw: str) -> Path:
    parts = [sanitize_component(part) for part in raw.replace("\\", "/").split("/") if part]
    if not parts:
        raise ValueError(f"Empty relative path from {raw!r}")
    return Path(*parts)

def filename_from_url(url: str) -> str:
    parsed = urllib.parse.urlparse(url)
    name = posixpath.basename(urllib.parse.unquote(parsed.path))
    if name.lower() == "get":
        query = urllib.parse.parse_qs(parsed.query)
        requested_file = query.get("file", [None])[0]
        if requested_file:
            name = requested_file
    return sanitize_component(name)

def score_asset(asset_id: str, metadata: dict[str, Any]) -> int:
    fields: list[str] = [asset_id, str(metadata.get("name", "")), str(metadata.get("title", ""))]
    for key in ("description", "shortDescription", "longDescription"):
        value = metadata.get(key)
        if value:
            fields.append(str(value))
    for key in ("tags", "categories"):
        value = metadata.get(key) or []
        if isinstance(value, list):
            fields.extend(str(entry) for entry in value)
    haystack = " ".join(fields).lower().replace("_", " ").replace("-", " ")
    score = 0
    for keyword, weight in VISUAL_TARGET_KEYWORDS.items():
        if re.search(rf"\b{re.escape(keyword)}\b", haystack):
            score += weight
    if "collection: the_shed" in haystack:
        score += 1
    return score

def source_policy(name: str) -> SourcePolicy:
    try:
        return POLICIES[name]
    except KeyError as exc:
        raise ValueError(f"Unsupported source {name!r}; allowed sources: {sorted(POLICIES)}") from exc
