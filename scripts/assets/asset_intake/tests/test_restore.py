"""Unit tests for targeted vault restore + on-disk verification (issue #1596)."""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from asset_intake import restore  # noqa: E402
from asset_intake.models import Candidate, DownloadItem  # noqa: E402


def make_item(dest: str, size: int) -> DownloadItem:
    return DownloadItem(
        source="polyhaven",
        asset_id="rustig_koppie_puresky",
        asset_type="hdri",
        title="t",
        source_url="https://polyhaven.com/a/x",
        license_name="CC0-1.0",
        license_url="https://polyhaven.com/license",
        api_url="https://polyhaven.com/our-api",
        url="https://example.test/file",
        destination=Path(dest),
        declared_size=size,
        md5=None,
        bundle_key="polyhaven:hdri:x:1k:hdr",
    )


def make_candidate(item: DownloadItem) -> Candidate:
    return Candidate(
        source=item.source,
        asset_id=item.asset_id,
        asset_type=item.asset_type,
        title=item.title,
        source_url=item.source_url,
        license_name=item.license_name,
        license_url=item.license_url,
        score=0,
        tags=(),
        items=(item,),
    )


class ParseTargetedListTests(unittest.TestCase):
    def _write(self, tmp: str, payload) -> Path:
        path = Path(tmp) / "targets.json"
        path.write_text(json.dumps(payload), encoding="utf-8")
        return path

    def test_string_and_dict_forms(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, ["polyhaven:aaa", {"source": "ambientcg", "asset_id": "Bricks075A"}])
            self.assertEqual(
                restore.parse_targeted_list(path),
                [("polyhaven", "aaa"), ("ambientcg", "Bricks075A")],
            )

    def test_unknown_source_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, ["bogus:aaa"])
            with self.assertRaises(ValueError):
                restore.parse_targeted_list(path)

    def test_duplicate_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, ["polyhaven:aaa", "polyhaven:aaa"])
            with self.assertRaises(ValueError):
                restore.parse_targeted_list(path)

    def test_missing_field_and_bad_form(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                restore.parse_targeted_list(self._write(tmp, ["polyhaven:"]))
            with self.assertRaises(ValueError):
                restore.parse_targeted_list(self._write(tmp, ["noseparator"]))
            with self.assertRaises(ValueError):
                restore.parse_targeted_list(self._write(tmp, []))


class VerifyRestoreOnDiskTests(unittest.TestCase):
    def _prepare(self, tmp: str, payload: bytes):
        vault = Path(tmp)
        rel = "sources/polyhaven/hdris/x/1k/x_1k.hdr"
        target = vault / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(payload)
        sha = hashlib.sha256(payload).hexdigest()
        item = make_item(rel, len(payload))
        candidate = make_candidate(item)
        record = {"bundle_key": item.bundle_key, "relative_path": rel, "sha256": sha}
        return vault, candidate, record

    def test_happy_path_verifies(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            payload = b"REAL-HDR-BYTES" * 100
            vault, candidate, record = self._prepare(tmp, payload)
            verified = restore.verify_restore_on_disk(vault, [candidate], [record])
            self.assertEqual(verified, 1)

    def test_missing_record_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            payload = b"x" * 50
            vault, candidate, _record = self._prepare(tmp, payload)
            with self.assertRaises(RuntimeError) as ctx:
                restore.verify_restore_on_disk(vault, [candidate], [])
            self.assertIn("no manifest download record", str(ctx.exception))

    def test_absent_file_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            payload = b"x" * 50
            vault, candidate, record = self._prepare(tmp, payload)
            (vault / candidate.items[0].destination).unlink()
            with self.assertRaises(RuntimeError) as ctx:
                restore.verify_restore_on_disk(vault, [candidate], [record])
            self.assertIn("file absent on disk", str(ctx.exception))

    def test_sha_mismatch_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            payload = b"x" * 50
            vault, candidate, record = self._prepare(tmp, payload)
            record["sha256"] = "0" * 64  # manifest claims a different hash than disk
            with self.assertRaises(RuntimeError) as ctx:
                restore.verify_restore_on_disk(vault, [candidate], [record])
            self.assertIn("sha256 mismatch", str(ctx.exception))


class BuildTargetedCandidatesTests(unittest.TestCase):
    def test_unsupported_source_fails_loud(self) -> None:
        with self.assertRaises(NotImplementedError) as ctx:
            restore.build_targeted_candidates([("quaternius", "medievaldungeon")], "ua")
        self.assertIn("quaternius", str(ctx.exception))


class FloorCatalogTests(unittest.TestCase):
    def _candidate(self, source: str, asset_id: str, asset_type: str, dest: str, size: int) -> Candidate:
        return make_candidate(
            DownloadItem(
                source=source,
                asset_id=asset_id,
                asset_type=asset_type,
                title=asset_id.replace("_", " "),
                source_url="https://example.test/a",
                license_name="CC0-1.0",
                license_url="https://example.test/l",
                api_url="https://example.test/api",
                url="https://example.test/file",
                destination=Path(dest),
                declared_size=size,
                md5=None,
                bundle_key=f"{source}:{asset_type}:{asset_id}",
            )
        )

    def test_catalog_line_shape(self) -> None:
        cand = self._candidate("polyhaven", "castle", "model", "sources/ph/castle/c_1k.gltf", 4000)
        line = restore.candidate_catalog_line(cand)
        self.assertEqual(
            line,
            {
                "source": "polyhaven",
                "asset_id": "castle",
                "asset_type": "model",
                "title": "castle",
                "relative_path": "sources/ph/castle/c_1k.gltf",
                "declared_size": 4000,
            },
        )

    def test_catalog_line_no_items_fails_loud(self) -> None:
        empty = Candidate(
            source="polyhaven",
            asset_id="x",
            asset_type="model",
            title="x",
            source_url="u",
            license_name="CC0-1.0",
            license_url="l",
            score=0,
            tags=(),
            items=(),
        )
        with self.assertRaises(ValueError):
            restore.candidate_catalog_line(empty)

    def test_write_catalog_manifest_roundtrips(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cands = [
                self._candidate("polyhaven", "castle", "model", "sources/a/c_1k.gltf", 4000),
                self._candidate("ambientcg", "Rock035", "material", "sources/b/r_1k.jpg", 1500),
            ]
            path = Path(tmp) / "catalog.jsonl"
            count = restore.write_catalog_manifest(cands, path)
            self.assertEqual(count, 2)
            lines = [json.loads(row) for row in path.read_text(encoding="utf-8").splitlines() if row.strip()]
            self.assertEqual(len(lines), 2)
            self.assertEqual(lines[1]["asset_id"], "Rock035")

    def test_select_floor_candidates_happy(self) -> None:
        cands = [
            self._candidate("polyhaven", "castle", "model", "sources/a/c.gltf", 4000),
            self._candidate("ambientcg", "Rock035", "material", "sources/b/r.jpg", 1500),
        ]
        selection = {"selected": [{"source": "ambientcg", "asset_id": "Rock035"}]}
        chosen = restore.select_floor_candidates(cands, selection)
        self.assertEqual([c.asset_id for c in chosen], ["Rock035"])

    def test_select_floor_candidates_empty_fails_loud(self) -> None:
        with self.assertRaises(RuntimeError) as ctx:
            restore.select_floor_candidates([], {"selected": []})
        self.assertIn("empty selection", str(ctx.exception))

    def test_select_floor_candidates_missing_fails_loud(self) -> None:
        cands = [self._candidate("polyhaven", "castle", "model", "sources/a/c.gltf", 4000)]
        selection = {"selected": [{"source": "polyhaven", "asset_id": "ghost"}]}
        with self.assertRaises(RuntimeError) as ctx:
            restore.select_floor_candidates(cands, selection)
        self.assertIn("absent from the catalog", str(ctx.exception))

    def test_default_selector_bin_missing_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(RuntimeError) as ctx:
                restore.default_floor_selector_bin(Path(tmp))
            self.assertIn("not built", str(ctx.exception))
            self.assertIn("litd-editor-asset-floor-select", str(ctx.exception))

    def test_default_selector_bin_resolves_when_present(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            exe = restore.FLOOR_SELECTOR_BIN + (".exe" if os.name == "nt" else "")
            target = Path(tmp) / "target" / "debug" / exe
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text("stub", encoding="utf-8")
            self.assertEqual(restore.default_floor_selector_bin(Path(tmp)), target)

    def test_run_floor_selector_missing_binary_fails_loud(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            catalog = Path(tmp) / "catalog.jsonl"
            catalog.write_text("{}\n", encoding="utf-8")
            missing = Path(tmp) / "definitely-not-here-floor-select"
            with self.assertRaises(RuntimeError) as ctx:
                restore.run_floor_selector(missing, catalog, [])
            self.assertIn("not found", str(ctx.exception))


if __name__ == "__main__":
    unittest.main()
