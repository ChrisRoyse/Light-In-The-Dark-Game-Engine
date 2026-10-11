from __future__ import annotations

import argparse
import concurrent.futures
import logging
import re
import urllib.parse
from pathlib import Path
from typing import Any

from quaternius_discovery import (
    QUATERNIUS_BASE_URL,
    drive_folder_ids,
    has_license_marker,
    html_text,
    pack_page_urls,
    pack_slug,
    pack_tags,
    pack_title,
)

from .config import KAYKIT_OWNER, KAYKIT_REPO_ALLOWLIST, KENNEY_PACK_SLUGS
from .models import Candidate, DownloadItem
from .net import (
    collect_google_drive_files,
    declared_content_length,
    fetch_json,
    fetch_text,
    google_drive_folder_url,
)
from .utils import filename_from_url, safe_rel_path, sanitize_component, score_asset, source_policy


def first_zip_link_from_html(page_url: str, html: str) -> str:
    links = re.findall(r"""href=["']([^"']+\.zip(?:\?[^"']*)?)["']""", html, flags=re.IGNORECASE)
    if not links:
        raise KeyError(f"no direct zip link found on {page_url}")
    return urllib.parse.urljoin(page_url, links[0].replace("&amp;", "&"))

def title_from_html(fallback: str, html: str) -> str:
    h1_match = re.search(r"<h1[^>]*>(.*?)</h1>", html, flags=re.IGNORECASE | re.DOTALL)
    if h1_match:
        title = re.sub(r"<[^>]+>", "", h1_match.group(1))
        title = re.sub(r"\s+", " ", title).strip()
        if title:
            return title
    return fallback.replace("-", " ").title()

def github_raw_url(owner: str, repo_name: str, branch: str, path: str) -> str:
    encoded_path = urllib.parse.quote(path, safe="/")
    return f"https://raw.githubusercontent.com/{owner}/{repo_name}/{branch}/{encoded_path}"

def kenney_candidates(args: argparse.Namespace, user_agent: str) -> tuple[list[Candidate], list[dict[str, Any]]]:
    policy = source_policy("kenney")
    candidates: list[Candidate] = []
    rejections: list[dict[str, Any]] = []
    for slug in KENNEY_PACK_SLUGS[: args.kenney_pack_limit]:
        page_url = f"https://www.kenney.nl/assets/{slug}"
        try:
            html = fetch_text(page_url, user_agent)
            if "Creative Commons CC0" not in html:
                raise ValueError("official pack page did not contain Creative Commons CC0 marker")
            title = title_from_html(slug, html)
            zip_url = first_zip_link_from_html(page_url, html)
            filename = filename_from_url(zip_url)
            declared_size = declared_content_length(zip_url, user_agent)
            metadata = {
                "title": title,
                "tags": slug.split("-") + ["game", "kit", "3d"],
                "description": f"Kenney CC0 game-ready pack {title}",
            }
            score = score_asset(slug, metadata) + 10
            bundle_key = f"kenney:game-kit:{slug}:{filename}"
            item = DownloadItem(
                source=policy.source,
                asset_id=slug,
                asset_type="game-kit",
                title=title,
                source_url=page_url,
                license_name=policy.license_name,
                license_url=policy.license_url,
                api_url=policy.api_url,
                url=zip_url,
                destination=Path("sources", "kenney", "game-kit", sanitize_component(slug), filename),
                declared_size=declared_size,
                md5=None,
                bundle_key=bundle_key,
                extraction_key=str(Path("extracted", "kenney", "game-kit", slug)),
            )
            candidates.append(
                Candidate(
                    source=policy.source,
                    asset_id=slug,
                    asset_type="game-kit",
                    title=title,
                    source_url=page_url,
                    license_name=policy.license_name,
                    license_url=policy.license_url,
                    score=score,
                    tags=tuple(str(tag) for tag in metadata["tags"]),
                    items=(item,),
                )
            )
        except Exception as exc:
            rejections.append(
                {
                    "source": policy.source,
                    "asset_id": slug,
                    "asset_type": "game-kit",
                    "title": slug,
                    "reason": str(exc),
                    "requested_profile": "official page CC0 marker + direct zip + Content-Length",
                }
            )
    logging.info("kenney game-kit selected %s of %s configured packs", len(candidates), len(KENNEY_PACK_SLUGS))
    return candidates, rejections

