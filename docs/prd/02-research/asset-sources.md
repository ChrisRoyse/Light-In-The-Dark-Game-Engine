# Research: CC0 Asset Sources — Inventory, Licensing, WC3 Coverage Map

> Expands [PRD §3.3](../../PRD.md#33-assets-cc0-low-poly-fantasy-rts-packs-zero-cost).
> Related: [Rendering Dimensionality](rendering-dimensionality.md) · [Model Format Selection](model-format-selection.md) · [G3N Engine Evaluation](g3n-evaluation.md)

Goal G4 requires every game model to be CC0 (or equivalently free for commercial use) at **$0 asset budget**. This document inventories each selected pack, records license-verification notes, maps pack contents onto the Warcraft III asset taxonomy (units / buildings / doodads / terrain / VFX / UI / audio), and identifies the gaps no CC0 pack covers.

## 0. Why CC0 specifically

The license bar is deliberately stricter than "free":

- **No attribution chains.** CC-BY would be workable but forces a credits manifest that must survive every asset edit, re-export, and atlas merge; CC0 ([deed](https://creativecommons.org/publicdomain/zero/1.0/)) waives all conditions, so derived assets (merged primitives, repainted atlases, renamed clips — all of which the pipeline performs, see [Model Format Selection §3](model-format-selection.md)) carry zero obligations.
- **No share-alike contamination.** GPL/CC-BY-SA art would impose terms on the shipped asset bundle. CC0 keeps the engine's BSD/MIT/Apache dependency policy (G4) uniform across code *and* content.
- **IP-risk hygiene.** The PRD's WC3-proximity risk row (§8) depends on every asset being provably original and freely licensed. CC0 packs from named, established authors (Quaternius, Kay Lousberg, Kenney) with public distribution history are the strongest provenance available at $0.
- **Derivative freedom is load-bearing**, not theoretical: team-color masking (R-RND-7), atlas downsampling (R-RND-2), clip renaming (R-AST-3), and missing-`Death`-clip authoring (§1.1) all modify the originals.

One pack in the table — Quaternius Ultimate Fantasy RTS — is distributed as "free for personal and commercial use" wording on some mirrors rather than an explicit CC0 deed; §4's verification step treats the in-archive license file as authoritative, and Quaternius's standard practice is CC0. If any download's archive lacks an acceptable grant, the pack is dropped and §2's coverage matrix re-evaluated — no asset enters `assets/` on storefront wording alone.

WC3 asset taxonomy used throughout:

| WC3 category | Meaning |
|---|---|
| **Units** | Animated characters: workers, soldiers, heroes, creeps, flyers |
| **Buildings** | Town hall chain, production/tech/defense structures, with construction & destruction states |
| **Doodads** | Decorative/destructible props: trees, rocks, crates, fences, gold mines |
| **Terrain** | Ground tiles/cliffs/water/ramps |
| **VFX** | Spell effects, projectiles, buff auras, explosions |
| **UI** | Command card icons, hero portraits, frames, cursors, minimap art |
| **Audio** | Unit responses, combat SFX, music |

---

## 1. Pack inventories

### 1.1 Quaternius — Ultimate Fantasy RTS (primary unit/building source)

- **URL:** <https://quaternius.com/packs/ultimatefantasyrts.html> (also bundled on itch.io: <https://quaternius.itch.io/>)
- **Contents:** **128 models** purpose-built for a fantasy RTS — exactly LitD's genre. Two opposing faction themes, each with: worker/gatherer units, melee and ranged soldiers, cavalry/monster units, and a **building set with evolution/upgrade stages** (the WC3 town-hall-tier pattern). Character models are **rigged and animated**; buildings are static meshes. Textured in the low-poly flat/gradient style.
- **Formats:** glTF (also FBX/OBJ/Blend) — glTF is the shipped format we consume directly.
- **License verification:** Quaternius distributes packs as **CC0 1.0 Universal** — stated on the site footer/FAQ and on the itch.io pages ("free for personal and commercial use, no attribution required"). **Verify at download time** that the pack ZIP's `License.txt` says CC0; archive a copy of the license file and the download-page snapshot in `assets/_licenses/quaternius-ultimate-fantasy-rts/` (M0 exit criterion).
- **WC3 mapping:** **Units** (primary source — the only selected pack with rigged, animated humanoid soldiers in two faction styles), **Buildings** (primary source — evolution stages map to WC3 building upgrades). Animation clips must be checked against the R-AST-3 contract (`Idle`/`Walk`/`Attack`/`Death`); Quaternius rigs typically include these but clip *names* vary — the asset validator normalizes/renames clips at import, and any missing `Death` clip is authored in Blender (reuse rig, simple fall-over keyframes).

### 1.2 KayKit — Medieval Hexagon Pack (primary terrain source)

- **URL:** <https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0> (also <https://kaylousberg.itch.io/kaykit-medieval-hexagon>)
- **Contents:** **200+ models**: hexagonal terrain tiles (grass, water, sand, stone, transitions, rivers, coasts), hills/mountains, roads/bridges, medieval buildings (castles, towers, houses, mills, markets), props (trees, rocks, banners), and units/buildings in **4 team-color variants**. All geometry samples a **single 1024² gradient atlas** that downsamples cleanly to 128² — the texturing pattern the PRD standardizes on for R-RND-2 and the low preset.
- **Formats:** glTF (.gltf/.glb), FBX, OBJ.
- **License verification:** **CC0 1.0** — stated in the GitHub repo's license file and the itch.io page. GitHub distribution is ideal for provenance: pin the exact commit hash in the asset manifest. Archive license as above.
- **WC3 mapping:** **Terrain** (primary source — directly feeds PRD Open Question 3's tile-mesh option), **Buildings** (secondary/neutral structures), **Doodads** (trees, rocks, props). The 4 team-color variants are reference material only — LitD does team color via shader uniform (R-RND-7), so we ship one neutral variant and mask team areas in the atlas.

### 1.3 KayKit — Medieval Builder Pack

- **URL:** <https://opengameart.org/content/kaykit-medieval-builder-pack-10> (also <https://kaylousberg.itch.io/kaykit-medieval-builder-pack>)
- **Contents:** **200+ models** of medieval scenery and buildings on square/free placement (not hex-bound): construction-stage building meshes (scaffolding, partial walls — directly usable as **WC3-style "under construction" states**), walls, gates, siege props, market stalls, fences, carts, wells, barrels, crates.
- **Formats:** glTF, OBJ, DAE (glTF consumed; see [Model Format Selection](model-format-selection.md)).
- **License verification:** **CC0** per the OpenGameArt listing's license field and the bundled license file. OpenGameArt displays the license machine-readably; archive page snapshot + ZIP license file.
- **WC3 mapping:** **Buildings** (construction states — a coverage item no other pack provides), **Doodads** (the deepest prop inventory of all selected packs: destructible crates/barrels/fences map to WC3 destructables).

### 1.4 Kenney — Castle Kit / Hexagon Kit / Retro Medieval Kit

- **URLs:** <https://kenney-assets.itch.io/castle-kit> · <https://kenney-assets.itch.io/hexagon-kit> · <https://opengameart.org/content/retro-medieval-kit> · master index <https://kenney.nl/assets>
- **Contents:** ~70–120 models each. *Castle Kit:* modular castle pieces (walls, towers, gates, keeps), **siege weapons** (catapult, ballista — WC3 siege-unit analogues, unrigged), banners. *Hexagon Kit:* a second hex terrain set (alternate biome look vs KayKit). *Retro Medieval Kit:* low-fi medieval village structures and props.
- **Formats:** glTF/GLB, OBJ, FBX.
- **License verification:** Kenney publishes **everything as CC0 1.0** — stated uniformly on kenney.nl and each itch.io page. Lowest-risk source in the list; same archival procedure.
- **WC3 mapping:** **Buildings** (defensive structures: towers, walls, gates — WC3 tower/wall analogues), **Doodads**, **Terrain** (alternate biome via Hexagon Kit), partial **Units** (siege weapons; need animation authored — wheel-roll and arm-swing are simple rigid-body keyframes, no skinning).

### 1.5 Supplementary Kenney packs (UI / VFX / audio raw material)

Not in the PRD table but same CC0 source, closing gaps identified in §3:

- **Kenney UI Pack & Game Icons** (<https://kenney.nl/assets/ui-pack>, <https://kenney.nl/assets/game-icons>): panel frames, buttons, cursors, generic ability icons — raw material for the WC3 command card and HUD chrome (R-UI-1).
- **Kenney Particle Pack** (<https://kenney.nl/assets/particle-pack>): smoke/fire/magic/spark sprite textures — base textures for billboard spell VFX.
- **Kenney Audio packs** (<https://kenney.nl/assets?q=audio>: RPG Audio, Impact Sounds, UI Audio): CC0 SFX. Format note: ship as `.ogg` per R-AUD-1 (transcode where source is `.wav` — G3N's player reads both, `repoes/engine/audio/player.go:44`, but `.ogg` keeps the asset budget down).

---

## 2. Coverage matrix

| WC3 category | Primary source | Secondary | Coverage |
|---|---|---|---|
| Units (soldiers, workers) | Quaternius Ultimate Fantasy RTS | — | **Good** (2 faction themes; clip-name normalization needed) |
| Units (heroes) | — | Quaternius (re-skin soldier rigs) | **Partial** — see gap G1 |
| Units (creeps/monsters) | Quaternius (monster units) | other Quaternius packs (Monsters etc.) | Adequate for v1 |
| Units (flyers) | — | — | **Gap** G6 |
| Buildings + upgrade tiers | Quaternius | KayKit Hexagon, Kenney Castle | **Good** |
| Construction states | KayKit Builder | — | Good |
| Defensive structures | Kenney Castle Kit | KayKit | Good |
| Doodads / destructables | KayKit Builder | KayKit Hexagon, Kenney | **Good** |
| Terrain | KayKit Hexagon | Kenney Hexagon | Good *if* tile terrain wins Open Question 3; heightmap path has **no** asset source — see gap G5 |
| Spell/projectile VFX | — | Kenney Particle (textures only) | **Gap** G2 |
| Hero portraits / icons | — | Kenney icons (generic only) | **Gap** G1/G3 |
| UI chrome / cursors | Kenney UI Pack | — | Partial (needs RTS-specific composition) |
| Unit voice responses | — | — | **Gap** G4 |
| Music | — | OGA CC0 search | Gap (non-blocking) |

---

## 3. Gap analysis — what no CC0 pack covers

- **G1 — Hero portraits.** WC3's animated 3D talking-head portraits have no CC0 equivalent, and no pack ships portrait-framed art of *its own* characters. **Mitigation (v1):** render portraits from the unit models themselves — a secondary camera/render-to-target close-up of the unit's head with the `Portrait` clip when present (R-AST-3 lists `Portrait` as optional). Static fallback: offline-rendered PNG headshots of each model, generated by a tool in `tools/`. No third-party art needed; style stays consistent automatically.
- **G2 — Spell VFX.** No CC0 pack provides RTS spell effects (storm bolt trails, blizzard, healing auras, explosions) as ready 3D assets. **Mitigation:** VFX are *engineered*, not sourced — billboarded quads and simple meshes using Kenney particle textures, additive-blend materials, and the point/spot-light cap (R-RND-4); selection circles and AoE markers are projected quads/decals built in M4 ([G3N Evaluation §7](g3n-evaluation.md)). This is the largest art-replacement engineering item and is scheduled inside M4.
- **G3 — Ability/command-card icons.** WC3-style painted icons don't exist as a coherent CC0 set; mixed-source icons look incoherent. **Mitigation:** Kenney Game Icons (single consistent flat style) for v1; flat-style icon coherence is acceptable for an engine vertical slice.
- **G4 — Unit voice responses** ("Ready to work?"). No CC0 voice-line library matches a fantasy roster. **Mitigation (v1):** generic SFX acknowledgments (click/horn/grunt from Kenney audio); voice acting is a content concern, not an engine concern — out of v1 scope.
- **G5 — Heightmap terrain texturing.** If Open Question 3 resolves to WC3-like heightmap cliffs rather than tile meshes, the tile packs don't apply and CC0 *tileable ground textures* (e.g. from ambientCG/Poly Haven, both CC0) plus custom cliff meshes are needed. The asset ecosystem therefore **weighs on the open question in favor of tiles**, as the PRD notes.
- **G6 — Flying units.** Faction flyers (gryphon/wyvern analogues) are thin in the selected packs. Quaternius's broader animal/monster CC0 catalogue (dragons, birds) is the designated fill-in; worst case, v1's vertical slice ships ground-only.

None of these gaps blocks M0–M6: the vertical slice (M6: build, train, fight, win/lose) is fully covered by §1 packs plus engineered VFX.

### 3.1 Style coherence across packs

Mixing four authors' packs risks a visually incoherent world. Mitigating factors, in order of leverage:

1. **Shared low-poly flat/gradient aesthetic.** Quaternius, KayKit, and Kenney all work in the same untextured-or-atlas, flat-shaded low-poly idiom; there are no photoreal textures to clash.
2. **Atlas repaint pass.** Because every selected pack uses small palette/gradient atlases, a single Blender batch pass can remap pack palettes toward one master palette per biome/faction — cheap insurance applied only where side-by-side screenshots show drift (checked during M4).
3. **Role partitioning.** §2's matrix assigns each pack a *layer* (Quaternius = units/buildings, KayKit = terrain/doodads, Kenney = defenses/props), so stylistic seams fall between world layers, where they read as intentional, rather than between adjacent units.
4. **Uniform lighting.** One directional sun + ambient (R-RND-4) and optional unlit preset (R-RND-5) flatten residual shading differences.

---

## 4. Acquisition & verification procedure (M0)

1. **Download** each pack from the URLs above; record source URL, retrieval date, version/commit, and SHA-256 of the archive in `assets/_manifest.json`.
2. **License capture:** copy the in-archive license file and a snapshot of the download page into `assets/_licenses/<pack>/`. A pack is accepted only if the license is CC0 or an explicit free-commercial grant **in the archive itself**, not just the storefront.
3. **Validate:** run every model through the asset-validation CLI (R-AST-2 / R-FMT-2): core-glTF check, triangle budget (R-RND-2), single-primitive rig rule, clip-name contract (R-AST-3) — see [Model Format Selection §3](model-format-selection.md).
4. **Normalize:** Blender batch pass for clip renaming, primitive merging, atlas conformance, +Y-up GLB export.
5. Re-validate; only validator-clean GLBs enter `assets/`.

M0's exit criterion "asset packs downloaded + validated" means steps 1–5 complete for the §1.1–1.4 packs.

## 5. Watch list — additional CC0 sources for gap-filling

Not yet selected, but pre-vetted as license-compatible fill-ins if §3's gaps widen or Open Question 3 goes the heightmap route:

| Source | Relevant content | License | Fills |
|---|---|---|---|
| [Quaternius — full catalogue](https://quaternius.com/) | Monsters, Animals, Dungeon, Nature packs — rigged creeps, flyers, doodad variety | CC0 | G6 (flyers), creep variety |
| [Poly Pizza](https://poly.pizza/) | Searchable aggregator of CC0/CC-BY low-poly models (filter to CC0 only) | per-model — filter required | spot fills, doodads |
| [ambientCG](https://ambientcg.com/) | Tileable PBR ground/rock/grass textures | CC0 | G5 (heightmap terrain texturing) |
| [Poly Haven](https://polyhaven.com/) | Textures + HDRIs | CC0 | G5, skybox/ambient reference |
| [OpenGameArt (CC0 filter)](https://opengameart.org/) | SFX, music, icons, misc models | per-item — CC0 filter required | G3, G4, music |

Aggregators (Poly Pizza, OpenGameArt) require **per-item** license verification — the §4 procedure applies to each downloaded item individually, never to the aggregator as a whole.

## 6. Sources

- [Quaternius — Ultimate Fantasy RTS](https://quaternius.com/packs/ultimatefantasyrts.html)
- [KayKit — Medieval Hexagon Pack (GitHub)](https://github.com/KayKit-Game-Assets/KayKit-Medieval-Hexagon-Pack-1.0)
- [KayKit — Medieval Builder Pack (OpenGameArt)](https://opengameart.org/content/kaykit-medieval-builder-pack-10)
- [Kenney — asset library index (all CC0)](https://kenney.nl/assets) · [Castle Kit](https://kenney-assets.itch.io/castle-kit) · [Hexagon Kit](https://kenney-assets.itch.io/hexagon-kit) · [Retro Medieval Kit](https://opengameart.org/content/retro-medieval-kit)
- [CC0 1.0 Universal deed](https://creativecommons.org/publicdomain/zero/1.0/)
