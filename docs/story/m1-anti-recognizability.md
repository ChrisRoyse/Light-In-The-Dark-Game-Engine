# M1 Anti-Recognizability Audit

Section of the Milestone-1 story bible (docs/story/m1-story-bible.md). Doctrine #1848
rule 1: the world must be ours -- so unique no one can trace it to any existing franchise.
This file audits every name and structural element and logs the franchise-collision web
searches that back the invented names. ASCII-only.

## A. Method

Two checks per element:
1. Name check: does the exact name (or a near-homophone) belong to a known franchise?
   Verified by web search; rejections and replacements are recorded.
2. Concept check: is the element a recognizable creature/plot/iconography from any
   source (orcs, elves-as-known, a borrowed pantheon, a named myth reused)?

Studied PD tragedies (Kalevala, Norse Eddas, Volsunga saga, Aristotle) informed only
STRUCTURE (irreversible choice, doomed loyalty, oath-cost, tragedy-from-error). No name,
creature, deity, place, or plot from them is reused. Details in the influence log.

## B. Invented names introduced by this bible -- verdicts

- Oruvenn (the world). Search: no franchise collision (returns unrelated settings
  Ordinem, Auremn, Orlec, Orrek). VERDICT: original, cleared.
- Corramere (the fallen hearth-city). Search: no collision (distinct from Castlemourn
  and Caldemount, which were checked and avoided). VERDICT: original, cleared.
- the Guttering (the catastrophe). Search: no fantasy-franchise collision (a candle/
  flame guttering-out is a common English image; no named-event reuse found). VERDICT:
  original, cleared.
- the Unturning (the metaphysical law that nothing reverses). LINEAGE (honesty pass): this
  is a deliberate literal-English abstraction of Atropos, the Greek Fate whose name means
  "the unturning one" (a- "not" + tropos "turning") -- public-domain Greek myth (the
  Moirai). We took the STRUCTURAL idea (an inexorable fate that is never turned aside) and
  the plain-English gloss, NOT the name Atropos, the Moirai, or any Greek plot or
  iconography. A search as a named fantasy-franchise principle surfaced no direct collision
  (nearest hit "Stone Unturned: A Legend of Ethshar" is a different phrase/work), but we do
  NOT claim absolute novelty: "unturning" is a common English participle and the Atropos
  gloss is public domain. Operator decision (2026-07-14): KEEP "the Unturning" as a
  public-domain abstraction, with the Atropos lineage documented (here and in the influence
  log). NOTE: replaced an earlier working term "the Ledger" as the PRIMARY principle name
  after the search below surfaced an existing game titled "Ledger of Ash"; "ledger" now
  appears only as an ordinary common noun (mortals keep ledgers of deeds), not as a titled
  concept.
- wardsmithing (Emberhold magic). Chosen AFTER rejecting "emberwright," which the search
  surfaced as an existing Steam achievement ("The Emberwright," Against the Storm) and
  near the EMBERWIND TTRPG. RECLASSIFIED (honesty pass): "wardsmithing"/"wardsmith" is NOT
  collision-free -- prior uses exist (a DC fan-universe "Wardsmithing" magic category; a
  "Wardsmith" on the Discworld MUD). It is retained under the generic-compositional-
  vocabulary carve-out: "ward" + "smith" is ordinary English fantasy compounding (one who
  smiths wards), not a coined mark owned by any franchise. By contrast the bible's own
  coined term "shadeweave" cleared cleanly, with no prior use found.
- shadeweaving / shadeweave (Veilspun magic). Chosen AFTER rejecting "veilcraft," which
  the search surfaced as the core magic term of a fan chronicle ("Avelir: The Broken
  Veil") whose premise (magic-as-debt paid in years; two civilizations mirror-split over
  the Veil; currency of "singular loss") was alarmingly close to an earlier draft of our
  magic-cost idea. We changed BOTH the term and the mechanic: our magic is not a
  debt-in-years system; Miren's death runs on oath-logic (a sworn ward cannot be lowered),
  not a life-debt. "Shadeweave" returned no franchise collision. VERDICT: original,
  cleared; near-collision actively designed away.
- "Held to the Last" (campaign title). Chosen AFTER rejecting "The Ledger of Ash," which
  the search surfaced as multiple existing works (an itch.io fantasy RPG "Ledger of Ash";
  AI-tale and romance-novel titles). "Held to the Last" is a common English phrase with
  no franchise ownership. VERDICT: cleared.

## C. Phase-1 locked names (pre-existing; #1788 factions.rs) -- audit only

These were authored in phase-1 and are not changed here; audited for the record.

