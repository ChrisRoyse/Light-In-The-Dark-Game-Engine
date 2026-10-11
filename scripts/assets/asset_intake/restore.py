"""Floor-targeted / list-targeted vault restore (issue #1596).

Full-corpus intake pulls the entire ~25GB catalog. Tests only need a small,
minimal-sufficient subset. This module adds an explicit *targeted* restore that
fetches EXACTLY the requested assets (by ``source:asset_id``) at the smallest
available resolution, then verifies -- by an independent on-disk readback -- that
every requested item actually landed (existence + size + spot sha256). Any miss
is a LOUD hard-fail, never a silent partial restore.

Per-role floor goal-seeking (architecture>=500, character>=100, ...) is NOT
classified in Python: the authoritative semantic-role classifier lives in Rust
(crates/litd-editor/src/asset_vault_semantics.rs). Duplicating it in Python would
create a second source of truth. Instead, ``--floor`` mode (issue #1733) writes the
candidate metadata to a catalog JSONL and shells out to the Rust selector binary
``litd-editor-asset-floor-select``, which classifies with the SAME code the editor
tests use and returns the cheapest-first ``source:asset_id`` subset that just meets
every floor. Python only consumes that selection; if the binary is absent or fails,
we fail LOUD naming the exact command -- never silently fall back to a guess.
"""

from __future__ import annotations

import json
import logging
import os
import subprocess
import urllib.parse
from pathlib import Path
from typing import Any

from .config import POLYHAVEN_API
from .models import Candidate, DownloadItem
from .net import fetch_json
from .source_ambientcg import ambient_candidate_from_asset, ambient_fetch_assets
from .source_polyhaven import exact_poly_file, poly_items_for_file
from .storage import sha256_file
from .utils import sanitize_component, source_policy

# Smallest useful variants -- targeted restore is about coverage/count, not 4K fidelity.
POLY_TARGET_PROFILES: tuple[tuple[str, str, str], ...] = (
    ("hdri", "1k", "hdr"),
    ("gltf", "1k", "gltf"),
)
AMBIENT_TARGET_PROFILES: dict[str, tuple[str, str, str]] = {
    "material": ("Material", "1K-JPG", "zip"),
    "hdri": ("hdri", "1K-HDR", "zip"),
    "3d-model": ("model", "LQ-2K-JPG", "zip"),
    "terrain": ("terrain", "1K-JPG", "zip"),
}


def parse_targeted_list(path: Path) -> list[tuple[str, str]]:
    """Parse a targeted-restore list file into ordered unique (source, asset_id).

    Accepts a JSON array of ``"source:asset_id"`` strings or
    ``{"source": ..., "asset_id": ...}`` objects. Every source must be a known
    policy source; unknown source / empty field / duplicate is a LOUD ValueError.
    """
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, list) or not raw:
        raise ValueError(f"Targeted list {path} must be a non-empty JSON array")
    targets: list[tuple[str, str]] = []
    seen: set[tuple[str, str]] = set()
    for entry in raw:
        if isinstance(entry, str):
            source, sep, asset_id = entry.partition(":")
            if not sep:
                raise ValueError(f"Targeted entry {entry!r} must be 'source:asset_id'")
        elif isinstance(entry, dict):
            source = str(entry.get("source", ""))
            asset_id = str(entry.get("asset_id", ""))
        else:
            raise ValueError(f"Targeted entry has unsupported type: {entry!r}")
        source = source.strip()
        asset_id = asset_id.strip()
        if not source or not asset_id:
            raise ValueError(f"Targeted entry missing source/asset_id: {entry!r}")
        source_policy(source)  # validates the source is known; raises loud otherwise
        key = (source, asset_id)
        if key in seen:
            raise ValueError(f"Duplicate targeted entry: {source}:{asset_id}")
        seen.add(key)
        targets.append(key)
    return targets


def _polyhaven_targeted_candidate(asset_id: str, user_agent: str) -> Candidate:
    policy = source_policy("polyhaven")
    files = fetch_json(f"{POLYHAVEN_API}/files/{urllib.parse.quote(asset_id)}", user_agent)
    if not isinstance(files, dict):
        raise ValueError(f"Poly Haven files response for {asset_id} was not an object")
    for top_key, resolution, extension in POLY_TARGET_PROFILES:
        if top_key not in files:
            continue
        asset_type = "hdri" if top_key == "hdri" else "surface"
        file_entry = exact_poly_file(files, top_key, resolution, extension)
        source_url = f"https://polyhaven.com/a/{asset_id}"
        root_parts = (asset_type + "s", sanitize_component(asset_id), sanitize_component(resolution))
        bundle_key = f"polyhaven:{asset_type}:{asset_id}:{resolution}:{extension}"
        items = poly_items_for_file(
            policy=policy,
            asset_id=asset_id,
            title=asset_id,
            asset_type=asset_type,
            source_url=source_url,
            root_parts=root_parts,
            bundle_key=bundle_key,
            file_entry=file_entry,
        )
        return Candidate(
            source=policy.source,
            asset_id=asset_id,
            asset_type=asset_type,
            title=asset_id,
            source_url=source_url,
            license_name=policy.license_name,
            license_url=policy.license_url,
            score=0,
            tags=(),
            items=items,
        )
    raise ValueError(
        f"Poly Haven asset {asset_id} exposes none of the targeted profiles "
        f"{POLY_TARGET_PROFILES}; available top-level keys: {sorted(files)}"
    )


