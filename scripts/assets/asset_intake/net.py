from __future__ import annotations

import concurrent.futures
import email.utils
import html as html_lib
import http.cookiejar
import json
import random
import re
import socket
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from typing import Any, Callable, TypeVar

from .models import DriveEntry, DriveFile
from .utils import sanitize_component


def fetch_json(url: str, user_agent: str, timeout: int = 60, max_attempts: int = 4) -> Any:
    def _op() -> bytes:
        request = urllib.request.Request(url, headers={"User-Agent": user_agent})
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.read()

    data = retry_transient(_op, description=f"GET {url}", max_attempts=max_attempts)
    return json.loads(data.decode("utf-8"))

def fetch_text(url: str, user_agent: str, timeout: int = 60, max_attempts: int = 4) -> str:
    def _op() -> bytes:
        request = urllib.request.Request(url, headers={"User-Agent": user_agent})
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.read()

    data = retry_transient(_op, description=f"GET {url}", max_attempts=max_attempts)
    return data.decode("utf-8", errors="replace")

class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[no-untyped-def]
        return None

def encode_url_path(url: str) -> str:
    parts = urllib.parse.urlsplit(url)
    path = urllib.parse.quote(urllib.parse.unquote(parts.path), safe="/%")
    query = urllib.parse.quote(urllib.parse.unquote(parts.query), safe="=&%")
    return urllib.parse.urlunsplit((parts.scheme, parts.netloc, path, query, parts.fragment))

def resolve_single_redirect(url: str, user_agent: str, timeout: int = 60) -> str:
    opener = urllib.request.build_opener(NoRedirectHandler)
    request = urllib.request.Request(url, headers={"User-Agent": user_agent})
    try:
        with opener.open(request, timeout=timeout):
            return url
    except urllib.error.HTTPError as exc:
        if 300 <= exc.code < 400:
            location = exc.headers.get("Location")
            if not location:
                raise ValueError(f"Redirect without Location for {url}") from exc
            redirected = urllib.parse.urljoin(url, location)
            return encode_url_path(redirected)
        raise

def declared_content_length(url: str, user_agent: str, timeout: int = 60, max_attempts: int = 4) -> int:
    def _op() -> str | None:
        request = urllib.request.Request(url, headers={"User-Agent": user_agent}, method="HEAD")
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.headers.get("Content-Length")

    value = retry_transient(_op, description=f"HEAD {url}", max_attempts=max_attempts)
    if value is None:
        raise ValueError(f"Server did not provide Content-Length for {url}")
    length = int(value)
    if length <= 0:
        raise ValueError(f"Server returned non-positive Content-Length for {url}: {value}")
    return length

def response_header_filename(disposition: str) -> str | None:
    if not disposition:
        return None
    encoded = re.search(r"filename\*=UTF-8''([^;]+)", disposition, flags=re.IGNORECASE)
    if encoded:
        return sanitize_component(urllib.parse.unquote(encoded.group(1).strip().strip('"')))
    plain = re.search(r'filename="?([^";]+)"?', disposition, flags=re.IGNORECASE)
    if plain:
        return sanitize_component(urllib.parse.unquote(plain.group(1).strip()))
    return None

def google_drive_download_url(file_id: str) -> str:
    return f"https://drive.google.com/uc?export=download&id={urllib.parse.quote(file_id, safe='')}"

def google_drive_folder_url(folder_id: str) -> str:
    return f"https://drive.google.com/drive/folders/{urllib.parse.quote(folder_id, safe='')}?usp=sharing"

def google_drive_file_id_from_url(url: str) -> str | None:
    """Return the Drive file id if url targets a Drive file download, else None."""
    parts = urllib.parse.urlsplit(url)
    if parts.netloc.lower() not in ("drive.google.com", "drive.usercontent.google.com"):
        return None
    ids = urllib.parse.parse_qs(parts.query).get("id")
    if ids and ids[0]:
        return ids[0]
    return None

# Transient failure classes worth retrying (RFC 9110 + real network faults).
# 4xx other than these are permanent client errors and must hard-fail immediately.
TRANSIENT_HTTP_STATUSES = frozenset({408, 429, 500, 502, 503, 504})
_RETRY_BASE_DELAY = 0.5
_RETRY_MAX_DELAY = 30.0
_RETRY_AFTER_CAP = 60.0

T = TypeVar("T")


