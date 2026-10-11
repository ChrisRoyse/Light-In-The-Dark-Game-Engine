from __future__ import annotations

import argparse
import logging
import urllib.parse
from pathlib import Path
from typing import Any

from .config import AMBIENTCG_API
from .models import Candidate, DownloadItem, SourcePolicy
from .net import declared_content_length, fetch_json, resolve_single_redirect
from .utils import filename_from_url, safe_rel_path, sanitize_component, score_asset, source_policy


def ambient_fetch_assets(params: dict[str, str], user_agent: str) -> list[dict[str, Any]]:
    query = urllib.parse.urlencode(params)
    response = fetch_json(f"{AMBIENTCG_API}?{query}", user_agent)
    assets = response.get("assets")
    if not isinstance(assets, list):
        raise ValueError("ambientCG response did not contain an assets array")
    return assets

def exact_ambient_download(asset: dict[str, Any], attributes: str, extension: str) -> dict[str, Any]:
    for entry in asset.get("downloads") or []:
        if entry.get("attributes") == attributes and entry.get("extension") == extension:
            required = {"url", "size", "attributes", "extension"}
            if not required.issubset(entry):
                missing = sorted(required - set(entry))
                raise KeyError(f"missing ambientCG download fields: {missing}")
            return entry
    raise KeyError(f"missing ambientCG download {attributes}.{extension}")

def ambient_candidate_from_asset(
    *,
    policy: SourcePolicy,
    asset: dict[str, Any],
    asset_type: str,
    requested_downloads: tuple[tuple[str, str], ...],
    user_agent: str,
) -> Candidate:
    asset_id = str(asset["id"])
    title = str(asset.get("title") or asset_id)
    source_url = str(asset.get("url") or f"https://ambientcg.com/a/{asset_id}")
    score = score_asset(asset_id, asset)
    tags = tuple(str(tag) for tag in asset.get("tags") or [])
    items: list[DownloadItem] = []
    for attributes, extension in requested_downloads:
        entry = exact_ambient_download(asset, attributes, extension)
        download_url = resolve_single_redirect(entry["url"], user_agent)
        filename = filename_from_url(entry["url"])
        root_parts = (
            "sources",
            "ambientcg",
            sanitize_component(asset_type),
            sanitize_component(asset_id),
            sanitize_component(attributes),
        )
        bundle_key = f"ambientcg:{asset_type}:{asset_id}:{attributes}:{extension}"
        extraction_key = None
        if extension.lower() == "zip":
            extraction_key = str(Path("extracted", "ambientcg", asset_type, asset_id, attributes))
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
                url=download_url,
                destination=Path(*root_parts, filename),
                declared_size=int(entry["size"]),
                md5=None,
                bundle_key=bundle_key,
                extraction_key=extraction_key,
            )
        )
    return Candidate(
        source=policy.source,
        asset_id=asset_id,
        asset_type=asset_type,
        title=title,
        source_url=source_url,
        license_name=policy.license_name,
        license_url=policy.license_url,
        score=score,
        tags=tags,
        items=tuple(items),
    )

def ambientcg_candidates(args: argparse.Namespace, user_agent: str) -> tuple[list[Candidate], list[dict[str, Any]]]:
    policy = source_policy("ambientcg")
    candidates: list[Candidate] = []
    rejections: list[dict[str, Any]] = []
    include = "downloads,tags,type,title,url"
    specs: list[tuple[str, str, int, tuple[tuple[str, str], ...], int]] = [
        (
            "material",
            "material",
            args.ambient_material_limit,
            ((args.ambient_material_resolution, "zip"),),
            2,
        ),
        (
            "hdri",
            "hdri",
            args.ambient_hdri_limit,
            ((args.ambient_hdri_resolution, "zip"),),
            1,
        ),
        (
            "3d-model",
            "model",
            args.ambient_model_limit,
            ((args.ambient_model_resolution, "zip"),),
            1,
        ),
    ]
    terrain_downloads: list[tuple[str, str]] = [(args.ambient_terrain_resolution, "zip")]
    if args.include_ambient_terrain_obj:
        terrain_downloads.append((args.ambient_terrain_obj_resolution, "obj"))
    specs.append(("terrain", "terrain", args.ambient_terrain_limit, tuple(terrain_downloads), 1))

    for query_type, asset_type, limit, downloads, min_score in specs:
        params = {
            "type": query_type,
            "sort": "popular",
            "limit": "500",
            "include": include,
        }
        if query_type == "material":
            params["category"] = "Rocks,Ground,PavingStones,Bricks,Concrete,Wood,Tiles"
        assets = ambient_fetch_assets(params, user_agent)
        ranked = sorted(
            assets,
            key=lambda asset: (score_asset(str(asset.get("id", "")), asset), str(asset.get("id", ""))),
            reverse=True,
        )
        selected = [asset for asset in ranked if score_asset(str(asset.get("id", "")), asset) >= min_score][:limit]
        logging.info("ambientcg %s selected %s of %s assets", asset_type, len(selected), len(assets))
        for asset in selected:
            asset_id = str(asset.get("id", ""))
            try:
                candidates.append(
                    ambient_candidate_from_asset(
                        policy=policy,
                        asset=asset,
                        asset_type=asset_type,
                        requested_downloads=downloads,
                        user_agent=user_agent,
                    )
                )
            except Exception as exc:
                rejections.append(
                    {
                        "source": policy.source,
                        "asset_id": asset_id,
                        "asset_type": asset_type,
                        "title": asset.get("title") or asset_id,
                        "reason": str(exc),
                        "requested_profile": ",".join(f"{a}.{e}" for a, e in downloads),
                    }
                )
    return candidates, rejections
