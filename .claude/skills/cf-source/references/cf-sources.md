# CF source inventory (full)

Complete URL/ID map for Carrion Fields factual transcription. ASCII only. See SKILL.md
for the quick recipe and traps. All mechanics here are FACTUAL (transcribe, do not
invent); originalize only names/lore per #984/#1670.

## Fetch endpoints

- Helpfile by id: `https://carrionfields.net/helpsearch.php?id=NNN`
- Helpfile by keyword: `https://carrionfields.net/helpsearch.php?keywords=<words>`
- Class page: `https://wiki.carrionfields.net/class/<name>`
- Race page: `https://wiki.carrionfields.net/race/<slug>`
- World map (ASCII, columns garble on fetch -> adjacency approximate):
  `http://wiki.qhcf.net/index.php?title=WorldMap`
- Area list (explore-tier area names): `http://wiki.qhcf.net/index.php?title=AreaList`
- Isildur's Ranking Guide (good/evil leveling ladders 1-51, updated 2022):
  `https://forums.carrionfields.com/dcboard.php?az=show_mesg&forum=5&topic_id=2032&mesg_id=2032`
- Item DB (3,197 browsable items, full stats; filters: type/slot/material/weapon-class/
  damage-type/affect/flag/area): `https://carrionfields.net/itemsearch`
- Local itemsearch seed dump (703KB, captured 2026-07-12; RE-SCRAPE fresh at build
  time): `%LOCALAPPDATA%\LitD\research\cf-itemsearch-dump-2026-07-12.txt`
- PBF character sheets: limited-gear intel for items itemsearch will not show.

Fetch via exa: `mcp__exa__web_fetch_exa` (batches URLs). Helpfiles maxCharacters ~2000;
class/race/forum pages ~6000-10000. `mcp__exa__web_search_exa` to find an unknown id/slug.

## Helpfile ID inventory

Stealth / concealment:
- hide 503, sneak 1169, camouflage + acute vision + waylay 467, creep 470,
  quiet movement 483, improved hide 1421, shadow disappear 1425, vanish 521,
  stalk 744, heightened awareness 730, treeform 610, forest haven 621.

Perception / detection:
- detect hidden 489, detect invis 384, faerie fire 400, faerie fog 401, invis 433.

Transmuter / dimensional:
- duo dimension 391, pass door + control translucence 438.

Combat / survival / positional:
- bash 480, flee 266, wimpy 291, rescue 514, assassinate 469, track 478 (also 476/477).

Advancement / grouping / economy:
- experience 96, groups 301 and 2791, practice 306, skill improvement 2103.

Magic verb families:
- cast 260, commune 261.

Items / gearing:
- limited items 61, outfit 314.

World / onboarding / policy:
- thief guilds by city 2167, newbie areas + directions 2523, academy 1159, maps 2924,
  prompt (HUD variables) 1546, botting policy 3046.

Necromancer (wiki page renders EMPTY -- use these instead): 233, 413, 523, 234.

Extra stealth-epic ids seen in #1597 source line (verify by fetch when needed):
384, 469, 744, 730, 610, 621, 476, 477, 478, 1569, 2185, 2696, 2318, 2344.

## Traps (verified 2026-07-12)

1. Necromancer wiki class page = EMPTY to fetchers -> helpfiles 233/413/523/234.
2. Angelfire `angelfire.com/ma/terryma/maps/` = 1998-era CF; has REMOVED areas
   (Camelot, Shire, Moria). Not current.
3. QHCF ASCII WorldMap garbles column alignment on fetch -> adjacency APPROXIMATE.
4. `#NNNN` inside vendored Calyx/ code = Calyx-Dev's OWN tracker, NOT this repo's
   GitHub issues. `gh issue view NNNN` before acting.
5. exa fetch, not plain WebFetch (WebFetch has failed on CF for agents).

## Per-type capture checklist

Race (#985 parent; #1000-#1019 children; framework
`crates/litd-engine/src/rpg/race_specs_types.rs`, RaceSpec schema litd.rpg.race_spec.v1):
stat caps (STR/INT/WIS/DEX/CON/CHA), alignment restriction, ethos, size,
resistances/vulnerabilities, innate/inherent abilities, infravision, allowed classes,
experience penalty, hometown, visual model (#782). Verified sample: Elf XP 400 / Good /
"very smart & quick but frail"; Wood-Elf STR max 19, classes Warrior/Ranger/Druid/Bard/Healer.

Class (#1019-#1037): prime stat, alignment/ethos + race restrictions, full leveled
skill/spell table, specializations (warrior weapon-spec/legacies, thief paths/points,
shaman/invoker paths, shapeshifter forms/foci, necromancer undead/becoming).

World (#1598 epic): overworld topology + road spine (Galadon hub -> Voralian W,
Hamsah Mu'tazz S, Udgaard/Hillcrest N, Arkham/Barovia E, Seantryn Modan SE), ~250 areas
/ 23k rooms, leveling bands `areas 1 5`..`areas 45 51` (Isildur topic 2032), newbie
routing (helpfile 2523), hometowns/recall, Underdark below, seas around, The Past + planes.

Items (#1682-#1690): name, source area, type, wear slot, material, level, weight, flags,
5-channel AC, weapon class/damage-type/average, full affect list (incl age/morale). Wire
source area to world nodes (#1632) and drops/resets (#1668).

## Related epics / issues

- Stealth epic #1597 (children #1599-#1629; roster pinned as a comment).
- World mirror epic #1598 (children #1632-#1671).
- System layer #1672-#1680 (MUD-to-3D doctrine #1672, combat #1673, positional #1674,
  magic #1675, summons #1676, leveling #1677, grouping #1678, vitals #1679, balance #1680).
- Items #1682-#1690; casting/UX #1695-#1703; buff/prep #1691-#1693.
- Content-source doctrine: #984 (originalize names), #985 (race framework), #1670
  (renaming ledger). Acquisition matrix example: #1623.
