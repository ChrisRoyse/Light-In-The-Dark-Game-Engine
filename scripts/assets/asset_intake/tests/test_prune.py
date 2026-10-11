"""Unit tests for the fail-loud atomic vault prune (issue #1821).

These build a REAL miniature vault on disk (real files + real manifest JSONL) and
exercise prune end to end: the happy path (files + records removed, receipts
purged, summary regenerated), the guard/abort paths (fixture, outside-vault,
untracked, missing), and the atomicity guarantee (a rejected prune leaves the
vault byte-for-byte unchanged).
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

from asset_intake import prune  # noqa: E402


def _download_record(rel: str, sha: str, size: int, bundle: str) -> dict:
    return {
        "record_type": "download",
        "source": "quaternius",
        "asset_id": "demo",
        "asset_type": "3d-model",
        "title": "demo",
        "relative_path": rel,
        "bundle_key": bundle,
        "sha256": sha,
        "bytes_on_disk": size,
    }


def _extraction_record(rel: str, sha: str, size: int, bundle: str) -> dict:
    return {
        "record_type": "extraction",
        "source": "quaternius",
        "asset_id": "demo",
        "asset_type": "3d-model",
        "title": "demo",
        "relative_path": rel,
        "bundle_key": bundle,
        "sha256": sha,
        "bytes_on_disk": size,
    }


def _make_vault(tmp: str) -> Path:
    """Build a real minimal vault: sources/ + extracted/ + manifests/ + receipts."""
    vault = Path(tmp) / "local-asset-vault"
    (vault / "manifests").mkdir(parents=True)
    (vault / "logs").mkdir()
    cache = vault / ".litd-verify-cache"
    cache.mkdir()
    (cache / "abc123.receipt").write_text("abc123", encoding="utf-8")

    files = {
        "sources/quaternius/game-kit/demo/dup.fbx": b"FBX-DUP-BYTES" * 8,
        "sources/quaternius/game-kit/demo/keep.gltf": b"KEEP-GLTF" * 8,
        "extracted/quaternius/game-kit/demo/Stan.gltf": b"STAN-FIXTURE" * 8,
    }
    download = []
    extraction = []
    for rel, payload in files.items():
        path = vault / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(payload)
        sha = hashlib.sha256(payload).hexdigest()
        if rel.startswith("sources/"):
            download.append(_download_record(rel, sha, len(payload), f"quaternius:demo:{Path(rel).name}"))
        else:
            extraction.append(_extraction_record(rel, sha, len(payload), f"quaternius:demo:{Path(rel).name}"))

    _write_jsonl(vault / "manifests" / "asset_intake_manifest.jsonl", download)
    _write_jsonl(vault / "manifests" / "asset_extraction_manifest.jsonl", extraction)
    return vault


def _write_jsonl(path: Path, records: list[dict]) -> None:
    with path.open("w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")


def _read_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


class PruneHappyPathTests(unittest.TestCase):
    def test_prune_file_removes_file_and_record_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            target = "sources/quaternius/game-kit/demo/dup.fbx"
            plan = prune.build_plan(vault, [target], prune_dangling=False)
            result = prune.apply_prune(vault, plan)

            # File is gone; the load-bearing sibling and fixture remain.
            self.assertFalse((vault / target).exists())
            self.assertTrue((vault / "sources/quaternius/game-kit/demo/keep.gltf").exists())
            self.assertTrue((vault / "extracted/quaternius/game-kit/demo/Stan.gltf").exists())

            # Record dropped from the download manifest; extraction manifest untouched.
            download = _read_jsonl(vault / "manifests" / "asset_intake_manifest.jsonl")
            self.assertEqual([r["relative_path"] for r in download], ["sources/quaternius/game-kit/demo/keep.gltf"])
            self.assertEqual(result["download_records_dropped"], 1)
            self.assertEqual(result["extraction_records_dropped"], 0)

            # Receipts purged, tripwire held, state summary regenerated.
            self.assertEqual(result["receipts_removed"], 1)
            self.assertEqual(result["pre_top_level_count"], result["post_top_level_count"])
            self.assertFalse((vault / ".litd-verify-cache" / "abc123.receipt").exists())
            summary = json.loads((vault / "manifests" / "asset_vault_state_summary.json").read_text("utf-8"))
            self.assertEqual(summary["download_manifest_records"], 1)

    def test_prune_dangling_cleans_preexisting_dangling_record(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            # Manually create a dangling record (file never on disk).
            manifest = vault / "manifests" / "asset_intake_manifest.jsonl"
            records = _read_jsonl(manifest)
            records.append(_download_record("sources/quaternius/game-kit/demo/ghost.obj", "0" * 64, 10, "q:demo:ghost"))
            _write_jsonl(manifest, records)

            plan = prune.build_plan(vault, [], prune_dangling=True)
            result = prune.apply_prune(vault, plan)
            self.assertEqual(result["download_records_dropped"], 1)
            remaining = [r["relative_path"] for r in _read_jsonl(manifest)]
            self.assertNotIn("sources/quaternius/game-kit/demo/ghost.obj", remaining)


class PruneGuardTests(unittest.TestCase):
    def _snapshot(self, vault: Path) -> dict:
        snap = {}
        for name in ("asset_intake_manifest.jsonl", "asset_extraction_manifest.jsonl"):
            snap[name] = (vault / "manifests" / name).read_bytes()
        snap["files"] = sorted(str(p.relative_to(vault)) for p in vault.rglob("*") if p.is_file())
        return snap

    def test_fixture_prune_refused_and_leaves_state_unchanged(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            before = self._snapshot(vault)
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, ["extracted/quaternius/game-kit/demo/Stan.gltf"], prune_dangling=False)
            self.assertIn("load-bearing test fixture", str(ctx.exception))
            self.assertIn("Stan.gltf", str(ctx.exception))
            self.assertEqual(before, self._snapshot(vault))  # byte-for-byte unchanged

    def test_outside_vault_path_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            outside = Path(tmp) / "evil.txt"
            outside.write_text("x", encoding="utf-8")
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, [str(outside)], prune_dangling=False)
            self.assertIn("OUTSIDE the vault", str(ctx.exception))

    def test_dotdot_escape_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, ["sources/../../escape.txt"], prune_dangling=False)
            self.assertIn("OUTSIDE the vault", str(ctx.exception))

    def test_untracked_file_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            stray = vault / "sources/quaternius/game-kit/demo/stray.bin"
            stray.write_bytes(b"stray")
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, ["sources/quaternius/game-kit/demo/stray.bin"], prune_dangling=False)
            self.assertIn("no manifest record", str(ctx.exception))

    def test_missing_path_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, ["sources/quaternius/game-kit/demo/nope.fbx"], prune_dangling=False)
            self.assertIn("does not exist", str(ctx.exception))

    def test_empty_selection_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            with self.assertRaises(ValueError) as ctx:
                prune.build_plan(vault, [], prune_dangling=False)
            self.assertIn("Nothing to prune", str(ctx.exception))


class TopLevelCountTests(unittest.TestCase):
    def test_counts_direct_children_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            vault = _make_vault(tmp)
            # sources, extracted, manifests, logs, .litd-verify-cache == 5 top-level entries.
            self.assertEqual(prune.top_level_count(vault), 5)


if __name__ == "__main__":
    unittest.main()
