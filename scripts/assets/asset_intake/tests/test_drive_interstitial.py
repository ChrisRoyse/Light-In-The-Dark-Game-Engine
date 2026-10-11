"""Unit tests for Google Drive virus-scan interstitial handling (issue #1593).

The fixture drive_interstitial_download_form.html is a faithful replica of the
real modern Drive interstitial (the drive.usercontent.google.com #download-form
carrying id/export/confirm/uuid) that gdown parses and that net.py mirrors. The
drive_error_no_confirm.html fixture is the real over-quota / access-denied page
(uc-error-subcaption) that MUST fail loud, never be treated as 'file absent'.
"""

from __future__ import annotations

import os
import sys
import unittest
import urllib.parse

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from asset_intake import net  # noqa: E402

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")


def load(name: str) -> str:
    with open(os.path.join(FIXTURES, name), encoding="utf-8") as handle:
        return handle.read()


class DownloadFormTests(unittest.TestCase):
    def setUp(self) -> None:
        self.html = load("drive_interstitial_download_form.html")

    def test_extract_form_url_targets_usercontent_with_all_hidden_inputs(self) -> None:
        url = net._extract_download_form_url(self.html)
        self.assertIsNotNone(url)
        parts = urllib.parse.urlsplit(url)
        self.assertEqual(parts.netloc, "drive.usercontent.google.com")
        self.assertEqual(parts.path, "/download")
        params = urllib.parse.parse_qs(parts.query)
        self.assertEqual(params["id"], ["17O-EXAMPLEFILEID9xQ"])
        self.assertEqual(params["export"], ["download"])
        self.assertEqual(params["confirm"], ["t"])
        self.assertEqual(params["uuid"], ["0f3e8a1c-2b4d-4e6f-9a1b-abcdef012345"])

    def test_url_from_confirmation_returns_form_url(self) -> None:
        url = net.url_from_gdrive_confirmation(self.html, "17O-EXAMPLEFILEID9xQ", context="ultimatemodularwomen")
        self.assertIn("drive.usercontent.google.com/download", url)
        self.assertIn("confirm=t", url)


class InterstitialFailLoudTests(unittest.TestCase):
    def test_over_quota_page_raises_with_reason(self) -> None:
        html = load("drive_error_no_confirm.html")
        self.assertIsNone(net._extract_download_form_url(html))
        with self.assertRaises(ValueError) as ctx:
            net.url_from_gdrive_confirmation(html, "1C93PRIVATEID", context="ultimateplatformer")
        message = str(ctx.exception)
        self.assertIn("no confirm-download link", message)
        self.assertIn("1C93PRIVATEID", message)
        self.assertIn("ultimateplatformer", message)
        # The Drive-reported reason must be surfaced, not swallowed.
        self.assertIn("Too many users", message)

    def test_empty_html_still_fails_loud(self) -> None:
        with self.assertRaises(ValueError) as ctx:
            net.url_from_gdrive_confirmation("<html></html>", "1AHkMISSING", context="sushirestaurantkit")
        self.assertIn("1AHkMISSING", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