def kaykit_candidates(args: argparse.Namespace, user_agent: str) -> tuple[list[Candidate], list[dict[str, Any]]]:
    policy = source_policy("kaykit")
    candidates: list[Candidate] = []
    rejections: list[dict[str, Any]] = []
    repos_url = f"https://api.github.com/users/{KAYKIT_OWNER}/repos?per_page=100"
    repo_entries = fetch_json(repos_url, user_agent)
    repo_by_name = {entry["name"]: entry for entry in repo_entries}
    selected_repos = KAYKIT_REPO_ALLOWLIST[: args.kaykit_repo_limit]
    for repo_name in selected_repos:
        try:
            repo = repo_by_name[repo_name]
            branch = str(repo.get("default_branch") or "")
            if branch != "main":
                raise ValueError(f"expected default branch main, got {branch!r}")
            html_url = str(repo["html_url"])
            license_url = f"{html_url}/blob/{branch}/LICENSE.txt"
            raw_license_url = github_raw_url(KAYKIT_OWNER, repo_name, branch, "LICENSE.txt")
            license_text = fetch_text(raw_license_url, user_agent)
            if "CC0 1.0 Universal" not in license_text and "Creative Commons Zero" not in license_text:
                raise ValueError("LICENSE.txt did not contain CC0 1.0 Universal marker")
            readme_text = fetch_text(github_raw_url(KAYKIT_OWNER, repo_name, branch, "README.md"), user_agent)
            if "CC0" not in readme_text:
                raise ValueError("README.md did not contain CC0 marker")
            tree_url = f"https://api.github.com/repos/{KAYKIT_OWNER}/{repo_name}/git/trees/{branch}?recursive=1"
            tree = fetch_json(tree_url, user_agent)
            if tree.get("truncated"):
                raise RuntimeError(f"GitHub tree was truncated for {repo_name}")
            items: list[DownloadItem] = []
            seen_destinations: set[str] = set()
            for blob in sorted(tree.get("tree", []), key=lambda entry: str(entry.get("path", "")).lower()):
                if blob.get("type") != "blob":
                    continue
                path = str(blob.get("path") or "")
                size = int(blob.get("size") or 0)
                if size <= 0:
                    rejections.append(
                        {
                            "source": policy.source,
                            "asset_id": repo_name,
                            "asset_type": "game-kit",
                            "title": repo_name,
                            "reason": f"skipped zero-byte GitHub blob {path}",
                            "requested_profile": "GitHub tree blob with positive size",
                        }
                    )
                    continue
                relative_path = safe_rel_path(path)
                destination = Path("sources", "kaykit", "game-kit", sanitize_component(repo_name), relative_path)
                destination_key = destination.as_posix().lower()
                if destination_key in seen_destinations:
                    raise RuntimeError(f"Sanitized path collision in {repo_name}: {path}")
                seen_destinations.add(destination_key)
                items.append(
                    DownloadItem(
                        source=policy.source,
                        asset_id=repo_name,
                        asset_type="game-kit",
                        title=str(repo.get("description") or repo_name),
                        source_url=html_url,
                        license_name=policy.license_name,
                        license_url=license_url,
                        api_url=tree_url,
                        url=github_raw_url(KAYKIT_OWNER, repo_name, branch, path),
                        destination=destination,
                        declared_size=size,
                        md5=None,
                        bundle_key=f"kaykit:game-kit:{repo_name}:{path}",
                    )
                )
            if not items:
                raise RuntimeError(f"No downloadable blobs found for {repo_name}")
            metadata = {
                "title": repo_name,
                "description": repo.get("description") or "",
                "tags": re.split(r"[-_.\s]+", repo_name) + ["kaykit", "game", "kit", "3d"],
            }
            candidates.append(
                Candidate(
                    source=policy.source,
                    asset_id=repo_name,
                    asset_type="game-kit",
                    title=str(repo.get("description") or repo_name),
                    source_url=html_url,
                    license_name=policy.license_name,
                    license_url=license_url,
                    score=score_asset(repo_name, metadata) + 20,
                    tags=tuple(str(tag) for tag in metadata["tags"] if str(tag)),
                    items=tuple(items),
                )
            )
        except Exception as exc:
            rejections.append(
                {
                    "source": policy.source,
                    "asset_id": repo_name,
                    "asset_type": "game-kit",
                    "title": repo_name,
                    "reason": str(exc),
                    "requested_profile": "official GitHub repo + README CC0 marker + LICENSE.txt CC0 marker + untruncated tree",
                }
            )
    logging.info("kaykit game-kit selected %s of %s configured repos", len(candidates), len(selected_repos))
    return candidates, rejections

