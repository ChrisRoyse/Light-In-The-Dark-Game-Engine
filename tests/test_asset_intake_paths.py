import importlib.util
import json
import sys
import unittest
import zipfile
from pathlib import Path
from tempfile import TemporaryDirectory


def load_intake_module():
    module_path = Path(__file__).resolve().parents[1] / "scripts" / "assets" / "intake_cc0_assets.py"
    spec = importlib.util.spec_from_file_location("intake_cc0_assets_for_tests", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Unable to load {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class AssetIntakePathContainmentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.intake = load_intake_module()
        from asset_intake.models import DownloadItem
        from asset_intake.storage import _filesystem_path, sha256_file, verify_zip_and_extract
        from asset_intake.utils import safe_rel_path

        cls.DownloadItem = DownloadItem
        cls.filesystem_path = staticmethod(_filesystem_path)
        cls.safe_rel_path = staticmethod(safe_rel_path)
        cls.sha256_file = staticmethod(sha256_file)
        cls.verify_zip_and_extract = staticmethod(verify_zip_and_extract)

    def test_allows_missing_nested_child_under_existing_vault(self):
        with TemporaryDirectory() as temp_dir:
            vault = Path(temp_dir) / "vault"
            vault.mkdir()
            child = vault / "sources" / "kaykit" / "pack" / "mesh.fbx"

            self.intake.assert_under(vault, child)

    def test_allows_existing_child_under_vault(self):
        with TemporaryDirectory() as temp_dir:
            vault = Path(temp_dir) / "vault"
            child = vault / "sources" / "existing.bin"
            child.parent.mkdir(parents=True)
            child.write_bytes(b"asset")

            self.intake.assert_under(vault, child)

    def test_rejects_parent_traversal_outside_vault(self):
        with TemporaryDirectory() as temp_dir:
            vault = Path(temp_dir) / "vault"
            vault.mkdir()
            child = vault / "sources" / ".." / ".." / "outside.bin"

            with self.assertRaisesRegex(ValueError, "outside intended directory"):
                self.intake.assert_under(vault, child)

    def test_rejects_absolute_path_outside_vault(self):
        with TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            vault = root / "vault"
            outside = root / "outside" / "asset.bin"
            vault.mkdir()

            with self.assertRaisesRegex(ValueError, "outside intended directory"):
                self.intake.assert_under(vault, outside)

    def test_extracts_zip_member_when_old_temp_suffix_would_exceed_windows_max_path(self):
        with TemporaryDirectory() as temp_dir:
            vault = Path(temp_dir) / "vault"
            archive_rel = Path(
                "sources",
                "quaternius",
                "manual-game-kit",
                "universalbasecharacters",
                "Universal_Base_Characters_Standard_.zip",
            )
            archive = vault / archive_rel
            archive.parent.mkdir(parents=True)

            member_name = (
                "Universal Base Characters[Standard]/"
                "Base Characters/"
                "Textures/"
                "Normals Unity - Godot/"
                "T_Superhero_Female_Normal.png"
            )
            payload = b"real zip member bytes"
            with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
                bundle.writestr(member_name, payload)

            extraction_key = Path(
                "extracted",
                "quaternius",
                "manual-game-kit",
                "universalbasecharacters",
                "Universal_Base_Characters_Standard",
            )
            old_temp_dir = (vault / extraction_key).with_name(
                Path(extraction_key).name + ".extracting-99999"
            )
            old_temp_target = old_temp_dir / self.safe_rel_path(member_name)
            self.assertGreater(len(str(old_temp_target)), 260)

            item = self.DownloadItem(
                source="quaternius",
                asset_id="universalbasecharacters",
                asset_type="game-kit",
                title="Universal Base Characters",
                source_url="https://quaternius.com/packs/universalbasecharacters.html",
                license_name="CC0-1.0",
                license_url="https://quaternius.com/packs/universalbasecharacters.html",
                api_url="https://quaternius.itch.io/universal-base-characters",
                url="manual-file://Universal Base Characters[Standard].zip",
                destination=archive_rel,
                declared_size=archive.stat().st_size,
                md5=None,
                bundle_key="quaternius:manual-game-kit:universalbasecharacters:Universal_Base_Characters_Standard_.zip",
                extraction_key=str(extraction_key),
            )

            records = self.verify_zip_and_extract(vault, item)

            self.assertEqual(len(records), 1)
            extracted = vault / records[0]["relative_path"]
            with open(self.filesystem_path(extracted), "rb") as handle:
                self.assertEqual(handle.read(), payload)
            self.assertEqual(records[0]["bytes_on_disk"], len(payload))
            self.assertEqual(records[0]["sha256"], self.sha256_file(extracted))

            marker = vault / extraction_key / "_extraction_manifest.jsonl"
            marker_records = [json.loads(line) for line in marker.read_text(encoding="utf-8").splitlines()]
            self.assertEqual(marker_records, records)


if __name__ == "__main__":
    unittest.main()
