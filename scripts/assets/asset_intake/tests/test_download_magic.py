"""Unit tests for byte-level magic verification of downloaded payloads (issue #1593).

A Google Drive large pack that fails the virus-scan confirm flow lands as an HTML
interstitial page saved in place of the archive. The interstitial can match the
declared byte size yet is NOT the binary, so ``verify_download_magic`` refuses to
record it as ingested. These tests exercise every branch against REAL bytes on
disk (HTML interstitial, valid ZIP, truncated non-ZIP, small non-Drive file).
"""

from __future__ import annotations

import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from asset_intake import storage  # noqa: E402


def _write(tmp: str, name: str, payload: bytes) -> Path:
    path = Path(tmp) / name
    path.write_bytes(payload)
    return path


class VerifyDownloadMagicTests(unittest.TestCase):
    def test_drive_html_interstitial_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            part = _write(tmp, "ultimatemodularwomen.zip.part", b"<!DOCTYPE html><html><head>Google Drive")
            with self.assertRaises(RuntimeError) as ctx:
                storage.verify_download_magic(
                    part, Path("ultimatemodularwomen.zip"), is_drive=True, context="quaternius:ultimatemodularwomen"
                )
            message = str(ctx.exception)
            self.assertIn("is HTML", message)
            self.assertIn("virus-scan interstitial", message)
            self.assertIn("ultimatemodularwomen", message)

    def test_drive_valid_zip_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            part = _write(tmp, "pack.zip.part", b"PK\x03\x04" + b"\x00" * 64)
            # No raise == accepted.
            storage.verify_download_magic(part, Path("pack.zip"), is_drive=True, context="quaternius:pack")

    def test_zip_extension_without_pk_magic_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            part = _write(tmp, "corrupt.zip.part", b"not-a-zip-file-at-all-just-bytes")
            with self.assertRaises(RuntimeError) as ctx:
                storage.verify_download_magic(
                    part, Path("corrupt.zip"), is_drive=False, context="kenney:corrupt"
                )
            self.assertIn("not a valid ZIP archive", str(ctx.exception))
            self.assertIn("kenney:corrupt", str(ctx.exception))

    def test_non_drive_non_zip_is_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            # A .gltf from polyhaven is not Drive and not a zip: no magic contract, no raise.
            part = _write(tmp, "model.gltf.part", b"<html>this would be caught only for drive")
            storage.verify_download_magic(part, Path("model.gltf"), is_drive=False, context="polyhaven:model")

    def test_drive_empty_zip_end_of_central_dir_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            # PK\x05\x06 is the empty-archive end-of-central-directory magic; still a real zip.
            part = _write(tmp, "empty.zip.part", b"PK\x05\x06" + b"\x00" * 18)
            storage.verify_download_magic(part, Path("empty.zip"), is_drive=True, context="quaternius:empty")


if __name__ == "__main__":
    unittest.main()