- Emberhold Compact, Veilspun Court (factions). DISCLOSURE (honesty pass): "Emberhold"
  collides with an existing Steam game titled "Emberhold" (Pacific Ember Studios, Steam
  app 3324490). This is an ACCEPTED collision, not a clean clear. Two mitigations stand on
  the record: (1) the full faction name is "Emberhold Compact," which differs from the
  standalone game title; (2) "ember" + "hold" is generic compositional English fantasy
  vocabulary (a stronghold that holds the ember/flame), covered by the generic-
  compositional-vocabulary carve-out rather than being a coined mark exclusive to that
  title. The name was locked in phase-1 (#1847) and is not changed here. Operator decision
  (2026-07-14): KEEP "Emberhold Compact" with the collision disclosed honestly. Veilspun
  Court returned no franchise collision. Concept (two trauma-split halves of one people)
  is original.
- Doran Ashheart (Emberhold hero). NOTE: the first name "Doran" is shared with a minor
  character in a well-known fantasy series (A Song of Ice and Fire's Doran Martell). The
  FULL name "Doran Ashheart" is original and the character is unrelated; risk is low.
  Flag retained for operator awareness; no change proposed to locked content.
- Sister Miren, Yvenne Duskblade, Caul the Whisperer (heroes). No traceable franchise
  collision on full names; original. Cleared. ("Duskblade" is also a Veilspun unit name
  in phase-1; internal reuse, not external.)
- Ashford Crossing (map). "Ashford" is a real English place name and a common toponym;
  "Ashford Crossing" is not a franchise property. Cleared as generic/original.
- Unit/structure/tech/ability/item names (Shieldsworn, Bastion Golem, Nightstalker,
  Gloomwing Raider, Warding Light, Shadowstep, etc.). These are phase-1 locked. "Golem"
  and "Shadowstep" are generic fantasy/game vocabulary, not franchise-exclusive; the
  compound names are original. No orc/elf/borrowed-pantheon content. Cleared with the
  standing note that generic fantasy vocabulary (golem, bastion, sentinel, halberdier) is
  common usage, not traceable IP.
- Shadow Weave (phase-1-locked Veilspun armor upgrade, faction_data.rs). RECLASSIFIED
  (honesty pass): this collides with "the Shadow Weave," a well-known Forgotten Realms
  (D&D) concept. The name is phase-1 locked (#1847) and is NOT changed here; it is retained
  under the generic-compositional-vocabulary carve-out ("shadow" + "weave" is common
  fantasy usage), NOT as a collision-free coinage. It is distinct from the bible's own
  coined "shadeweave" (the Veilspun magic term), which cleared cleanly with no prior use.

## D. Concept-level audit (creatures, plots, iconography)

- No orcs, no elves-as-known, no dwarves, no borrowed gods. The two peoples are one
  original human people fractured by one original event; no non-human "races" are
  introduced by this bible. (Phase-1 unit "beasts" like Shadowmaw / Bastion Golem are
  constructs/creatures of the factions' own magic, not a borrowed bestiary.)
- No reused myth CONTENT. The Guttering, Corramere, and shadeweaving are invented. "The
  Unturning" is a public-domain literal-English gloss of the Greek Fate Atropos (see the
  Section B entry and the influence log) -- a PD abstraction, not a reused name or plot.
  Structural inspirations (doom-inevitability, doomed loyalty, oath-cost, tragedy-from-
  error) are patterns, not content, and are not attributable to any single source's names
  or plot.
- No time-magic iconography (no hourglasses-that-rewind, no chrono relics) -- forbidden by
  rule 3 and absent by design.
- Gunpowder ceiling respected: no firearms, cannon, industry, or electricity anywhere in
  the setting; ranged weapons are torsion/bow/glider (rule 2).

## E. Collision-search log (evidence)

Searches were run via the exa web tools during authoring (2026-07-14). Summary of hits:
- "Caldmourn" -> collided with D&D "Castlemourn" (Ed Greenwood) and "Caldemount";
  REJECTED (city renamed Corramere).
- "veilcraft" -> collided with fan chronicle "Avelir: The Broken Veil" incl. premise;
  REJECTED (magic renamed shadeweaving; mechanic changed).
- "Ashmoor"/"Ostrenmark" -> "Ashmoor" collided with multiple settings (Time Will Tell
  RPG; D&D "Ashenmoor"; a podcast); REJECTED (world named Oruvenn instead).
- "emberwright" -> collided with an Against the Storm achievement and near EMBERWIND;
  REJECTED (magic renamed wardsmithing).
- "Ledger of Ash" -> collided with an existing itch.io RPG and other titles; REJECTED
  (campaign titled "Held to the Last"; principle renamed the Unturning).
- "Oruvenn", "Corramere", "the Guttering", "shadeweave" -> no franchise collision found;
  ACCEPTED (clean).
- "the Unturning" -> no direct named-principle collision, but it is an intentional
  public-domain gloss of Atropos/the Moirai (not a clean coinage); ACCEPTED as a PD
  abstraction with lineage disclosed (operator keep decision, 2026-07-14).
- "Emberhold" (in the phase-1-locked faction "Emberhold Compact") -> COLLIDES with the
  Steam game "Emberhold" (Pacific Ember Studios); ACCEPTED with disclosure (operator keep
  decision, 2026-07-14; full name differs; generic compositional vocabulary).
- "wardsmithing"/"wardsmith" -> prior uses exist (DC fan-universe category; Discworld MUD);
  and "Shadow Weave" (phase-1-locked upgrade) collides with Forgotten Realms -> both
  RETAINED under the generic-compositional-vocabulary carve-out, not claimed collision-free.

VERDICT (honest): the bible's own coined terms (Oruvenn, Corramere, the Guttering,
shadeweave, "Held to the Last") are clear, and no reused myth/creature/pantheon CONTENT
appears. Known non-clean names are disclosed and retained by decision: "Emberhold" (an
accepted Steam-title collision), "the Unturning" (a PD Atropos gloss), and "wardsmithing"/
"Shadow Weave" (generic-compositional-vocabulary prior uses). Two further low-risk notes
are flagged for operator awareness: the shared first name "Doran"; and generic fantasy
vocabulary in phase-1 unit names.
