# Light in the Dark — Flagship Game Identity

> Decision D-2026-06-11-32 (owner-decided, research-grounded). The M6 vertical slice is
> v0.1 of THIS game (D-24). Inspirations researched 2026-06-11: Warcraft III lore
> structure and art direction, Age of Wonders 4 progression systems, Age of Mythology
> Retold studied and deliberately *not* adopted for its deity layer. Sources at bottom.
> **IP guardrail (NG1): structure and themes are inspiration; every name, character,
> place, and text in this document and the game is original.**

---

## 1. Title and premise

**Title: Light in the Dark.** Game = engine = brand. Release subtitles per major version
(working subtitle for v0.1: *First Flame*).

**Premise.** The world of **Veil** has no sun. Civilization exists in the radiance of the
**Beacons** — ancient, dwindling pillars of light raised by a vanished people, each
sustaining a habitable haven amid the **Gloam**, the living darkness between. The Beacons
are going out, one by one, and nobody knows why. Every faction is an answer to the same
question: *what do you do when the light is running out?*

This premise is engineered for the platform vision (PRD §1.0): "explain an idea" maps to
"light a beacon in the dark" — the game's fiction and the product's purpose rhyme.

## 2. Factions (4 playable + 1 antagonist)

WC3's structural lesson, applied: four asymmetric playable factions occupying distinct
philosophical answers, plus a pure-antagonist force that never gathers gold or builds
farms — the threat stays mythic (the Burning Legion lesson, [Warcraft Retrospective 17]).

| Faction (working name) | Answer to the dying light | Role analogue | Identity |
|---|---|---|---|
| **The Vigil** | *Defend what remains.* Feudal beacon-keeper kingdom; knights, clerics, siegecraft; lawful, dutiful, brittle — their nobility hides rot and zealotry. | WC3 Human | Versatile, defensive, light-infused support magic |
| **The Gloamborn** | *Embrace the dark and rule it.* Those who walked into the Gloam and came back changed; they harvest the dark as a resource. The playable corruption arc lives here. | WC3 Undead/Scourge | Attrition, conversion, terrain-blight ("gloamspread"), cheap swarms + elite horrors |
| **The Unbound** | *Outrun it.* Nomad clans who abandoned the beacons generations ago; fire-carriers, beast-riders, storm-callers; savage reputation, honorable core, seeking redemption from an old betrayal. | WC3 Orc/Horde | Aggression, mobility, portable light (carried flame = moving vision/aura) |
| **The Rootkin** | *Outlast it.* Ancient grove-beings and their fey symbionts who remember the world before the Beacons; their groves make their own light (bioluminescence). Isolationist guardians forced into the war. | WC3 Night Elf | Stealth at night, regeneration, terrain symbiosis, awakening dormant titans |
| **The Dark** (unplayable) | *It is not an answer; it is the question.* Not evil — hungry. The Gloam's will, manifesting through extinguished beacons. Campaign antagonist and neutral-hostile presence on every map. | Burning Legion | Never builds, never gathers; arrives in escalating manifestations |

Asymmetry budget: factions differ in economy texture, light/vision mechanics, and army
shape — within the engine's data-table-driven unit/ability model (R-AST-1). No
engine-level special casing per faction.

## 3. Light as the central mechanic

The fantasy and the RTS systems unify around light = safety = vision = territory:

- **Beacons and flame**: control points and base hearts. The Vigil fortifies them, the
  Unbound carry portable flame, the Gloamborn extinguish and invert them, the Rootkin
  grow their own. Fog of war (already custom render work, 05-rendering) is
  fiction-load-bearing: the dark on your screen IS the antagonist.
- **Day/night analogue — the Flicker**: Veil has no sun, but Beacons pulse on a cycle;
  the dim phase (the Flicker) empowers Gloamborn/Rootkin night mechanics, echoing WC3's
  day/night gameplay meaning ([Ars Technica]).
- **The Dark escalates** on maps as beacons fall — a shared environmental clock that
  pressures turtling and rewards map control.

## 4. Heroes (the magic carrier — no deity layer)

Owner decision: **no patron-deity/god-power layer** (AoM's signature studied and
declined). Heroes ARE the supernatural element, per WC3's "rock stars with battle axes"
philosophy ([Ars Technica]) — every hero should look like they could take on an army.

Adopted from AoW4 ([GameRant faction guide], [Finger Guns review]), all game-layer data:

- **Skill trees on level-up**: each hero levels (WC3-style XP) and chooses along a
  3-branch tree (Battle / Command / Mysticism analogues) — choice, not fixed kit-unlock.
- **Tiered gear**: items tier 1–4, found/bought/crafted; inventory already in the
  canonical API (items category).
- **Grimoires** (AoW4 Tomes, renamed): in-match themed research tracks replacing flat
  upgrade lists — e.g. *Grimoire of the Ember Road* (Unbound fire/mobility) vs *Grimoire
  of the Long Vigil* (defense/healing). Each grants units, abilities, upgrades; choosing
  one forecloses another → build variety per match.
- **Transformations**: capstone grimoires permanently transform your army for the match
  (winged vanguard, ember-skinned, gloam-touched…) — AoW4's most-loved feature, deeply
  "medieval magic" in feel.