def _parse_retry_after(value: str | None) -> float | None:
    """Parse a Retry-After header value (delay-seconds OR HTTP-date), else None.

    Per RFC 9110 the server may send either an integer number of seconds or an
    absolute HTTP-date. Returns the delay in seconds (>= 0), or None when the
    header is absent/unparseable so the caller falls back to computed backoff.
    """
    if not value:
        return None
    text = value.strip()
    try:
        return max(0.0, float(int(text)))
    except ValueError:
        pass
    try:
        when = email.utils.parsedate_to_datetime(text)
    except (TypeError, ValueError):
        return None
    if when is None:
        return None
    if when.tzinfo is None:
        when = when.replace(tzinfo=timezone.utc)
    return max(0.0, (when - datetime.now(timezone.utc)).total_seconds())


def _retry_after_from_exc(exc: BaseException) -> float | None:
    if isinstance(exc, urllib.error.HTTPError) and exc.headers is not None:
        return _parse_retry_after(exc.headers.get("Retry-After"))
    return None


def _classify_transient(exc: BaseException) -> tuple[bool, int | None]:
    """Return (is_transient, http_status_if_any) for a raised exception."""
    if isinstance(exc, urllib.error.HTTPError):
        return exc.code in TRANSIENT_HTTP_STATUSES, exc.code
    # Connection resets, DNS blips, read timeouts: transient at the socket layer.
    if isinstance(exc, (urllib.error.URLError, socket.timeout, TimeoutError, ConnectionError)):
        return True, None
    return False, None


def _backoff_delay(attempt: int, retry_after: float | None) -> float:
    """Exponential backoff with full jitter; honor Retry-After when present."""
    if retry_after is not None:
        return min(retry_after, _RETRY_AFTER_CAP)
    ceiling = min(_RETRY_BASE_DELAY * (2 ** (attempt - 1)), _RETRY_MAX_DELAY)
    return random.uniform(0.0, ceiling)


def retry_transient(
    operation: Callable[[], T],
    *,
    description: str,
    max_attempts: int = 4,
    sleep: Callable[[float], None] = time.sleep,
) -> T:
    """Run `operation`, retrying only transient failures with bounded backoff.

    Transient failures (HTTP 408/429/5xx, connection/timeout errors) are retried
    up to `max_attempts` with exponential backoff + jitter, honoring Retry-After.
    Permanent failures (any other exception, e.g. HTTP 404/403) are re-raised
    immediately -- no wasted retries. On exhaustion we raise LOUD naming the
    resource, the attempt count, and the last status/error.
    """
    if max_attempts < 1:
        raise ValueError(f"max_attempts must be >= 1, got {max_attempts}")
    last_exc: BaseException | None = None
    last_status: int | None = None
    for attempt in range(1, max_attempts + 1):
        try:
            return operation()
        except Exception as exc:  # noqa: BLE001 -- classified below, re-raised if permanent
            transient, status = _classify_transient(exc)
            last_exc = exc
            if status is not None:
                last_status = status
            if not transient:
                raise
            if attempt >= max_attempts:
                break
            sleep(_backoff_delay(attempt, _retry_after_from_exc(exc)))
    status_text = f"HTTP {last_status}" if last_status is not None else type(last_exc).__name__
    raise RuntimeError(
        f"Transient request failed after {max_attempts} attempts: {description} "
        f"(last status: {status_text}; last error: {last_exc})"
    ) from last_exc

def _extract_download_form_url(contents: str) -> str | None:
    """Rebuild the large-file download URL from Drive's #download-form.

    Modern Drive serves a virus-scan interstitial whose <form id="download-form">
    action points at drive.usercontent.google.com and whose hidden inputs carry
    id/export/confirm/uuid (and sometimes resourcekey). This mirrors gdown's
    get_url_from_gdrive_confirmation form branch without a bs4 dependency.
    """
    form_match = re.search(r'<form\b[^>]*\bid="download-form"[^>]*>', contents, re.IGNORECASE)
    if not form_match:
        return None
    action_match = re.search(r'action="([^"]+)"', form_match.group(0), re.IGNORECASE)
    if not action_match:
        return None
    action = html_lib.unescape(action_match.group(1))
    body_end = contents.find("</form>", form_match.end())
    body = contents[form_match.end():body_end] if body_end != -1 else contents[form_match.end():]
    components = urllib.parse.urlsplit(action)
    params = urllib.parse.parse_qs(components.query, keep_blank_values=True)
    for input_tag in re.findall(r"<input\b[^>]*>", body, re.IGNORECASE):
        type_match = re.search(r'type="([^"]*)"', input_tag, re.IGNORECASE)
        if type_match and type_match.group(1).lower() != "hidden":
            continue
        name_match = re.search(r'name="([^"]*)"', input_tag, re.IGNORECASE)
        if not name_match:
            continue
        value_match = re.search(r'value="([^"]*)"', input_tag, re.IGNORECASE)
        params[name_match.group(1)] = [html_lib.unescape(value_match.group(1)) if value_match else ""]
    query = urllib.parse.urlencode(params, doseq=True)
    return urllib.parse.urlunsplit(components._replace(query=query))

