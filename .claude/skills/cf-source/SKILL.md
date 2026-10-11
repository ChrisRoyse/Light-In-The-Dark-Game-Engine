---
name: cf-source
description: Research Carrion Fields facts fast and correctly for GameEditor content/issue work. Use when you need CF mechanics, stats, skill/spell levels, race/class specs, world/area layout, item stats, or leveling ladders to transcribe into issues or data. Covers the helpfile/wiki/forum/itemsearch source map with known helpfile IDs, the exa fetch recipe, and render traps (empty necromancer page, 1998 angelfire, garbled ASCII map, Calyx #refs).
---

# CF source research (Carrion Fields, factual transcription)

North-star #959: recreate Carrion Fields. Content work is **FACTUAL TRANSCRIPTION**,
never invention -- mechanics/stats/layout are captured from the sources below.
Only names/lore get originalized into Ashen Reach terms per #984/#1670. Inventing
numbers violates the no-fabrication rule (a prior pilot did this; superseded).

## Fetch recipe (use the exa MCP tools)

Plain WebFetch has failed on CF for agents; use exa. `mcp__exa__web_fetch_exa` batches
multiple URLs in one call.

- Helpfiles: keep `maxCharacters` modest (~2000) -- they are short.
- Class/race wiki pages and forum topics: larger (~6000-10000).
- `mcp__exa__web_search_exa` when you don't know the id/slug.

Always cite the exact id/slug in the issue you write (see /issueflow section 3).

## Source map (quick)

| Source | URL pattern |
|--------|-------------|
| Helpfile by id | `https://carrionfields.net/helpsearch.php?id=NNN` |
| Helpfile by keyword | `https://carrionfields.net/helpsearch.php?keywords=...` |
| Class page | `https://wiki.carrionfields.net/class/<name>` |
| Race page | `https://wiki.carrionfields.net/race/<name>` |
| World map (ASCII) | `http://wiki.qhcf.net/index.php?title=WorldMap` |
| Area list | `http://wiki.qhcf.net/index.php?title=AreaList` |
| Leveling ladders (Isildur) | forums.carrionfields.com dcboard topic **2032** (forum=5) |
| Item DB (3,197 items, full stats) | `https://carrionfields.net/itemsearch` |
| Local itemsearch seed dump | `%LOCALAPPDATA%\LitD\research\cf-itemsearch-dump-2026-07-12.txt` |
| PBF character sheets | limited-gear intel (limited items itemsearch won't show) |

Key helpfile IDs (full inventory in references/cf-sources.md):
hide 503, sneak 1169, camouflage/acute-vision/waylay 467, creep 470, quiet movement
483, duo dimension 391, pass door/control translucence 438, detect hidden 489, detect
invis 384, faerie fire 400, faerie fog 401, invis 433, bash 480, flee 266, wimpy 291,
rescue 514, experience 96, groups 301/2791, practice 306, skill improvement 2103,
cast 260, commune 261, botting 3046, prompt 1546, limited items 61, outfit 314,
academy 1159, thief guilds 2167, newbie areas 2523, maps 2924.

## TRAPS (verified 2026-07-12) -- do not get burned

1. **Necromancer wiki class page renders EMPTY to fetchers.** Do NOT trust a blank
   pull as "no data." Use helpfiles **233 / 413 / 523 / 234** instead.
2. **Angelfire area list (`angelfire.com/ma/terryma/maps/`) is 1998-era CF** -- contains
   REMOVED areas (Camelot, Shire, Moria). Do not treat as current world layout.
3. **The QHCF ASCII world map garbles column alignment when fetched** -- treat area
   adjacency as APPROXIMATE, not authoritative pixel geometry.
4. **`#NNNN` refs inside vendored Calyx/ code are Calyx-Dev's OWN tracker**, NOT this
   repo's GitHub issues. `gh issue view NNNN` to confirm before acting on one.

## What to capture per content type

- **Race** (#985/#1000-#1019): stat caps STR/INT/WIS/DEX/CON/CHA, alignment restriction,
  ethos, size, resistances/vulnerabilities, innate/inherent abilities, infravision,
  allowed classes, experience penalty, hometown. Framework lives in
  `crates/litd-engine/src/rpg/race_specs_types.rs` (RaceSpec, schema litd.rpg.race_spec.v1).
- **Class** (#1019-#1037): prime stat, align/ethos+race restrictions, full leveled
  skill/spell table, specializations (weapon-spec/legacies/paths/forms).
- **World** (#1598): topology, road spine, leveling bands (`areas 1 5`..`areas 45 51`),
  hometowns/recall, per-area rosters (Isildur topic 2032; newbie routing 2523).
- **Items** (#1682-#1690): level/weight/material/flags/5-channel AC/affects/weapon dice;
  re-scrape itemsearch fresh at build time (site filters support type/slot/material/
  weapon-class/damage-type/affect/flag/area for chunked pulls).

## The rule

Mechanics/stats/layout = factual, cite the id. Names/lore = originalize per #984.
When you feed these into an issue, follow /issueflow (parent refs first, ids inline,
FSV gate last). See references/cf-sources.md for the complete ID/URL inventory.

## References
- references/cf-sources.md -- full helpfile-ID + URL inventory, epic map, per-type fields.
