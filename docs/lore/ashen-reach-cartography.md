# Ashen Reach Cartography Canon

Canonical detail for issue #1448 under #944 acquired-map doctrine and the
#1055-#1059 Ashen Reach canon. This document defines player-facing regional
cartography for Vhael without creating a built-in RPG map, minimap, route line,
or character-position marker.

## Originality Boundary

This is original LitD setting expression. The idea of a parchment map, a
cartographer, roads, ferries, danger marginalia, and inaccurate medieval
proportions is generic historical and fantasy structure. The protected
expression here is ours: Vhael, Galdmere, the Ashen Reach, the Vellum of Ember
Roads, named routes, annotations, provenance, faction marks, and quest hooks.

Research references inform generic art direction only. The Hereford Mappa
Mundi is useful because it combines towns, beasts, stories, worldview, and
vellum material into one artifact. The LitD map must not copy its layout,
religious program, drawings, labels, or composition.

Reference sources used:

| Source | Use |
|---|---|
| https://www.herefordcathedral.org/mappa-mundi | Official reference for mappa mundi as a vellum, worldview, town, creature, and story artifact. |
| https://www.themappamundi.co.uk/mappa-mundi/ | Interactive reference for medieval map categories, material damage, beasts, towns, myths, and routes as art-direction pressure. |

## Runtime Rule

A fresh RPG character receives no map surface for this canon. The Vellum of
Ember Roads is visible only after the character gains an in-world source:
cartographer purchase, recovered copy, archive transcription, faction grant, or
quest reward.

The player-facing map never draws the character's current position, never draws
a live route ribbon, never becomes an exact survey grid, and never replaces
directions from roads, signs, rumors, journal text, and landmarks. Editor map
previews remain creator tooling and are not player-runtime access.

## Regional Map: The Vellum Of Ember Roads

The first durable player-facing regional map is the **Vellum of Ember Roads**,
a Galdmere-made parchment chart of central and eastern Vhael. It is useful
enough to orient a traveler, but its proportions are biased by trade roads,
faction fears, and what the maker could safely ask about the Reach.

Linked original visual asset:
`docs/assets/ashen-reach-vellum-of-ember-roads.svg`.

| Field | Canon |
|---|---|
| Map item id | `map.vhael.vellum_of_ember_roads` |
| Item name | Vellum of Ember Roads |
| Region | Galdmere, central Vhael, and the western approaches to the Ashen Reach |
| Default access | None; must be acquired through authored gameplay state |
| First source | Bought from Sera Vell, cartographer of the Galdmere East Gate stair |
| Better copies | Vesper archive copy, Iron Verdict road writ, Dawnward lamp-road survey, Thornwild bark-copy, Drakemarch scale-rubbing |
| Known limits | Eastern Reach distances are compressed, Hollowgate depth is symbolic, Drakemarch lairs are deliberately vague |

## Required Places

The Vellum of Ember Roads includes every #1057 root place, but not with modern
survey confidence.

| Place | Map Treatment | Reliability |
|---|---|---|
| Galdmere | Large city mark beside the Mourning Wall and East Gate road. | High |
| Ashen Reach | Torn grey eastern zone with broken ravines and ash warnings. | Low |
| Vael Mourn | Salt-bell coast mark with drowned archive glyphs. | Medium |
| Karthspire | Black mountain-fort mark west of the imperial road. | Medium |
| Thornwilds | Sprawling forest mass that crowds roads and hides bridge marks. | Medium |
| Hollowgate Deeps | Cave-mouth warning drawn below the road layer, not to scale. | Low |
| Sunspire | White citadel and lamp-road mark. | High |
| Verdict Halls | Fortified court mark on the causeway road. | High |
| Drakemarch | Highland dragon-margin with lair warnings instead of exact lair dots. | Low |

## Route Lore

Routes are written as remembered travel, tolls, warnings, and rumors rather
than coordinates.

| Route | Kind | Lore |
|---|---|---|
| Old Ash Road | road | Leaves Galdmere's East Gate beneath the black arch and frays into ash before the Reach. |
| Broken Lantern Shrine Road | shrine road | A charred sign points from Old Ash Road toward the shrine where lamp oil still refuses rain. |
| Grey Ravine Track | pass | The track is drawn as a torn notch because maps disagree where stone becomes ash. |
| Bell Ferry | ferry | Low-tide ferry to Vael Mourn; the ferryman asks for a name, not only coin. |
| Karth Pass | mountain pass | Onyx Dominion patrols levy tolls and alter mile-counts on hostile copies. |
| Thorn Bridge | bridge | Thornwild root-bridge appears only on copies traded through greenward guides. |
| Verdict Causeway | faction crossing | Iron Verdict writs mark legal passage and prisons along the causeway. |
| Lamp Road | faction road | Dawnward lantern stones lead toward the Sunspire during Hollow fog. |
| Hollowgate Descent | under-road | Drawn as a cave mouth below the parchment road; safe depth is intentionally omitted. |
| Drakemarch High Road | highland road | The line fades into dragon marginalia where scouts stop agreeing. |

## Style Direction

The map reads as a handled medieval object, not a modern UI overlay. Its
imperfect proportions are intentional: social importance, danger, and rumor
weight matter more than measured scale.

