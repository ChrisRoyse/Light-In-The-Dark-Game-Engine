"""Parse official Quaternius pack pages for strict asset intake."""

from __future__ import annotations

import html
import posixpath
import re
import urllib.parse


QUATERNIUS_BASE_URL = "https://quaternius.com/"

PACK_HREF_RE = re.compile(r"""href=["']([^"']*packs/[^"']+\.html)["']""", re.IGNORECASE)
DRIVE_FOLDER_RE = re.compile(
    r"""drive\.google\.com/(?:drive/(?:u/\d+/)?folders/|open\?id=|uc\?[^"'\s<>]*id=)([A-Za-z0-9_-]{20,})""",
    re.IGNORECASE,
)
TITLE_RE = re.compile(r"<title[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)


def html_text(markup: str) -> str:
    stripped = re.sub(r"<[^>]+>", " ", markup)
    return html.unescape(re.sub(r"\s+", " ", stripped)).strip()


def has_license_marker(markup: str) -> bool:
    text = html_text(markup).lower()
    return "cc0" in text or "free to use in personal and commercial projects" in text


def pack_page_urls(homepage_html: str) -> list[str]:
    urls: set[str] = set()
    for match in PACK_HREF_RE.finditer(homepage_html):
        href = html.unescape(match.group(1).strip())
        url = urllib.parse.urljoin(QUATERNIUS_BASE_URL, href)
        urls.add(url)
    return sorted(urls)


def drive_folder_ids(pack_html: str) -> list[str]:
    ids: set[str] = set()
    for match in DRIVE_FOLDER_RE.finditer(pack_html):
        ids.add(match.group(1))
    return sorted(ids)


def pack_slug(page_url: str) -> str:
    path = urllib.parse.urlparse(page_url).path
    stem = posixpath.basename(path).removesuffix(".html")
    slug = re.sub(r"[^a-z0-9]+", "-", stem.lower()).strip("-")
    if not slug:
        raise ValueError(f"Could not derive Quaternius pack slug from {page_url}")
    return slug


def pack_title(pack_html: str, page_url: str) -> str:
    match = TITLE_RE.search(pack_html)
    if match:
        title = html_text(match.group(1)).replace("Quaternius •", "").strip()
        if title:
            return title
    words = pack_slug(page_url).replace("-", " ")
    return words.title()


def pack_tags(slug: str, title: str) -> tuple[str, ...]:
    raw = re.split(r"[-_.\s]+", f"{slug} {title}")
    tags = [tag.lower() for tag in raw if tag]
    tags.extend(["quaternius", "game", "kit", "3d"])
    return tuple(dict.fromkeys(tags))