- **Faction customization** (AoW4 culture/traits/tomes model) ships at **M8 with the
  World Editor** as its marquee feature: player-made factions are data tables + Lua —
  the engine is already shaped for it.

## 5. Campaign structure

WC3's progressive multi-faction storytelling ([wowwiki RoC]): one continuous story told
through consecutive faction campaigns, each handing off to the next, converging on an
alliance finale against the Dark. Centerpiece: a **corruption arc** — a Vigil hero's
fall to the Gloamborn across campaigns one and two (the Arthas deconstruction: loss of
agency under the player's own control, [Shapes deep-dive]) — and a **redemption arc**
mirroring it in the Unbound campaign. Campaign persistence (D-15) carries heroes and
choices between missions.

v0.1 (M6) ships skirmish only; campaign missions begin at M8 with the editor's
mission-flow tooling. Lore/art identity applies from the first skirmish map.

## 6. Art direction (binding for the generative pipeline, D-12)

Original WC3's actual rules, from its artists ([Ars Technica], [Game Informer],
[MadmadeLabel]) — these are **prompts and validation criteria for `tools/assetgen`** and
curation rules for CC0 pack selection:

1. **Comic-book fantasy, not realism.** "Superhero versions of fantasy characters."
   Realistic proportions read as small and confusing from RTS camera — proven failure.
2. **Exaggerated proportions, big silhouettes.** Bulky shoulders, oversized weapons,
   readable at a glance in battle chaos. Silhouette-first design.
3. **Hand-painted flat-color textures carry the detail** on low-poly meshes (KISS
   method). Matches our KayKit/Quaternius CC0 baseline and the single-atlas rule
   (R-RND-2) exactly.
4. **Saturated, bold palette.** Faction color identity: Vigil golds/blues, Gloamborn
   violets/sickly greens, Unbound ember reds/ochres, Rootkin teals/moonlit silvers,
   the Dark = absence (desaturation + light-eating black).
5. **Medieval + magic only.** No gunpowder beyond alchemical siege, no clockwork-tech
   aesthetic, no modern or sci-fi elements anywhere.
6. **Readability over fidelity, always** — value structure and shape language before
   detail ([80.lv]). The Reforged backlash (lean realistic units = readability loss,
   [Blizzard forums]) is the cautionary tale.

## 7. What we deliberately did NOT take

- **AoM patron gods / god powers / favor resource** — declined (owner). Heroes carry
  the power fantasy; no fourth resource.
- **AoM myth-unit rock-paper-scissors tier** — mythic creatures exist inside faction
  rosters and grimoire summons, not as a separate counter class.
- **AoW4 4X layer** (turn-based, provinces, diplomacy) — wrong genre; only its
  progression DNA (tomes→grimoires, hero trees, transformations, faction creator).
- **WC3 lore content** — names, places, characters, events all original (NG1).

## 8. v0.1 (M6) scope of this identity

Two factions playable (the Vigil and the Unbound — clearest art coverage from CC0 packs
+ assetgen), one skirmish map with Beacon/Flicker mechanics, heroes with skill trees and
items, one grimoire track per faction, win/lose vs the M5.5 AI. The Gloamborn, Rootkin,
the Dark's campaign manifestation, transformations, and faction customization layer in
across M7–M9.

---

### Sources (researched 2026-06-11)

- [Ars Technica — How Warcraft III birthed a genre](https://arstechnica.com/features/2020/01/how-warcraft-iii-birthed-a-genre-changed-a-franchise-and-earned-a-reforge-ing/) — art philosophy quotes (Didier)
- [Game Informer — WC3 concept art origins](https://gameinformer.com/2018/11/14/explore-warcraft-iiis-origins-in-this-rare-concept-art-gallery) — readability-driven proportions
- [MadmadeLabel — Samwise Didier retrospective](https://madmadelabel.com/article/architect-of-azeroth) — KISS texturing, silhouette bulk
- [80.lv — Mastering Blizzard's stylized art](https://80.lv/articles/matt-mcdaid-mastering-the-stylized-art) — readability sub-elements
- [Warcraft Retrospective 17/18](https://lintian.eu/2024/03/01/warcraft-retrospective-17/) — faction-count history, Legion unplayability lesson
- [Shapes — WC3 deep dive](https://shapes.inc/fandom/warcraft-iii/deep-dive) — Arthas arc as loss-of-agency deconstruction
- [GameRant — AoW4 faction creation](https://gamerant.com/age-of-wonders-4-faction-creation-guide/), [Finger Guns — AoW4 review](https://fingerguns.net/reviews/2023/05/02/age-of-wonders-4-review-pc-the-godir-await/) — tomes, hero trees, transformations
- [GameRant — AoM Retold gods](https://gamerant.com/age-of-mythology-retold-major-minor-god-norse-greek-egyptian/), [VideoGamer — AoM Retold review](https://www.videogamer.com/reviews/age-of-mythology-retold-review/) — the deity layer we declined
- [Blizzard forums — Footman analysis](https://us.forums.blizzard.com/en/warcraft3/t/graphics-an-analysis-of-the-footman/1274) — Reforged readability cautionary tale