def _ambientcg_targeted_candidate(asset_id: str, user_agent: str) -> Candidate:
    policy = source_policy("ambientcg")
    include = "downloads,tags,type,title,url,dataType"
    assets = ambient_fetch_assets({"id": asset_id, "include": include}, user_agent)
    match = next((a for a in assets if str(a.get("id", "")) == asset_id), None)
    if match is None:
        raise ValueError(f"ambientCG returned no asset with id {asset_id!r}")
    data_type = str(match.get("dataType") or "material").lower()
    profile = AMBIENT_TARGET_PROFILES.get(
        {"physicallybasedmaterial": "material", "hdri": "hdri", "3dmodel": "3d-model", "terrain": "terrain"}.get(
            data_type, "material"
        )
    )
    if profile is None:
        raise ValueError(f"No targeted download profile for ambientCG dataType {data_type!r} ({asset_id})")
    asset_type, attributes, extension = profile
    return ambient_candidate_from_asset(
        policy=policy,
        asset=match,
        asset_type=asset_type,
        requested_downloads=((attributes, extension),),
        user_agent=user_agent,
    )


def build_targeted_candidates(targets: list[tuple[str, str]], user_agent: str) -> list[Candidate]:
    """Build a candidate for EACH requested target directly (no whole-catalog walk).

    Any target that cannot be resolved is a LOUD hard-fail naming the unmet
    target -- never a silent skip (the #1596 'no silent partial restore' rule).
    """
    builders = {
        "polyhaven": _polyhaven_targeted_candidate,
        "ambientcg": _ambientcg_targeted_candidate,
    }
    candidates: list[Candidate] = []
    for source, asset_id in targets:
        builder = builders.get(source)
        if builder is None:
            raise NotImplementedError(
                f"Targeted restore does not yet support source {source!r} "
                f"(requested {source}:{asset_id}); supported: {sorted(builders)}"
            )
        candidate = builder(asset_id, user_agent)
        declared = sum(item.declared_size for item in candidate.items)
        logging.info(
            "targeted candidate %s:%s -> %s file(s), %s declared bytes",
            source,
            asset_id,
            len(candidate.items),
            declared,
        )
        candidates.append(candidate)
    return candidates


def verify_restore_on_disk(
    vault_root: Path,
    selected: list[Candidate],
    download_records: list[dict[str, Any]],
) -> int:
    """Independently prove every requested item landed: manifest + on-disk readback.

    For each item we require (a) a matching manifest download record, (b) the file
    present on disk, (c) size on disk == declared size, and (d) a freshly recomputed
    sha256 equal to the manifest's recorded sha256. Any discrepancy accumulates and
    the function raises LOUD listing every failure -- no silent partial restore.
    """
    records_by_key: dict[tuple[str, str], dict[str, Any]] = {}
    for record in download_records:
        key = (str(record.get("bundle_key", "")), str(record.get("relative_path", "")))
        records_by_key[key] = record
    problems: list[str] = []
    verified = 0
    for candidate in selected:
        for item in candidate.items:
            problems_before = len(problems)
            rel = str(item.destination).replace("\\", "/")
            key = (item.bundle_key, rel)
            record = records_by_key.get(key)
            if record is None:
                problems.append(f"{candidate.source}:{candidate.asset_id} -> no manifest download record for {rel}")
                continue
            path = vault_root / item.destination
            if not path.exists():
                problems.append(f"{candidate.source}:{candidate.asset_id} -> file absent on disk: {path}")
                continue
            actual_size = path.stat().st_size
            if actual_size != item.declared_size:
                problems.append(
                    f"{candidate.source}:{candidate.asset_id} -> size mismatch for {rel}: "
                    f"declared {item.declared_size}, on disk {actual_size}"
                )
            recorded_sha = str(record.get("sha256", ""))
            actual_sha = sha256_file(path)
            if actual_sha != recorded_sha:
                problems.append(
                    f"{candidate.source}:{candidate.asset_id} -> sha256 mismatch for {rel}: "
                    f"manifest {recorded_sha}, on disk {actual_sha}"
                )
            if len(problems) == problems_before:
                verified += 1
    if problems:
        raise RuntimeError(
            "Targeted restore verification FAILED for "
            f"{len(problems)} item(s):\n  - " + "\n  - ".join(problems)
        )
    logging.info("targeted restore verified %s item(s) on disk (existence + size + sha256)", verified)
    return verified


