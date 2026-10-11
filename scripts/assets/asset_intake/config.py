from __future__ import annotations

from pathlib import Path

from .models import SourcePolicy

BYTES_PER_GB = 1024**3
REPO_ROOT = Path(__file__).resolve().parents[3]
POLYHAVEN_API = "https://api.polyhaven.com"
AMBIENTCG_API = "https://ambientCG.com/api/v3/assets"
DEFAULT_VAULT_ROOT = REPO_ROOT / "local-asset-vault"
DEFAULT_USER_AGENT = (
    "LitDGameEditorAssetIntake/0.1 "
    "(https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/issues/1242)"
)

KENNEY_PACK_SLUGS = (
    "fantasy-town-kit",
    "castle-kit",
    "modular-dungeon-kit",
    "mini-dungeon",
    "tiny-dungeon",
    "isometric-miniature-dungeon",
    "roguelike-caves-dungeons",
    "medieval-rts",
    "platformer-pack-medieval",
    "retro-fantasy-kit",
    "tower-defense-kit",
)

KAYKIT_OWNER = "KayKit-Game-Assets"
KAYKIT_REPO_ALLOWLIST = (
    "KayKit-Dungeon-Remastered-1.0",
    "KayKit-Medieval-Hexagon-Pack-1.0",
    "KayKit-Character-Pack-Adventures-1.0",
    "KayKit-Character-Pack-Skeletons-1.0",
    "KayKit-City-Builder-Bits-1.0",
    "KayKit-Furniture-Bits-1.0",
    "KayKit-Halloween-Bits-1.0",
    "KayKit-Prototype-Bits-1.0",
    "KayKit-Restaurant-Bits-1.0",
    "KayKit-Space-Base-Bits-1.0",
)

ASSET_TYPE_PRIORITY = {
    "game-kit": 1000,
    "model": 350,
    "terrain": 300,
    "surface": 180,
    "material": 160,
    "hdri": 120,
}

VISUAL_TARGET_KEYWORDS = {
    "abandoned": 4,
    "ancient": 3,
    "barrel": 2,
    "brick": 5,
    "building": 4,
    "castle": 6,
    "cave": 4,
    "church": 5,
    "cliff": 5,
    "cobble": 6,
    "cobblestone": 6,
    "container": 2,
    "crate": 2,
    "dirt": 4,
    "dungeon": 6,
    "forest": 4,
    "gothic": 5,
    "grave": 5,
    "graveyard": 6,
    "ground": 4,
    "historic": 3,
    "lava": 5,
    "medieval": 7,
    "moss": 4,
    "mountain": 4,
    "old": 2,
    "path": 4,
    "paving": 6,
    "rock": 6,
    "rocks": 6,
    "ruin": 7,
    "ruins": 7,
    "rustic": 4,
    "snow": 3,
    "soil": 3,
    "statue": 3,
    "stone": 7,
    "stump": 3,
    "terrain": 7,
    "tile": 4,
    "tiles": 4,
    "tomb": 5,
    "tree": 3,
    "urban": 2,
    "wall": 5,
    "wood": 4,
    "woodland": 4,
}

POLICIES = {
    "polyhaven": SourcePolicy(
        source="polyhaven",
        license_name="CC0-1.0",
        license_url="https://polyhaven.com/license",
        api_url="https://polyhaven.com/our-api",
    ),
    "ambientcg": SourcePolicy(
        source="ambientcg",
        license_name="CC0-1.0",
        license_url="https://docs.ambientcg.com/license/",
        api_url="https://docs.ambientcg.com/api/v3/assets/",
    ),
    "kenney": SourcePolicy(
        source="kenney",
        license_name="CC0-1.0",
        license_url="https://www.kenney.nl/support",
        api_url="https://www.kenney.nl/assets",
    ),
    "kaykit": SourcePolicy(
        source="kaykit",
        license_name="CC0-1.0",
        license_url="https://github.com/KayKit-Game-Assets",
        api_url="https://api.github.com/users/KayKit-Game-Assets/repos",
    ),
    "quaternius": SourcePolicy(
        source="quaternius",
        license_name="CC0-1.0",
        license_url="https://quaternius.com/",
        api_url="https://drive.google.com/drive/folders/",
    ),
}
