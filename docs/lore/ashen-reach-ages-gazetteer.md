# Ashen Reach Ages And Gazetteer

Canonical detail for issue #1057 under the #1055 Ashen Reach world bible and
the #1056 cosmology canon. This document defines the historical ages of Vhael,
the root places of the realm, and the current implementation language for
Editor-authored zones on the LitD Rust/wgpu engine.

## Originality Boundary

This is all-original LitD setting expression. The idea of a fantasy timeline,
capital city, dangerous frontier, faction strongholds, forests, courts, caves,
and dragon highlands is generic genre structure. The protected expression here
is ours: Vhael, Galdmere, the Mourning Wall, Vael Mourn, Karthspire, the
Thornwilds, Hollowgate Deeps, the Sunspire, the Verdict Halls, Drakemarch, the
Sundering, and the written history attached to them.

Do not copy protected maps, region layouts, named cities, written histories,
quest text, faction geography, or distinctive travel routes from another
setting. Use outside material only as generic design pressure, then author new
Vhael-specific expression.

## Timeline Rule

The Ages are social memory, not a calendar spreadsheet. Vhael measures history
by what the world could remember at the time: first breath, first names, the
world-wound, and the long work of mourning after the wound.

Every historical entry must answer at least one playable question: who claims
authority because of this, what relic or place survived it, what faction lies
about it, and what a player can do with that knowledge.

## The Four Ages

| Age | World State | Playable Pressure |
|---|---|---|
| The First Breath | The gods wake the Weave; the first peoples enter a world before mortal death. | Creation shrines, old oaths, and truths no kingdom can claim. |
| The Reign of Names | Mortal kingdoms rise, gods walk openly, and the first mourning customs begin. | Noble ruins, name-law, sung lineages, and contested legitimacy. |
| The Sundering | Pact-makers tear a seam to the Hollow; the dead walk; provinces are unmade; gods spend their strength sealing the wound. | Scar relics, Hollow-tainted dungeons, divine exhaustion, and permanent-death stakes. |
| The Long Mourning | Present age: no crown rules all Vhael; factions fight over relics, names, law, wilderness, and the Ashen Reach. | Player reputation, faction warfare, memorials, and living-world consequences. |

## First Breath

The First Breath is remembered as a world without graves. The gods were not yet
distant, the Weave lay close to the surface of things, and names were gifts
rather than defenses. Modern priests argue over whether mortals were sung into
being by all gods together or each people woke where a god first touched the
world.

Playable remnants should feel almost too clean: sealed springs that cannot be
poisoned, stones that remember the first oath sworn on them, and high places
where Invocation answers before the caster finishes speaking.

## Reign Of Names

The Reign of Names begins when mortals learn that a life can outlast a body if
someone keeps the name. Kingdoms, guilds, and families become record-making
machines. The first Mourning customs begin as song, then oath, then public law.
The gods still walk openly, but mortals begin choosing which divine stories to
write down and which to bury.

This age should seed noble claims, old city charters, legal relics, and Vesper
Court disputes over whose memory was edited.

## Sundering

The Sundering is the central wound. A cabal of pact-makers tries to command a
seam to the Hollow and discovers that the outside does not obey borders. The
dead rise because their names are pulled loose from the Weave. Provinces vanish
from maps and return as wrong weather, cracked roads, or ruins with no agreed
name.

The gods spend themselves sealing the tear. They save the world but do not make
it whole. Death becomes permanent, divine presence grows distant, and the Ashen
Reach remains as a grey half-real frontier.

## Long Mourning

The Long Mourning is the current playable age. No single crown rules Vhael.
Galdmere stays neutral because every faction needs a place where names can be
read aloud without immediate war. The Ashen Reach keeps leaking relics and
monsters. The factions turn memory, law, wilderness, empire, light, and Hollow
power into competing answers for how the world should survive.

Player history belongs here. A character's deeds can become tavern rumor,
faction doctrine, public memorial, legal precedent, or ash.

## Gazetteer Rule

Every root place must carry all of these:

