---
name: content-pipeline
description: Mass-produce Carrion Fields content (races, classes, abilities, world/areas, items, mobs) as editor-authored, Calyx-ingested data -- the assembly line for the 300+ CF content issues. Use when authoring or batching any rpg/ content spec. Covers the author->emit->persist_and_lower->readback flow, the framework-availability map, the one-worktree-per-category rule, and per-item Definition of Done.
---

# Content pipeline

The repeatable assembly line for the ~315-issue CF content backlog (#83-#1660, mostly per-entity
decompositions of epic #984). All content is authored as editor data ingested into Calyx -- NEVER
hardcoded Rust side-stores (that is the #1455 violation of doctrine #1348; see the [[calyx]] skill).
All rpg content lives in `crates/litd-engine/src/rpg/`. Batch depth: `references/batch-workflow.md`.

## 1. The canonical flow (verified, per category)

Four steps, proven on Race (#1000 Elf) / Class / Ability:

1. **Author** a `*Spec` / `*Catalog` in a `#[cfg(test)]` fixture (e.g. `RaceSpec`,
   `crates/litd-engine/src/rpg/race_specs_types.rs`, schema `litd.rpg.race_spec.v1`).
2. **Emit** `EngineContentRecord`s via the category factory:
   `race_spec_content_records(&spec) -> Result<Vec<EngineContentRecord>>`
   (`crates/litd-engine/src/rpg/race_specs_content.rs`). Provenance MUST be editor-authored:
   `source.path = "editor://rpg/ashen_reach/race_specs/{id}"`. No `rpg://` literals, no
   self-attesting `AnchorKind::TestPass` (the side-store gate rejects both).
3. **Persist and lower** through the ONE shared entry point
   (`crates/litd-engine/src/rpg/authored_content.rs:107`):
   `persist_and_lower_rpg_authored_content(store: &EngineCalyxContentStore, records) -> Result<RpgAuthoredContentLoweringReadback>`.
   Internally it runs `validate_rpg_authored_content_records` -> `store.put_content(record)`
   (Calyx Aster = source of truth) -> `EngineLoweringValve::lower_content_for_sim(&row)` ->
   `verify_lowered_package`. Every content factory MUST route through this validator (gate-enforced).
4. **Readback** (this is the FSV):
   `store.get_content_for_editor(kind, id) -> EngineContentConstellationReadback`
   (`crates/litd-engine/src/calyx_content.rs:188`). FSV passes when the readback matches what was
   authored.

`store` is an `EngineCalyxContentStore` (`crates/litd-engine/src/calyx_content.rs:143`).

## 2. Framework-availability map (grep before you assume)

State verified on this branch (2026-07-12) -- re-grep `crates/litd-engine/src/rpg/` before a batch,
this map drifts as frameworks land:

| Category | State | Where / what to do |
|---|---|---|
| **Race** | EXISTS | `race_specs_types.rs` (`RaceSpec`, `litd.rpg.race_spec.v1`). Author now. |
| **Ability** (skill/spell/song/supplication/cabal-power) | EXISTS (generic) | `abilities_types.rs` (`enum AbilityKind`, `AbilityDefinition`, `AbilityCatalog`). No visual gate -> CLEANEST full closes. |
| **Class** | EXISTS | `class_specs_types.rs` (`ClassSpec`, `litd.rpg.class_spec.v1`) + `class_specs_content.rs`/fixture. Has a visual contract -> mechanics-complete, visual pending #782. |
| **World / Area / City** | PARTIAL (infernal-only) | Only `infernal_zones*.rs` fixtures exist here. NO generic `AreaSpec` on this branch -- build the generic framework first (or confirm #1706 landed). |
| **Item / loot** | PARTIAL | `infernal_relic_vaults_content.rs` only; build a generic `ItemSpec`. |
| **Mob / creature / boss** | PARTIAL | `infernal_boss_agents_types.rs` (`InfernalBossAgentSpec`) only; build a generic `CreatureSpec`. |
| **Quest** | MISSING | Build the framework first. |
| **Cabal / religion / faction** | PARTIAL | `faction_leaders` + `themes` only; need `CabalSpec` / `ReligionSpec`. |

Where a framework EXISTS, content is authorable NOW. Where PARTIAL/MISSING, build the generic
framework first (generalize from the infernal-only fixtures), then author.

## 3. ONE worktree per content category (critical for fan-out)

Every rpg category funnels through THREE shared files -- its `*_test_fixture.rs`, its `*_tests.rs`,
and the global hub `crates/litd-engine/src/rpg.rs` (~296 lines, near the 500-line ratchet gate).

- **ONE worktree per category, MANY instances per PR.** Cross-category parallel is safe (different
  fixtures); intra-category parallel guarantees conflicts on those three files.
- Do the `rpg.rs` mod/re-export edit as ONE deliberate serialized step.
- Split a fixture when it nears 500 lines (the line-count ratchet fails commits that add/grow
  >=500-line .rs files).

## 4. Facts come from CF; originalize only name/lore (#984)

Content facts come from the **[[cf-source]]** skill -- the Carrion Fields wiki
(`wiki.carrionfields.net/race|class/<slug>`, fetchable via exa `web_search_exa` / `web_fetch_exa`;
plain WebFetch has failed here). This is FACTUAL TRANSCRIPTION, not free design:

- Capture the real CF mechanics (stat caps, alignment, size, resistances/vulnerabilities, innate
  abilities, vision, allowed classes, XP penalty, hometowns, ...). Do NOT invent numbers.
- Originalize ONLY the name and lore flavor into Ashen Reach terms per #984/#1670. Mechanics stay
  factual. The `RaceSpec` carries `player_facing_canon_name` / `mechanics_source_url` /
  `source_issue` fields designed for exactly this split.

## 5. Definition of Done (per item)

1. Authored as editor data (NEVER a hardcoded Rust side-store).
2. Ingested into Calyx (issue #1348) -- Aster vault is the source of truth.
3. Lowered to a fingerprinted deterministic package.
4. Readback-verified: `get_content_for_editor` matches authored.
5. FSV in reality per issue #800 with MINIMAL-SUFFICIENT evidence (smallest dataset that gives
   100% proof, not the biggest).
6. The side-store gate is clean: `pwsh scripts/check-calyx-side-store-eradication.ps1` (exit 0).

Note on "close": abilities can FULLY close (no visual gate). Any spec carrying a visual contract
(Race/Class/Area) reaches "mechanics-complete, visual pending #782" but does not fully close until
the wgpu model lands. Area content is also gated on in-editor Synapse render (#800, #944 acquired-map
doctrine, map flags pinned false).

## 6. Batch workflow (keep the loop fast)

Transcribe facts -> author data files -> emit + lower -> readback assert -> **ONE consolidated FSV
harness per batch, not per item**. See `references/batch-workflow.md`. Known framework gaps
(peer-filed): #1630 (no MCP authoring tool / no runtime loader -- specs are `cfg(test)` Rust),
#1631 (resistance vocab hardcoded to 9 combat channels; class_access lacks referential integrity).