| Style Element | Direction |
|---|---|
| Surface | Warm vellum/parchment, stains, pinholes, rubbed fold lines, darker torn eastern edge. |
| Ink | Hand-inked roads and rivers with uneven stroke width. |
| Proportion | Social and danger importance outweighs accurate scale; Galdmere and faction seats are oversized. |
| Symbols | Faction seals, lamps, bells, court marks, root marks, cave mouths, dragon teeth, and ash plumes. |
| Marginalia | Monster warnings, toll notes, crossed-out rumors, and "ask at Galdmere" annotations. |
| Unknown Reach | Grey washes, torn gaps, overwritten route names, and warnings that the road changes after storms. |
| Forbidden UI Drift | No live character marker, no exact coordinate grid, no automatic quest pin, no route ribbon. |

## Provenance

Sera Vell compiled the base copy in Galdmere from toll ledgers, Vesper songs,
Iron Verdict writs, Dawnward lamp-road notes, caravan scars, and Thornwild
warnings. She has not crossed the Grey Ravine herself; her Reach notes are a
collage of paid rumors, dead courier marks, and one ash-stained rubbing from a
traveler who never returned.

Known alterations:

| Alterer | Change | Gameplay Meaning |
|---|---|---|
| Vesper Court copyists | Add memory bells and erased-name marks near Vael Mourn. | Better archive quests, worse road distances. |
| Iron Verdict clerks | Mark lawful crossings, prisons, and warrants. | Better legal travel, visible faction risk. |
| Thornwild guides | Add living bridge and root-shelter marks. | Better forest travel, missing city toll detail. |
| Onyx Dominion surveyors | Straighten imperial roads and inflate Karthspire reach. | Useful for Dominion patrols, politically biased. |
| Dawnward lamp-bearers | Add lantern stones and Hollow fog shelters. | Better safety notes, poorer smuggler paths. |

What is intentionally missing:

| Missing Fact | Reason |
|---|---|
| Exact Hollowgate depth | The mapmaker refuses to sell suicide as certainty. |
| Ashen Dragon lairs | Drakemarch hunters alter copies to protect or bait rivals. |
| Current Thorn Bridge location | The bridge grows and is traded as guide knowledge. |
| Final Ashen Reach distance | Roads shift after ash storms and thin-Weave weather. |

## How Better Maps Are Earned

Cartography is progression content.

| Better Map | Source |
|---|---|
| East Gate copy | Buy from Sera Vell after reaching Galdmere. |
| Vesper bell-copy | Earn archive access in Vael Mourn. |
| Lamp-road survey | Earn Dawnward trust or escort a lantern caravan. |
| Thorn bark-copy | Trade with Thornwild guides after saving a root-shelter. |
| Verdict writ map | Win legal standing or serve a court warrant. |
| Drakemarch scale-rubbing | Survive a dragon-hunter route and return with witness. |
| Ashen approach fragment | Recover from the Broken Lantern Shrine quest chain. |

## Quest Route Hook

The Vellum of Ember Roads supports the first validated Ashen Reach route from
#1447 without becoming a GPS layer.

Quest: `quest.ashen_approach`

Map function:

1. The map shows Galdmere and the Old Ash Road leaving by the East Gate.
2. The Old Ash Road line breaks beside the Broken Lantern Shrine symbol.
3. A grey notch beyond the shrine is labelled "ravine where lamp smoke falls."
4. The Ashen Reach edge is washed out and says "follow the ash plume only if
   Nella's rumor agrees."
5. Journal text and NPC rumor remain required; the map alone is intentionally
   ambiguous.

The player reads the map, compares it to Nella's Galdmere rumor, follows the
Old Ash Road sign, finds the Broken Lantern Shrine, uses the Grey Ravine notch,
and then navigates by the ash-plume silhouette. The artifact provides
landmarks and provenance; it does not point at the player or draw a live path.

## Linked SVG Asset Contract

The linked SVG is an inspectable authoring/asset reference for the Vellum of
Ember Roads. It must include all nine root places, at least four named routes,
at least three faction/provenance symbols, grey/torn Ashen Reach treatment, and
a no-grid/no-current-position visual language. It may be used by editor tools
as a source asset for acquired map items, but it is not a default player HUD.

## Canon Links

| Issue | Boundary |
|---|---|
| #944 | Acquired maps only; no built-in RPG map/minimap/world-map. |
| #1055 | Root world bible and originality covenant. |
| #1057 | Root places and travel pressure. |
| #1058 | Faction identity and symbols. |
| #1059 | Mourning, legends, Ashen Dragons, and map doctrine. |
| #1205 | World-knowledge graph traversal usable for route validation. |
| #1445 | Runtime acquired-map access policy. |
| #1447 | In-world wayfinding validation for the Galdmere-to-Reach route. |
| #1448 | This cartography canon. |

## FSV Requirements

For #1448 closeout, source-of-truth verification reads this physical Markdown
file and `docs/assets/ashen-reach-vellum-of-ember-roads.svg`, hashes both,
confirms the nine root places, route lore, style direction, provenance, missing
facts, better-map progression, linked asset, #1447 quest route hook, #944
source gating, originality boundary, and current no-GPS/no-current-position
rules. It must manually inspect the SVG/rendered report through Synapse and
confirm no player-position marker, exact coordinate grid, automatic quest pin,
or route ribbon appears.