| Requirement | Meaning |
|---|---|
| Identity | What makes the place recognizable in one sentence. |
| Pressure | Why players go there or fight over it. |
| Memory | What age or death custom marks it. |
| System Hook | Which Editor/runtime feature should preserve its identity. |

Zones are authored in the LitD editor and rendered by the proprietary Rust/wgpu
engine. The corrected implementation language is: Editor-authored zone on the LitD engine renderer, with reset and repopulation rules where the game design
requires them. Do not describe current implementation as g3n.

## Root Gazetteer

| Place | Identity | Pressure | System Hook |
|---|---|---|---|
| Galdmere | Free crossroads city built around the public Mourning Wall. | Neutral trade, guild rivalry, memorial politics, and faction diplomacy. | City zone, guild services, memorial UI, tavern roleplay. |
| The Ashen Reach | Grey half-real frontier over the Sundering scar. | Thin-Weave magic, relics, Hollow incursions, and high-risk exploration. | High-tier frontier zone, hazard rules, relic spawning, monster repopulation. |
| Vael Mourn | Drowned coastal city of bells and salt-stained archives. | Vesper Court rites, lost names, flooded vaults, and memory disputes. | Waterlogged district set, bell events, archive quests. |
| Karthspire | Black fortress-mountain and seat of the Onyx Dominion. | Imperial command, tyranny, crown plots, and war staging. | Fortress zone, siege hooks, faction throne path. |
| The Thornwilds | Old forest and moor claimed by the Thornwild. | Root law, druid secrets, anti-city raids, and living terrain. | Wilderness zone, growth hazards, faction hideout. |
| Hollowgate Deeps | Cavern shrines where the Hollow leaks closest. | Pact rites, corruption, forbidden power, and shape-loss. | Dungeon network, corruption meter, Pact progression. |
| The Sunspire | White citadel of the Dawnward. | Oaths, lightward defenses, Hollow resistance, and public trials. | Citadel zone, divine events, order defenses. |
| The Verdict Halls | Fortified courts of the Iron Verdict. | Law, warrants, judgment, prisons, and disputed executions. | Court zone, legal state, reputation penalties. |
| Drakemarch | Highland lairs of Ashen Dragons and Sundering hoards. | Dragon hunts, relic hoards, clan roads, and age-marking victories. | Endgame lair chain, boss reset rules, legendary loot. |

## Travel And Boundary Notes

Galdmere is the social hub, but it is not safe because the code makes it
harmless. It is safe because factions need its neutrality and because the
Mourning Wall makes public violence politically expensive.

The Ashen Reach is not a distant expansion island. It is the wound that names
the whole setting. Travel toward it should feel like approaching a place where
normal rules grow thin.

Every stronghold place should support both story and conquest: faction banners,
AI leaders, item-of-power risk, patrols, social status, and local rumors.

## Forbidden Drift

Do not make Vhael a complete map of unrelated themed zones. Do not make the
Ashen Reach a generic wasteland. Do not make Galdmere only a vendor hub. Do not
make the Sundering only ancient flavor with no present mechanical pressure. Do
not copy stale implementation wording from the issue body.

## Canon Links

| Issue | Boundary |
|---|---|
| #1055 | Root world bible and originality covenant. |
| #1056 | Cosmology, magic, Hollow, Weave, and Sundering metaphysics. |
| #1057 | This Ages and gazetteer detail. |
| #1058 | Pantheon and faction detail. |
| #1059 | Peoples, Mourning, legends, artifacts, and monsters. |
| #954 | Reset and repopulation gameplay pressure. |
| #987 | Ashen Reach high-tier zone pressure. |
| #1071 | Current proprietary Rust/wgpu engine doctrine. |

## FSV Requirements

For #1057 closeout, source-of-truth verification reads this physical Markdown
file, the #1055 root, and the #1056 cosmology file from disk, hashes all three,
confirms all four ages, all nine root places, implementation wording, issue
links, reset/repopulation pressure, and parent anchors are present, and confirms
stale g3n implementation wording and protected-copy instructions are rejected.