def url_from_gdrive_confirmation(contents: str, file_id: str, context: str = "") -> str:
    """Extract the confirm-token download URL from a Drive interstitial page.

    Raises ValueError LOUD (with the Drive-reported reason) when no confirm link
    is present -- a private/removed file or a quota block, never a silent skip.
    """
    legacy = re.search(r'href="(/uc\?export=download[^"]+)"', contents)
    if legacy:
        return "https://docs.google.com" + html_lib.unescape(legacy.group(1))
    form_url = _extract_download_form_url(contents)
    if form_url:
        return form_url
    embedded = re.search(r'"downloadUrl":"([^"]+)"', contents)
    if embedded:
        return embedded.group(1).replace("\\u003d", "=").replace("\\u0026", "&")
    error = re.search(r'<p[^>]*class="uc-error-subcaption"[^>]*>(.*?)</p>', contents, re.DOTALL)
    detail = re.sub(r"<[^>]+>", "", error.group(1)).strip() if error else contents[:200].strip()
    raise ValueError(
        f"Google Drive presented no confirm-download link for {context or file_id} "
        f"(id={file_id}); file may be private, removed, or over quota. Drive said: {detail!r}"
    )

def _open_drive_request(
    opener: urllib.request.OpenerDirector,
    url: str,
    user_agent: str,
    timeout: int,
    max_attempts: int = 4,
):
    """Open a Drive URL with bounded transient-retry; permanent HTTP fails LOUD.

    Transient statuses (429/5xx/408) and socket faults are retried with backoff
    by retry_transient. A permanent HTTP error (e.g. 404 private/removed file) is
    converted to a loud ValueError immediately -- no wasted retries.
    """

    def _op():
        request = urllib.request.Request(url, headers={"User-Agent": user_agent})
        try:
            return opener.open(request, timeout=timeout)
        except urllib.error.HTTPError as exc:
            if exc.code in TRANSIENT_HTTP_STATUSES:
                raise
            retry_after = exc.headers.get("Retry-After") if exc.headers else None
            suffix = f" Retry-After={retry_after}" if retry_after else ""
            raise ValueError(
                f"Google Drive request failed for {url}: HTTP {exc.code} {exc.reason}.{suffix}"
            ) from exc

    return retry_transient(_op, description=f"Google Drive request {url}", max_attempts=max_attempts)

def open_google_drive_stream(file_id: str, user_agent: str, timeout: int = 120, context: str = ""):
    """Open a streaming response for a Drive file, completing the large-file
    virus-scan confirm-token flow on the SAME cookie session when needed.

    Small files return the binary on the first request (unchanged happy path).
    Large files return an HTML interstitial; we extract the confirm token /
    resourcekey, re-request on the same session, and verify the second response
    is a real attachment (not HTML) before handing it back. The caller owns the
    returned response and must read + close it.
    """
    url = google_drive_download_url(file_id)
    cookie_jar = http.cookiejar.CookieJar()
    opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cookie_jar))
    response = _open_drive_request(opener, url, user_agent, timeout)
    disposition = response.headers.get("Content-Disposition", "")
    content_type = response.headers.get("Content-Type", "")
    if "attachment" in disposition.lower() and "text/html" not in content_type.lower():
        return response
    try:
        body = response.read().decode("utf-8", errors="replace")
    finally:
        response.close()
    confirm_url = url_from_gdrive_confirmation(body, file_id, context)
    response = _open_drive_request(opener, confirm_url, user_agent, timeout)
    disposition = response.headers.get("Content-Disposition", "")
    content_type = response.headers.get("Content-Type", "")
    if "attachment" not in disposition.lower() or "text/html" in content_type.lower():
        first_bytes = response.read(96)
        response.close()
        raise ValueError(
            f"Google Drive confirm step for {context or file_id} (id={file_id}) STILL returned "
            f"a non-attachment response: Content-Type={content_type!r} "
            f"Content-Disposition={disposition!r} first-bytes={first_bytes!r} confirm_url={confirm_url}"
        )
    return response

