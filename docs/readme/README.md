# README Visual Reference Index

This directory contains the image assets used by the repository root
[`README.md`](../../README.md). The root README remains the source of truth for
the full narrative, captions, and product vision. This file exists so agents can
quickly re-load the visual references after context compaction without guessing
which images matter for editor, game-view, renderer, MCP, and authoring work.

## Required Editor References

- [`ED11-world-view.png`](ED11-world-view.png) - primary bird-eye world view,
  mode tabs, hierarchy, inspector/tool panel, content browser, minimap, and AI
  co-author placement.
- [`ED01-editor-shell.png`](ED01-editor-shell.png) - dockable editor shell,
  real-time viewport, scene hierarchy, inspector, and content browser.
- [`ED09-test-in-editor.png`](ED09-test-in-editor.png) - play-in-editor loop,
  hot reload, and live debug expectations. `DOCTRINE-OBSOLETE-NAV(#944)`: the
  circular minimap inside this artifact's *play viewport* is OBSOLETE and MUST NOT
  be implemented. Play-in-editor previews the packaged player runtime, so it
  inherits the player-runtime MUST-NOT list below - it is not an editor overview.
- [`ED12-play-topdown.png`](ED12-play-topdown.png) - top-down/ARPG play camera
  target when Playtest snaps from edit view into the game. BINDING (#1510 / #1537):
  reference for render quality, top-down framing, scene readability, and HUD craft
  (health/mana, action bar). **RUNTIME_CLEAN** after #1537 — no minimap, no GPS
  player-dot, no quest pins/route ribbons. A map UI is still permitted only while a
  concrete acquired map artifact is being viewed (#944).
- [`ED13-play-firstperson.png`](ED13-play-firstperson.png) - first-person play
  camera target for the same world and same playtest loop.

## Tool Surface References

- [`ED02-terrain-editor.png`](ED02-terrain-editor.png) - terrain sculpting,
  paint, doodads, regions, and brushes.
- [`ED03-object-editor.png`](ED03-object-editor.png) - WC3-style object and
  game-data editing.
- [`ED04-trigger-editor.png`](ED04-trigger-editor.png) - no-code
  Event/Condition/Action trigger authoring.
- [`ED05-ai-coauthor.png`](ED05-ai-coauthor.png) and
  [`03-mcp-agent.png`](03-mcp-agent.png) - shared human/AI capability layer and
  MCP-driven authoring.
- [`ED06-material-assets.png`](ED06-material-assets.png) - material, asset, and
  PBR look authoring.
- [`ED07-cinematic-director.png`](ED07-cinematic-director.png) - visual
  storytelling, staging, lighting, camera, and cinematic direction.
- [`ED08-vfx-editor.png`](ED08-vfx-editor.png) and [`09-vfx.png`](09-vfx.png) -
  particles, VFX curves, color ramps, and effect preview expectations.

## Game And System References

- [`07-asset-pipeline.png`](07-asset-pipeline.png) - text-to-playable asset
  generation pipeline.
- [`08-gameplay.png`](08-gameplay.png) - target game rendering and stylized
  real-time PBR direction.
- [`10-architecture.png`](10-architecture.png) - Calyx, engine, and editor
  separation.
- [`S17-agent-binding.png`](S17-agent-binding.png) - in-game agent binding
  surface.
- [`S16-quest-designer.png`](S16-quest-designer.png),
  [`GP10-dialogue.png`](GP10-dialogue.png), and [`GP11-quest.png`](GP11-quest.png)
  - story, dialogue, and quest authoring direction.
  `DOCTRINE-OBSOLETE-NAV(#944)`: `GP11-quest.png` is the single worst artifact in
  this set - it shows a quest-tracker panel, a quest pin on a minimap, AND a glowing
  route ribbon leading to the objective. All three are OBSOLETE and MUST NOT be
  implemented (#1510, #1537). `S16-quest-designer.png` shows a "Player Location"
  marker and route lines, but inside an EDITOR world-map *preview* panel - that is
  permitted authoring tooling and must never leak into packaged player runtime.

Gameplay screenshots that visually include a corner map, quest pin, route hint,
or marker are legacy composition references only. They are not permission to add
a default RPG map/minimap/GPS surface. Runtime RPG map access is governed by
#944 and must be source-gated through cartographer purchases, found/looted maps,
quest/archive/NPC/exploration knowledge, or equivalent authored state.

### This is now MECHANICALLY ENFORCED, not a prose promise

The whole reason #1510 existed is that a binding constraint (acquired-map doctrine)
and a blessed reference artifact drifted apart with **nothing checking they still
agreed**. Prose rots; a gate does not. Every `docs/readme/*.png` now carries a
doctrine verdict in [`scripts/reference-artifact-doctrine.tsv`](../../scripts/reference-artifact-doctrine.tsv),
enforced on every commit by [`scripts/check-reference-artifacts.ps1`](../../scripts/check-reference-artifacts.ps1)
(a step in `scripts/check-format.ps1`). An artifact classified `FORBIDDEN_NAV`
cannot be presented in any tracked markdown without the literal annotation token
`DOCTRINE-OBSOLETE-NAV(#944)` beside it, and a NEW image cannot enter `docs/readme/`
at all without someone looking at it and classifying it. The gate fails the commit
otherwise.

**A full visual audit of all 111 reference images (#1510) found 19 that depicted
packaged-player runtime carrying forbidden navigation.** #1537 cleared ED12;
#1558 batch-cleared the remaining 18 (`forbidden_nav` count is now **0** in
`scripts/reference-artifact-doctrine.tsv`). Replacements are GPU reference frames
with `runtime_rpg_no_map_access` (combat HUD only; no minimap/GPS/quest pins/routes).
Higher-fidelity art reshoots may land later, but doctrine-forbidden pixels are gone.
The authoritative, machine-checked list is the TSV.

The **editor-only** bird-eye world overview (e.g. the minimap element described for
`ED11-world-view.png`, and the world-map preview panel in `S16-quest-designer.png`)
is an authoring affordance and is explicitly separate from — and must never leak
into — the packaged player runtime, where navigation is acquired-map-gated only.

### MUST-NOT list — packaged player runtime (BINDING, #944 / ADR-944 / #1510)

Regardless of what any legacy reference image in this directory depicts, the
packaged RPG player runtime MUST NOT implement:

- a built-in map, minimap, or world-map screen of any kind;
- a GPS-style live player-position dot/indicator ("Here" marker);
- quest pins, quest-tracker waypoint markers, or objective map markers;
- a route ribbon, path line, or turn-by-turn navigation overlay;
- automatic exploration-unlocked map coverage (free fog-of-war reveal into a
  player-facing map surface).

The ONLY permitted player-facing map UI is one shown while a concrete acquired
map artifact (cartographer purchase, found/looted map, quest/archive/NPC/
exploration knowledge) is actively being viewed, limited to that artifact's
authored scope and quality. Editor authoring overviews are creator tooling and
never ship as player HUD. Any reference image contradicting this list — all 19
`FORBIDDEN_NAV` artifacts in `scripts/reference-artifact-doctrine.tsv` — is
OBSOLETE on those elements and is a reference ONLY for render quality, camera
framing, scene readability, and combat-HUD craft (health/mana orbs, action bar,
party frames, floating damage, chat).

**The packaged player runtime honours this list today.** `crates/litd-game` (the
shipped player binary) renders the world and has no HUD/minimap/quest surface at
all, and `crates/litd-engine/src/render/ui_map_policy.rs` fails closed:
`RuntimeRpgNoMapAccess` rejects every marker, and `RuntimeRpgDiegeticAcquired`
requires a concrete acquired `source_id`/`scope_id` and still forbids player dots,
quest pins and route lines. The reference images — not the code — are what drifted.

## Usage Rules For Agents

Before visual/editor/gameplay/render/MCP-surface work, inspect the relevant
images here and the corresponding root README section. These images are the
layout and quality floor, not proof that a new implementation works. Issue
closure still requires manual Synapse/computer-control FSV against the real
source of truth.

Operational state lives in GitHub issues. For this repo, agents must not use
Codex goal tools (`get_goal`, `create_goal`, `update_goal`) unless the operator
explicitly asks for that Codex feature. Treat Codex goal objective/resume state
as context only. If the raw `{"detail":"Bad Request"}` banner appears, follow
#1268 with bounded read-only diagnostics and continue through issue state; do
not directly edit Codex runtime SQLite. Agents must not terminate, restart,
close, or otherwise control user programs, terminal tabs, VS Code sessions,
browsers, other Codex sessions, or unrelated processes unless the operator
explicitly gives that process-control instruction in the current conversation.