def discover_quaternius_drive_packs(user_agent: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    policy = source_policy("quaternius")
    packs: list[dict[str, Any]] = []
    rejections: list[dict[str, Any]] = []
    homepage_html = fetch_text(QUATERNIUS_BASE_URL, user_agent)
    page_urls = pack_page_urls(homepage_html)
    if not page_urls:
        raise RuntimeError(f"No official Quaternius pack pages found at {QUATERNIUS_BASE_URL}")

    seen_drive_ids: set[str] = set()
    for page_url in page_urls:
        slug = pack_slug(page_url)
        try:
            page_html = fetch_text(page_url, user_agent)
            title = pack_title(page_html, page_url)
            if not has_license_marker(page_html):
                raise ValueError("official pack page did not contain CC0 or free-commercial-use marker")
            folder_ids = drive_folder_ids(page_html)
            if not folder_ids:
                raise ValueError("official pack page did not expose a public Google Drive folder")
            if len(folder_ids) != 1:
                raise ValueError(f"official pack page exposed multiple Google Drive folders: {folder_ids}")
            drive_folder_id = folder_ids[0]
            if drive_folder_id in seen_drive_ids:
                raise ValueError(f"Google Drive folder {drive_folder_id} was already claimed by another pack")
            seen_drive_ids.add(drive_folder_id)
            packs.append(
                {
                    "slug": slug,
                    "title": title,
                    "page_url": page_url,
                    "drive_folder_id": drive_folder_id,
                    "tags": pack_tags(slug, title),
                    "description": html_text(page_html),
                }
            )
        except Exception as exc:
            rejections.append(
                {
                    "source": policy.source,
                    "asset_id": slug,
                    "asset_type": "game-kit",
                    "title": slug,
                    "reason": str(exc),
                    "requested_profile": "official Quaternius page license marker + single public Google Drive folder",
                }
            )
    logging.info("quaternius discovery accepted %s of %s official pack pages", len(packs), len(page_urls))
    return sorted(packs, key=lambda pack: str(pack["slug"])), rejections

def quaternius_candidate_from_pack(
    *,
    pack: dict[str, Any],
    policy: SourcePolicy,
    user_agent: str,
    drive_depth: int,
    metadata_workers: int,
) -> Candidate:
    slug = str(pack["slug"])
    title = str(pack["title"])
    page_url = str(pack["page_url"])
    drive_folder_id = str(pack["drive_folder_id"])
    page_html = fetch_text(page_url, user_agent)
    page_text = html_text(page_html)
    if not has_license_marker(page_html):
        raise ValueError("official pack page did not contain CC0 or free-commercial-use marker")
    drive_files = collect_google_drive_files(
        folder_id=drive_folder_id,
        user_agent=user_agent,
        max_depth=drive_depth,
        metadata_workers=metadata_workers,
    )
    items: list[DownloadItem] = []
    seen_destinations: set[str] = set()
    drive_url = google_drive_folder_url(drive_folder_id)
    for drive_file in drive_files:
        relative_path = safe_rel_path("/".join(drive_file.path_parts))
        destination = Path("sources", "quaternius", "game-kit", sanitize_component(slug), relative_path)
        destination_key = destination.as_posix().lower()
        if destination_key in seen_destinations:
            raise RuntimeError(f"Sanitized path collision in {title}: {'/'.join(drive_file.path_parts)}")
        seen_destinations.add(destination_key)
        items.append(
            DownloadItem(
                source=policy.source,
                asset_id=slug,
                asset_type="game-kit",
                title=title,
                source_url=page_url,
                license_name=policy.license_name,
                license_url=page_url,
                api_url=drive_url,
                url=drive_file.url,
                destination=destination,
                declared_size=drive_file.declared_size,
                md5=None,
                bundle_key=f"quaternius:game-kit:{slug}:{'/'.join(drive_file.path_parts)}",
            )
        )
    if not items:
        raise RuntimeError(f"No downloadable Google Drive files found for {title}")
    metadata = {
        "title": title,
        "description": str(pack.get("description") or page_text),
        "tags": list(pack.get("tags", ())) + ["quaternius", "game", "kit", "3d"],
    }
    return Candidate(
        source=policy.source,
        asset_id=slug,
        asset_type="game-kit",
        title=title,
        source_url=page_url,
        license_name=policy.license_name,
        license_url=page_url,
        score=score_asset(slug, metadata) + 25,
        tags=tuple(str(tag) for tag in metadata["tags"] if str(tag)),
        items=tuple(items),
    )

def quaternius_candidates(args: argparse.Namespace, user_agent: str) -> tuple[list[Candidate], list[dict[str, Any]]]:
    policy = source_policy("quaternius")
    candidates: list[Candidate] = []
    discovered_packs, rejections = discover_quaternius_drive_packs(user_agent)
    requested_slugs = tuple(dict.fromkeys(args.quaternius_pack_slug or ()))
    if requested_slugs:
        discovered_by_slug = {str(pack["slug"]): pack for pack in discovered_packs}
        missing = sorted(slug for slug in requested_slugs if slug not in discovered_by_slug)
        if missing:
            raise RuntimeError(f"Requested Quaternius pack slug(s) were not discovered: {', '.join(missing)}")
        selected_packs = [discovered_by_slug[slug] for slug in requested_slugs]
    elif args.quaternius_pack_limit is None:
        selected_packs = discovered_packs
    else:
        selected_packs = discovered_packs[: args.quaternius_pack_limit]
    if not selected_packs:
        logging.info("quaternius game-kit selected 0 of 0 discovered packs")
        return candidates, rejections

    pack_workers = min(args.quaternius_pack_workers, len(selected_packs))
    per_pack_metadata_workers = max(1, args.drive_metadata_workers // pack_workers)
    logging.info(
        "quaternius walking %s packs with %s pack workers and %s metadata workers per pack",
        len(selected_packs),
        pack_workers,
        per_pack_metadata_workers,
    )
    with concurrent.futures.ThreadPoolExecutor(max_workers=pack_workers) as executor:
        future_to_pack = {
            executor.submit(
                quaternius_candidate_from_pack,
                pack=pack,
                policy=policy,
                user_agent=user_agent,
                drive_depth=args.quaternius_drive_depth,
                metadata_workers=per_pack_metadata_workers,
            ): pack
            for pack in selected_packs
        }
        for future in concurrent.futures.as_completed(future_to_pack):
            pack = future_to_pack[future]
            slug = str(pack["slug"])
            title = str(pack["title"])
            try:
                candidate = future.result()
            except Exception as exc:
                logging.warning("quaternius pack rejected %s: %s", slug, exc)
                rejections.append(
                    {
                        "source": policy.source,
                        "asset_id": slug,
                        "asset_type": "game-kit",
                        "title": title,
                        "reason": str(exc),
                        "requested_profile": "official Quaternius page license marker + public Google Drive files with attachment Content-Length",
                    }
                )
                continue
            candidates.append(candidate)
            logging.info("quaternius pack accepted %s: %s files", candidate.asset_id, len(candidate.items))
    candidates.sort(key=lambda candidate: candidate.asset_id)
    logging.info("quaternius game-kit selected %s of %s discovered packs", len(candidates), len(selected_packs))
    return candidates, rejections