def google_drive_file_metadata(file_id: str, fallback_name: str, user_agent: str, timeout: int = 60) -> DriveFile:
    url = google_drive_download_url(file_id)

    def _head() -> tuple[str, str, str | None]:
        request = urllib.request.Request(url, headers={"User-Agent": user_agent}, method="HEAD")
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return (
                response.headers.get("Content-Disposition", ""),
                response.headers.get("Content-Type", ""),
                response.headers.get("Content-Length"),
            )

    disposition, content_type, value = retry_transient(
        _head, description=f"Google Drive HEAD {url} ({fallback_name})"
    )
    is_small_file = (
        "attachment" in disposition.lower()
        and "text/html" not in content_type.lower()
        and value is not None
    )
    if not is_small_file:
        # Large file: HEAD hit the virus-scan interstitial (no attachment). Resolve the
        # real download via the confirm-token flow and read its headers, without pulling
        # the whole body down during discovery.
        stream = open_google_drive_stream(file_id, user_agent, timeout=timeout, context=fallback_name)
        try:
            disposition = stream.headers.get("Content-Disposition", "")
            content_type = stream.headers.get("Content-Type", "")
            value = stream.headers.get("Content-Length")
        finally:
            stream.close()
        if value is None:
            raise ValueError(
                f"Google Drive file {file_id} ({fallback_name}) resolved past the confirm step "
                f"but returned no Content-Length; cannot verify the download size"
            )
    declared_size = int(value)
    if declared_size <= 0:
        raise ValueError(f"Google Drive file {file_id} returned non-positive Content-Length: {value}")
    name = response_header_filename(disposition) or sanitize_component(fallback_name)
    return DriveFile(
        file_id=file_id,
        name=name,
        path_parts=(name,),
        url=url,
        declared_size=declared_size,
        content_type=content_type,
    )

def google_drive_entry_from_tooltip(file_id: str, tooltip: str) -> DriveEntry | None:
    tooltip = html_lib.unescape(tooltip).strip()
    suffixes = (
        (" Compressed Archive", "file"),
        (" Shared folder", "folder"),
        (" Binary", "file"),
        (" Image", "file"),
        (" Text", "file"),
        (" PDF", "file"),
        (" Video", "file"),
        (" Audio", "file"),
        (" Archive", "file"),
        (" Unknown", "file"),
    )
    for suffix, kind in suffixes:
        if tooltip.endswith(suffix):
            name = tooltip[: -len(suffix)].strip()
            if name:
                return DriveEntry(file_id=file_id, name=name, kind=kind)
    return None

def parse_google_drive_entries(html_text: str) -> list[DriveEntry]:
    entries: list[DriveEntry] = []
    seen: set[str] = set()
    pattern = re.compile(r'data-id="([A-Za-z0-9_-]{20,})"[^>]*data-tooltip="([^"]+)"')
    for match in pattern.finditer(html_text):
        file_id = match.group(1)
        if file_id in seen:
            continue
        entry = google_drive_entry_from_tooltip(file_id, match.group(2))
        if entry is None:
            continue
        entries.append(entry)
        seen.add(file_id)
    return sorted(entries, key=lambda entry: (entry.kind != "folder", entry.name.lower(), entry.file_id))

def collect_google_drive_files(
    *,
    folder_id: str,
    user_agent: str,
    path_parts: tuple[str, ...] = (),
    depth: int = 0,
    max_depth: int = 4,
    metadata_workers: int = 8,
) -> list[DriveFile]:
    if depth > max_depth:
        raise RuntimeError(f"Google Drive recursion exceeded max depth {max_depth} at folder {folder_id}")
    folder_url = google_drive_folder_url(folder_id)
    html_text = fetch_text(folder_url, user_agent)
    entries = parse_google_drive_entries(html_text)
    if not entries:
        raise RuntimeError(f"No Google Drive entries found in {folder_url}")
    files: list[DriveFile] = []
    file_entries: list[tuple[DriveEntry, tuple[str, ...]]] = []
    for entry in entries:
        safe_name = sanitize_component(entry.name)
        if entry.kind == "folder":
            files.extend(
                collect_google_drive_files(
                    folder_id=entry.file_id,
                    user_agent=user_agent,
                    path_parts=path_parts + (safe_name,),
                    depth=depth + 1,
                    max_depth=max_depth,
                    metadata_workers=metadata_workers,
                )
            )
            continue
        file_entries.append((entry, path_parts))

    def metadata_for_entry(entry_with_path: tuple[DriveEntry, tuple[str, ...]]) -> DriveFile:
        entry, parent_parts = entry_with_path
        metadata = google_drive_file_metadata(entry.file_id, entry.name, user_agent)
        return DriveFile(
            file_id=metadata.file_id,
            name=metadata.name,
            path_parts=parent_parts + (metadata.name,),
            url=metadata.url,
            declared_size=metadata.declared_size,
            content_type=metadata.content_type,
        )

    if file_entries:
        workers = max(1, min(metadata_workers, len(file_entries)))
        with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as executor:
            for drive_file in executor.map(metadata_for_entry, file_entries):
                files.append(drive_file)
    return sorted(files, key=lambda drive_file: "/".join(drive_file.path_parts).lower())
