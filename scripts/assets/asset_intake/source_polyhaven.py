from __future__ import annotations

import argparse
import logging
import urllib.parse
from pathlib import Path
from typing import Any

from .config import POLYHAVEN_API
from .models import Candidate, DownloadItem, SourcePolicy
from .net import fetch_json
from .utils import filename_from_url, safe_rel_path, sanitize_component, score_asset, source_policy


def exact_poly_file(files: dict[str, Any], top_key: str, resolution: str, extension: str) -> dict[str, Any]:
    try:
        entry = files[top_key][resolution][extension]
    except KeyError as exc:
        raise KeyError(f"missing Poly Haven {top_key}/{resolution}/{extension}") from exc
    required = {"url", "size", "md5"}
    if not required.issubset(entry):
        missing = sorted(required - set(entry))
        raise KeyError(f"missing Poly Haven file fields: {missing}")
    return entry

def poly_items_for_file(
    *,
    policy: SourcePolicy,
    asset_id: str,
    title: str,
    asset_type: str,
    source_url: str,
    root_parts: tuple[str, ...],
    bundle_key: str,
    file_entry: dict[str, Any],
) -> tuple[DownloadItem, ...]:
    items: list[DownloadItem] = []
    main_name = filename_from_url(file_entry["url"])
    main_dest = Path("sources", "polyhaven", *root_parts, main_name)
    items.append(
        DownloadItem(
            source=policy.source,
            asset_id=asset_id,
            asset_type=asset_type,
            title=title,
            source_url=source_url,
            license_name=policy.license_name,
            license_url=policy.license_url,
            api_url=policy.api_url,
            url=file_entry["url"],
            destination=main_dest,
            declared_size=int(file_entry["size"]),
            md5=str(file_entry["md5"]),
            bundle_key=bundle_key,
        )
    )
    include = file_entry.get("include") or {}
    if not isinstance(include, dict):
        raise ValueError(f"Poly Haven include field is not an object for {asset_id}")
    for include_rel, include_entry in include.items():
        required = {"url", "size", "md5"}
        if not required.issubset(include_entry):
            missing = sorted(required - set(include_entry))
            raise KeyError(f"missing Poly Haven include fields for {asset_id}: {missing}")
        rel = safe_rel_path(str(include_rel))
        items.append(
            DownloadItem(
                source=policy.source,
                asset_id=asset_id,
                asset_type=asset_type,
                title=title,
                source_url=source_url,
                license_name=policy.license_name,
                license_url=policy.license_url,
                api_url=policy.api_url,
                url=include_entry["url"],
                destination=Path("sources", "polyhaven", *root_parts, rel),
                declared_size=int(include_entry["size"]),
                md5=str(include_entry["md5"]),
                bundle_key=bundle_key,
            )
        )
    return tuple(items)

def polyhaven_candidates(args: argparse.Namespace, user_agent: str) -> tuple[list[Candidate], list[dict[str, Any]]]:
    policy = source_policy("polyhaven")
    candidates: list[Candidate] = []
    rejections: list[dict[str, Any]] = []
    fetch_specs = [
        ("models", "model", args.poly_model_limit, args.poly_model_resolution, "gltf", "gltf"),
        ("textures", "surface", args.poly_texture_limit, args.poly_surface_resolution, "gltf", "gltf"),
        ("hdris", "hdri", args.poly_hdri_limit, args.poly_hdri_resolution, "hdri", "hdr"),
    ]
    for api_type, asset_type, limit, resolution, top_key, extension in fetch_specs:
        assets_url = f"{POLYHAVEN_API}/assets?{urllib.parse.urlencode({'t': api_type})}"
        assets = fetch_json(assets_url, user_agent)
        if not isinstance(assets, dict):
            raise ValueError(f"Poly Haven {api_type} asset list was not an object")
        ranked = sorted(
            assets.items(),
            key=lambda pair: (score_asset(pair[0], pair[1]), int(pair[1].get("download_count", 0))),
            reverse=True,
        )
        selected = [pair for pair in ranked if score_asset(pair[0], pair[1]) > 0][:limit]
        logging.info("polyhaven %s selected %s of %s assets", api_type, len(selected), len(assets))
        for asset_id, metadata in selected:
            title = str(metadata.get("name") or asset_id)
            source_url = f"https://polyhaven.com/a/{asset_id}"
            score = score_asset(asset_id, metadata)
            tags = tuple(str(tag) for tag in metadata.get("tags") or [])
            try:
                files = fetch_json(f"{POLYHAVEN_API}/files/{urllib.parse.quote(asset_id)}", user_agent)
                file_entry = exact_poly_file(files, top_key, resolution, extension)
                root_parts = (asset_type + "s", sanitize_component(asset_id), sanitize_component(resolution))
                bundle_key = f"polyhaven:{asset_type}:{asset_id}:{resolution}:{extension}"
                items = poly_items_for_file(
                    policy=policy,
                    asset_id=asset_id,
                    title=title,
                    asset_type=asset_type,
                    source_url=source_url,
                    root_parts=root_parts,
                    bundle_key=bundle_key,
                    file_entry=file_entry,
                )
                candidates.append(
                    Candidate(
                        source=policy.source,
                        asset_id=asset_id,
                        asset_type=asset_type,
                        title=title,
                        source_url=source_url,
                        license_name=policy.license_name,
                        license_url=policy.license_url,
                        score=score,
                        tags=tags,
                        items=items,
                    )
                )
            except Exception as exc:
                rejections.append(
                    {
                        "source": policy.source,
                        "asset_id": asset_id,
                        "asset_type": asset_type,
                        "title": title,
                        "reason": str(exc),
                        "requested_profile": f"{top_key}/{resolution}/{extension}",
                    }
                )
    return candidates, rejections