# --- Floor-targeted restore (issue #1733) --------------------------------------------------
# The Rust binary is the single source of truth for role classification + floor selection.
FLOOR_SELECTOR_BIN = "litd-editor-asset-floor-select"


def candidate_catalog_line(candidate: Candidate) -> dict[str, Any]:
    """The classifier-relevant metadata for one candidate (no blobs), for the Rust selector.

    ``relative_path`` is the candidate's first file destination -- enough for the Rust audio
    extension check; every other classifier signal is asset-level (source/asset_id/type/title).
    """
    if not candidate.items:
        raise ValueError(f"candidate {candidate.source}:{candidate.asset_id} has no items to catalog")
    relative_path = str(candidate.items[0].destination).replace("\\", "/")
    declared = sum(item.declared_size for item in candidate.items)
    return {
        "source": candidate.source,
        "asset_id": candidate.asset_id,
        "asset_type": candidate.asset_type,
        "title": candidate.title,
        "relative_path": relative_path,
        "declared_size": int(declared),
    }


def write_catalog_manifest(candidates: list[Candidate], path: Path) -> int:
    """Write candidate metadata to a catalog JSONL the Rust selector consumes. Returns count."""
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [json.dumps(candidate_catalog_line(candidate), sort_keys=True) for candidate in candidates]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    logging.info("wrote %s catalog entries to %s", len(lines), path)
    return len(lines)


def default_floor_selector_bin(repo_root: Path) -> Path:
    """Locate the built Rust selector binary; fail LOUD with the build command if absent."""
    exe = FLOOR_SELECTOR_BIN + (".exe" if os.name == "nt" else "")
    candidates = [repo_root / "target" / "debug" / exe, repo_root / "target" / "release" / exe]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    raise RuntimeError(
        f"floor selector binary {exe} not built; run "
        f"`cargo build -p litd-editor --bin {FLOOR_SELECTOR_BIN}`. "
        f"Searched: {[str(candidate) for candidate in candidates]}"
    )


def run_floor_selector(selector_bin: Path, catalog_path: Path, floor_overrides: list[str]) -> dict[str, Any]:
    """Invoke the Rust selector on the catalog manifest and return its parsed JSON selection.

    Any failure -- binary missing, non-zero exit, or unparseable output -- is a LOUD RuntimeError
    naming the exact command that failed. There is never a Python-side classification fallback.
    """
    command = [str(selector_bin), "--catalog", str(catalog_path)]
    for spec in floor_overrides:
        command += ["--floor", spec]
    printable = " ".join(command)
    try:
        proc = subprocess.run(command, capture_output=True, text=True, check=False)
    except FileNotFoundError as exc:
        raise RuntimeError(
            f"floor selector binary not found; build it with "
            f"`cargo build -p litd-editor --bin {FLOOR_SELECTOR_BIN}`. Command: {printable} ({exc})"
        ) from exc
    if proc.returncode != 0:
        raise RuntimeError(
            f"floor selector failed (exit {proc.returncode}) for command `{printable}`:\n"
            f"{proc.stderr.strip() or proc.stdout.strip()}"
        )
    try:
        selection = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"floor selector emitted invalid JSON for `{printable}`: {exc}\n{proc.stdout[:500]}"
        ) from exc
    if not isinstance(selection, dict) or "selected" not in selection:
        raise RuntimeError(f"floor selector output missing 'selected' key for `{printable}`: {proc.stdout[:500]}")
    return selection


def select_floor_candidates(candidates: list[Candidate], selection: dict[str, Any]) -> list[Candidate]:
    """Reduce the in-memory candidates to exactly the selector's chosen (source, asset_id) set.

    A candidate the selector named but that is absent from the catalog is a LOUD hard-fail, as is
    an empty selection -- restoring nothing silently is forbidden (the #1596 rule).
    """
    selected_raw = selection.get("selected", [])
    keys = [(str(entry["source"]), str(entry["asset_id"])) for entry in selected_raw]
    if not keys:
        raise RuntimeError("floor selector returned an empty selection; refusing to restore nothing")
    by_key: dict[tuple[str, str], Candidate] = {(c.source, c.asset_id): c for c in candidates}
    resolved: list[Candidate] = []
    missing: list[str] = []
    for source, asset_id in keys:
        candidate = by_key.get((source, asset_id))
        if candidate is None:
            missing.append(f"{source}:{asset_id}")
        else:
            resolved.append(candidate)
    if missing:
        raise RuntimeError(f"floor selection referenced candidates absent from the catalog: {missing}")
    return resolved
