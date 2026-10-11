"""Unit tests for the transient-retry/backoff logic (issue #1591).

These use REAL urllib.error.HTTPError objects -- the exact exception type urllib
raises on an HTTP 503/404 -- not an invented mock, so the classification path is
exercised against the true error shape. A companion live-network FSV harness
(fsv_asset_robustness.py) induces a genuine httpbin 503 to prove end-to-end.
"""

from __future__ import annotations

import email.message
import io
import os
import sys
import unittest
import urllib.error

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from asset_intake import net  # noqa: E402


def http_error(code: int, reason: str, retry_after: str | None = None) -> urllib.error.HTTPError:
    headers = email.message.Message()
    if retry_after is not None:
        headers["Retry-After"] = retry_after
    return urllib.error.HTTPError("https://example.test/x", code, reason, headers, io.BytesIO(b""))


class ClassifyTransientTests(unittest.TestCase):
    def test_5xx_and_429_and_408_are_transient(self) -> None:
        for code in (408, 429, 500, 502, 503, 504):
            transient, status = net._classify_transient(http_error(code, "x"))
            self.assertTrue(transient, code)
            self.assertEqual(status, code)

    def test_4xx_permanent_not_transient(self) -> None:
        for code in (400, 401, 403, 404):
            transient, status = net._classify_transient(http_error(code, "x"))
            self.assertFalse(transient, code)
            self.assertEqual(status, code)

    def test_network_faults_are_transient(self) -> None:
        for exc in (urllib.error.URLError("reset"), TimeoutError("slow"), ConnectionResetError("boom")):
            transient, status = net._classify_transient(exc)
            self.assertTrue(transient)
            self.assertIsNone(status)


class RetryAfterTests(unittest.TestCase):
    def test_integer_seconds(self) -> None:
        self.assertEqual(net._parse_retry_after("120"), 120.0)

    def test_http_date_future(self) -> None:
        # A far-future HTTP-date must parse to a positive delay.
        delay = net._parse_retry_after("Wed, 21 Oct 2099 07:28:00 GMT")
        self.assertIsNotNone(delay)
        self.assertGreater(delay, 0.0)

    def test_absent_and_garbage(self) -> None:
        self.assertIsNone(net._parse_retry_after(None))
        self.assertIsNone(net._parse_retry_after("not-a-date"))

    def test_backoff_honors_retry_after(self) -> None:
        self.assertEqual(net._backoff_delay(1, 3.0), 3.0)
        self.assertEqual(net._backoff_delay(5, 999.0), net._RETRY_AFTER_CAP)

    def test_backoff_jitter_bounded_by_ceiling(self) -> None:
        for attempt in range(1, 6):
            ceiling = min(net._RETRY_BASE_DELAY * (2 ** (attempt - 1)), net._RETRY_MAX_DELAY)
            for _ in range(50):
                delay = net._backoff_delay(attempt, None)
                self.assertGreaterEqual(delay, 0.0)
                self.assertLessEqual(delay, ceiling)


class RetryTransientTests(unittest.TestCase):
    def setUp(self) -> None:
        self.slept: list[float] = []

    def _sleep(self, seconds: float) -> None:
        self.slept.append(seconds)

    def test_transient_then_success(self) -> None:
        calls = {"n": 0}

        def op() -> str:
            calls["n"] += 1
            if calls["n"] < 3:
                raise http_error(503, "Service Unavailable")
            return "ok"

        result = net.retry_transient(op, description="GET x", max_attempts=4, sleep=self._sleep)
        self.assertEqual(result, "ok")
        self.assertEqual(calls["n"], 3)
        self.assertEqual(len(self.slept), 2)  # slept before attempts 2 and 3

    def test_permanent_404_not_retried(self) -> None:
        calls = {"n": 0}

        def op() -> str:
            calls["n"] += 1
            raise http_error(404, "Not Found")

        with self.assertRaises(urllib.error.HTTPError) as ctx:
            net.retry_transient(op, description="GET x", max_attempts=4, sleep=self._sleep)
        self.assertEqual(ctx.exception.code, 404)
        self.assertEqual(calls["n"], 1)  # no wasted retries
        self.assertEqual(self.slept, [])

    def test_persistent_503_exhausts_loud(self) -> None:
        calls = {"n": 0}

        def op() -> str:
            calls["n"] += 1
            raise http_error(503, "Service Unavailable")

        with self.assertRaises(RuntimeError) as ctx:
            net.retry_transient(op, description="GET https://pack.test/medievaldungeon", max_attempts=3, sleep=self._sleep)
        message = str(ctx.exception)
        self.assertIn("after 3 attempts", message)
        self.assertIn("HTTP 503", message)
        self.assertIn("medievaldungeon", message)
        self.assertEqual(calls["n"], 3)

    def test_retry_after_used_for_delay(self) -> None:
        def op() -> str:
            raise http_error(429, "Too Many Requests", retry_after="7")

        with self.assertRaises(RuntimeError):
            net.retry_transient(op, description="GET x", max_attempts=2, sleep=self._sleep)
        self.assertEqual(self.slept, [7.0])


if __name__ == "__main__":
    unittest.main()
