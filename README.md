<div align="center">

![Light in the Dark](docs/readme/01-hero.png)

# Light in the Dark

### A no-code, AI-native game engine for building living worlds — and **Ashen Reach**, the multiplayer RPG being built with it.

*Direct beautiful visual stories. Fill them with characters who are genuinely alive. Build it all without code — and let an AI build it with you. A **proprietary game engine written in Rust, built on Calyx** (grounded AI), rendering Warcraft‑class worlds — all on one machine.*

</div>

---

> **Status — active development.** This README is the *vision*, captured as a living backlog of **400+ atomic tasks**. **State lives in [GitHub Issues](../../issues)** (start at the pinned **START HERE**).
>
> **Two things live here:** **① the Engine** ("Light in the Dark") — a Warcraft-III-World-Editor-class no-code creation platform; and **② the first Game** ("Ashen Reach") — an original, multiplayer, full-loot, roleplay-enforced RPG built *with* the engine to prove it can build anything. Every name, god, and tale in Ashen Reach is wholly original.

---
---

# 📦 Part I · The Engine

*First, the tool: a game creator that anyone can use, that an AI can drive, and that runs entirely on your own machine.*

## 🥇 Visual storytelling, first

![Visual storytelling](docs/readme/02-storytelling.png)

Players don't stay for stats — they stay for **emotion, character, and choices that matter.** So the whole engine is built to tell stories *visually*. You don't write code; you **direct scenes**: stage characters, set the mood and lighting, frame the camera, branch the dialogue, score the moment. Branching dialogue, cinematics, environmental storytelling, a Calyx-grounded lore bible, and an AI co-writer make telling an unforgettable story feel like play.

## 🛠️ The no-code editor — the World Editor, reborn

![The editor](docs/readme/04-editor.png)

The Warcraft III World Editor was legendary because it was **opinionated and effortless**: copy a unit, tweak its stats, snap together Event → Condition → Action blocks, and you had a game. We're rebuilding that magic. An **Object Editor** to copy-paste-customize units, heroes, abilities, and items; a visual **Trigger Editor** for no-code logic; a **Terrain Editor**; asset managers; live in-editor playtest. WC3-simple on the surface, infinitely deep underneath. *The magic is the simplicity of piecing together an entire game.*

## 🧠 An AI builds your game — the engine *is* an MCP server

![AI builds your game](docs/readme/03-mcp-agent.png)

The core architectural bet: **every capability of the editor is an agent-controllable MCP tool.** The GUI and the MCP server are two faces of one capability layer. Tell an AI agent *"build a dungeon crawl with an emotional intro and a boss that taunts the party"* — and it authors the terrain, units, abilities, triggers, dialogue, VFX, and agents end-to-end, then plays it back to verify its own work.

## 🧠 Calyx — the living-world brain

![Calyx and its lenses](docs/readme/05-calyx-lenses.png)

**Calyx** is a Rust grounded-association engine — an *intelligent database* that ingests everything (logs, bits, game-state signals), finds associations, and **decides when to wake and prompt the world's AI agents.** It runs locally on the RTX 5090 with **50 diverse embedder "lenses"** (image, audio, text, semantic, medical, legal, finance, code, DNA, molecule, protein) so it understands content across every modality. It keeps the AI *honest*: anything out-of-distribution — a wrong answer, impossible physics — **fails closed** rather than hallucinating.

## 👹 Every NPC, creature, and object is alive

![Living-world AI agents](docs/readme/06-living-agents.png)

Not scripted behavior trees — **actual AI agents**, right-sized to their role: tiny ~5 MB models animate objects and particles; small local LLMs (Gemma-class 2–4B) give NPCs in-character conversation. A guardian system (**Ward**) bounds what agents may do — creative *inside* the envelope, impossible actions *refused* at the edge.

![Binding an AI agent](docs/readme/S17-agent-binding.png)

**Bind a mind in seconds.** Pick an NPC, write a prompt, choose a model, set its personality — and it wakes up. No code. The same tools an AI uses to build the world, you use to fill it with souls.

## 🎮 What your games look like

![Gameplay](docs/readme/08-gameplay.png)

Everything is rendered by **our own proprietary game engine, written in Rust on `wgpu`** (Vulkan/DX12/Metal) — built on permissive open-source foundations, with our own WGSL shaders. *Honest visual target:* best-in-class **stylized real-time PBR** — the look of *Sunderfolk*, *Warcraft III Reforged*, or *League of Legends*: Forward+ lighting, cascaded shadows, bloom/tonemap/color-grade, dense GPU particles. Beautiful, achievable, and — critically — it **scales down** to run on a modest PC where photoreal can't.

<table>
<tr>
<td width="50%">

![VFX editor](docs/readme/09-vfx.png)
**Particles & VFX — just describe it.** Fireballs, lightning, explosions: author them with emitters and curves, or describe them in words and let Calyx generate the effect.

</td>
<td width="50%">

![AI asset pipeline](docs/readme/07-asset-pipeline.png)
**Text → playable creature.** Describe it and watch it become real: prompt → 2D concept → 3D mesh → auto-rig → animated, game-ready model.

</td>
</tr>
</table>

## 🏛️ Architecture — three tiers, strictly separated

![Three-tier architecture](docs/readme/10-architecture.png)

```
🧠 CALYX — the knowledge + backend layer (Rust, GPU, 50 lenses): the database, search,
           dedup, provenance, the AI agent runtime, and orchestration
   │  the ENGINE is BUILT ON Calyx   │  ── lowering valve (one-way, frozen) ──►
🦀 ENGINE (Rust, on Calyx):
   ⚙️ deterministic SIM  — fixed-point, zero-alloc, replayable   ← only place excluded from live Calyx
   🎨 RENDERER (wgpu)    — Forward+ PBR, shadows, post-FX, particles ← consumes a sim snapshot
🛠️ EDITOR (egui, on engine + Calyx) — the product + an MCP server
```

It's **all Rust now** — a single proprietary engine **built on Calyx** as its backend brain (we dropped g3n and Go entirely; no cgo). The sim core is bit-exact and replayable; the **lowering valve** is the only bridge to it, so the world is *alive* and *deterministic* at once. It runs **locally on one workstation** — Ryzen 9 9950X3D, RTX 5090 (32 GB), 128 GB RAM — maxed out for *creating*.

### 🖥️ Create on a beast, play on anything
The Battle.net model: you **build** with the full power of a high-end workstation, but the **games you ship run on a $500–$1,000 Windows PC** (~Ryzen 5 / RX 6600 / 16 GB) at 1080p60, scaling up to ultra on a 5090. A graphics quality scale (with open-source FSR upscaling) carries the stylized look from min-spec to max — and the engine ships that look **by default**, so a non-coder gets Warcraft-class quality with zero tuning.

### 🌐 An ecosystem, not just an engine
Beyond building games, the platform is a **Battle.net-style ecosystem**: publish and share your games, browse and download others', host servers (200–500 players), join via localhost / LAN / SSH / invite, climb ladders, and play each other's worlds. *Build it, share it, play together.*

![The living world](docs/readme/S20-living-world.png)

**The result: a world that thinks.** Calyx watches the whole realm at once and decides, moment to moment, which characters, creatures, and objects to bring to life — so the world feels alive whether two players or two hundred are online.

---
---

# 🗺️ Part II · The First World We'll Build — **Ashen Reach**

*To prove the engine can build anything, we're building the hardest thing: an original RPG with the systemic depth of the legendary 30-year permadeath MUDs. A realm called **Vhael**, where death is permanent and your **name** is the only thing that outlives you. This is its story.*

## The Cosmology

<table>
<tr>
<td width="50%">

![The Weave and the Hollow](docs/readme/L01-weave-hollow.png)
**The Weave & the Hollow.** Vhael hangs between the **Weave** — the lattice of living order — and the **Outer Dark**, a starless beyond where hungry things drift. Magic is *reaching*: pull a thread of the Weave, or open a seam to the Hollow and let something answer.

</td>
<td width="50%">

![The Sundering](docs/readme/L02-sundering.png)
**The Sundering.** Ages ago, pact-makers tore a seam to the Hollow seeking deathless power. The dead walked; provinces were unmade; the gods spent their strength to seal it — and grew distant, working now through mortal servants.

</td>
</tr>
<tr>
<td width="50%">

![The Ashen Reach](docs/readme/L03-ashen-reach.png)
**The Ashen Reach.** The scar of the Sundering: a grey, half-real wilderness where the Weave is thin and the Hollow leaks through. Magic is cheap and dangerous here — and the rarest relics in the world wash up at the edge of nothing.

</td>
<td width="50%">

![Galdmere](docs/readme/L04-galdmere.png)
**Galdmere & the Mourning.** The great free city and neutral crossroads. At its heart stands the **mourning-wall**, carved with the names of the remembered dead. You don't play to win — you play to be *remembered*.

</td>
</tr>
</table>

### 🧭 The Realm of Vhael — a gazetteer

*The world is built as connected top-down regions, each with its own dangers, inhabitants, and resets. Among them:*

- **Galdmere** — the great free city and neutral crossroads; home of the mourning-wall.
- **The Ashen Reach** — the grey, half-real scar of the Sundering; thin Weave, the rarest relics, the worst monsters.
- **Vael Mourn** — a drowned coastal city of bells; seat of the Vesper Court.
- **Karthspire** — a black fortress-mountain; throne of the Onyx Dominion.
- **The Sunspire** — the white citadel of the Dawnward.
- **The Verdict Halls** — fortified courts of the Iron Verdict.
- **The Thornwilds** — trackless ancient forest and moor; domain of the Thornwild and the druids.
- **Hollowgate Deeps** — caverns where the Hollow Pact keeps its shrines and the Outer Dark leaks closest.
- **Drakemarch** — highland lairs of the Ashen Dragons.
- **The Infernal Deeps** — the brutal hell-realm at the bottom of the world, where the most insane relics are born.

## ⚡ The Seven Gods of Vhael

![The Pantheon](docs/readme/L05-pantheon.png)

Seven gods watch over Vhael. They rarely act directly — they work through sworn mortals, granting **divine power** to those who prove their devotion through roleplay. A god may favor you, or forsake you for betraying their nature.

<table>
<tr>
<td width="20%">

![Korthac](docs/readme/L06-god-war.png)
**Korthac** — War, Victory.

</td>
<td width="20%">

![Mphirae](docs/readme/L07-god-death.png)
**Mphirae** — Death, Secrets.

</td>
<td width="20%">

![Aelinor](docs/readme/L08-god-healing.png)
**Aelinor** — Healing, Reason.

</td>
<td width="20%">

![Skarn](docs/readme/L09-god-greed.png)
**Skarn** — Greed, Wrath.

</td>
<td width="20%">

![Vessimir](docs/readme/L10-god-magic.png)
**Vessimir** — Magic, Truth.

</td>
</tr>
</table>

*…with **Whisanne** (Passion, Courage, Wisdom) and **Orthuun** (Order, Judgment) completing the seven.*

## ⚔️ The Seven Factions

*Player-run orders, each sworn to a cause. Join one, rise through its ranks by roleplay, and wage war for its survival. Membership is lifelong; betrayal is costly.*

<table>
<tr>
<td width="33%">

![The Ironbound](docs/readme/L11-faction-ironbound.png)
**The Ironbound** *(Any)* — warriors who **despise magic**. Lead them as the **Warmaster**.

</td>
<td width="33%">

![The Dawnward](docs/readme/L12-faction-dawnward.png)
**The Dawnward** *(Good)* — knights of the light against the Hollow. Lead as the **High Lantern**.

</td>
<td width="33%">

![The Vesper Court](docs/readme/L13-faction-vesper.png)
**The Vesper Court** *(Any)* — keepers of song and memory. They decide who is *remembered*.

</td>
</tr>
<tr>
<td width="33%">

![The Iron Verdict](docs/readme/L14-faction-verdict.png)
**The Iron Verdict** *(Orderly)* — judges who impose law on a lawless realm.

</td>
<td width="33%">

![The Onyx Dominion](docs/readme/L15-faction-onyx.png)
**The Onyx Dominion** *(Evil/Orderly)* — an empire to rule all Vhael. Climb the ranks and become **Sovereign** — emperor.

</td>
<td width="33%">

![The Thornwild](docs/readme/L16-faction-thornwild.png)
**The Thornwild** *(Neutral)* — wild folk who would see civilization swallowed by root and storm.

</td>
</tr>
</table>

![The Hollow Pact](docs/readme/L17-faction-hollow.png)
**The Hollow Pact** *(Evil)* — scholars and cultists who bargain with the beings of the Outer Dark for power no one should hold. Lead as the **Chancellor of the Veil**.

### 🏴 Item-of-power warfare

![Faction warfare](docs/readme/L18-raid-warfare.png)

Every faction's power flows from a single **item-of-power**, guarded by Inner and Outer guardians in its hideout. **Steal a rival's item** and carry it to your shrine, and *their members lose their powers* until they take it back. This drives perpetual, high-stakes **raiding warfare**, shifting alliances, and faction politics — the endgame of Vhael.

### 🐉 The horrors of the Reach

<table>
<tr>
<td width="50%">

![The Ashen Dragons](docs/readme/L19-ashen-dragon.png)
**The Ashen Dragons** — eldest and most feared of monsters, each hoarding **legendary relics** torn from the Sundering. To slay one is to be sung of for an age.

</td>
<td width="50%">

![The Outer Dark](docs/readme/L20-outer-dark.png)
**The Hollow's Children** — silent veil-walkers and hungering grey things that seep through thin places in reality. Near the Reach, the dark gets very close.

</td>
</tr>
</table>

## 💀 The Mightiest Beings of Vhael

*Every great RPG is defined by the terrors it pits you against. These are the legends of the realm — the foes that take a party, a plan, and a great deal of luck.*

<table>
<tr>
<td width="50%">

![Arch-devil](docs/readme/B01-archdevil.png)
**The Arch-Devils** — horned lords of the Infernal Deeps, wreathed in hellfire.

</td>
<td width="50%">

![Lich-Lord](docs/readme/B02-lichlord.png)
**The Lich-Lords** — undead sorcerer-kings who cheated death itself.

</td>
</tr>
<tr>
<td width="50%">

![Dracolich](docs/readme/B03-dracolich.png)
**The Dracoliches** — undead dragons, necrotic fire pouring from bare ribs.

</td>
<td width="50%">

![Titan](docs/readme/B04-titan.png)
**The Titans** — primordial giants the size of mountains.

</td>
</tr>
<tr>
<td width="50%">

![Leviathan](docs/readme/B05-leviathan.png)
**The Leviathans** — colossal horrors of the drowned coasts.

</td>
<td width="50%">

![Eldritch horror](docs/readme/B06-eldritch.png)
**The Greater Hollow-Things** — eldritch beings that warp reality itself.

</td>
</tr>
<tr>
<td width="50%">

![Behemoth](docs/readme/B07-behemoth.png)
**The Infernal Behemoths** — four-armed hell-beasts of muscle and chain.

</td>
<td width="50%">

![Giant warlord](docs/readme/B08-giantking.png)
**The Giant Warlords** — storm-giant kings who command the peaks.

</td>
</tr>
<tr>
<td width="50%">

![Wraith-king](docs/readme/B09-wraithking.png)
**The Wraith-Kings** — death-knight lords leading hosts of shades.

</td>
<td width="50%">

![Mummy-lord](docs/readme/B10-mummylord.png)
**The Mummy-Lords** — ancient withered kings risen from sunken tombs.

</td>
</tr>
</table>

---
---

# ⚔️ Part III · How You Play

## 🎮 What it looks like in play

*These are the **gameplay benchmark** — actual top-down/isometric real-time RPG play: full HUD, ability hotbars, floating damage, minimaps, party frames. This is the quality bar every game built in the editor should reach, and it runs on a $500–$1,000 PC.*

![Combat](docs/readme/GP03-combat.png)
*Real-time party combat — a boss health bar, floating crits, a named ability hotbar, chat, and minimap. Deep, readable, fast.*

<table>
<tr>
<td width="50%">

![Town](docs/readme/GP01-town.png)
**The world.** Bustling towns, merchants, questgivers — a living hub.

</td>
<td width="50%">

![Exploration](docs/readme/GP02-exploration.png)
**Exploration.** Venture the grey wilds with your party toward a glowing objective.

</td>
</tr>
<tr>
<td width="50%">

![Dungeon](docs/readme/GP05-dungeon.png)
**Dungeons.** Torch-lit delves full of undead, traps, and treasure.

</td>
<td width="50%">

![Party](docs/readme/GP08-party.png)
**Party play.** Tank, healer, mage, archer — coordinate or die.

</td>
</tr>
</table>

### Deep classes — 20–30 skills each, 60–100 spells for casters

*Every class is a deep toolkit. Martials master ~20–30 skills and specializations; spellcasters and priests command **60–100 spells** across schools and paths. Build your character; no two are alike.*

<table>
<tr>
<td width="50%">

![Spellbook](docs/readme/GP19-spellbook.png)
**The spellbook.** Dozens of spells across Fire, Frost, Lightning, Shadow, Arcane, Nature — each with rank, cooldown, and cost.

</td>
<td width="50%">

![Talents](docs/readme/GP20-talents.png)
**Skills & talents.** A packed hotbar and a branching talent tree — deep build customization.

</td>
</tr>
<tr>
<td width="50%">

![Mage](docs/readme/GP04-mage-spell.png)
**Spellcasting.** Ground-targeted AoE, vivid particles, real consequences.

</td>
<td width="50%">

![Necromancer](docs/readme/GP15-necromancer.png)
**Summoners.** Raise an undead host and overwhelm your foes.

</td>
</tr>
</table>

### The stakes — PvP, full loot, faction war

<table>
<tr>
<td width="50%">

![PvP](docs/readme/GP06-pvp.png)
**Full-loot PvP.** Kill anyone, loot everything. No safe ground.

</td>
<td width="50%">

![Full loot](docs/readme/GP13-full-loot.png)
**The spoils.** A fallen player drops it all — grab what you can.

</td>
</tr>
<tr>
<td width="50%">

![Soul-knight](docs/readme/GP16-soulknight.png)
**The Unhallowed Knight.** A soul-bound blade that grows with every kill.

</td>
<td width="50%">

![Faction war](docs/readme/GP18-faction-war.png)
**Faction war.** Dozens of players clash over an item-of-power.

</td>
</tr>
<tr>
<td width="50%">

![Boss](docs/readme/GP07-boss.png)
**Boss raids.** Bring a party and a plan for the Ashen Dragons.

</td>
<td width="50%">

![Infernal Deeps](docs/readme/GP14-infernal.png)
**The Infernal Deeps.** Hell's end-game, where the best loot — and worst deaths — live.

</td>
</tr>
</table>

### Systems & roleplay

<table>
<tr>
<td width="33%">

![Inventory](docs/readme/GP09-inventory.png)
**Gear & stats.** Equip, compare, build your character.

</td>
<td width="33%">

![Dialogue](docs/readme/GP10-dialogue.png)
**Dialogue.** Branching, choice-driven conversations.

</td>
<td width="33%">

![Quests](docs/readme/GP11-quest.png)
**Quests.** Trackers, markers, and epic questlines.

</td>
</tr>
<tr>
<td width="33%">

![Tavern](docs/readme/GP17-tavern.png)
**Roleplay.** Gather, scheme, and tell stories in-character.

</td>
<td width="33%">

![Hideout](docs/readme/GP12-faction-hideout.png)
**Faction halls.** Your order's hideout and item-of-power.

</td>
<td width="33%">

*…and it all runs on a modest PC, built no-code in the Editor.*

</td>
</tr>
</table>

---

## Become someone

<table>
<tr>
<td width="50%">

![Character creation](docs/readme/12-character-creation.png)
**Create your character.** Pick from generic fantasy races (human, elf, dwarf, orc, gnome…) and class archetypes (warrior, mage, thief, cleric, ranger, necromancer…), roll your six attributes, and choose your alignment and ethos.

</td>
<td width="50%">

![Write your role](docs/readme/13-role-creation.png)
**Write your role.** Who are you? What drives you? Compose your backstory and ambition. **Roleplay is required and enforced** — and how well you live your role is how you *grow*.

</td>
</tr>
<tr>
<td width="50%">

![Classes](docs/readme/14-class-showcase.png)
**17 classes, deep specialization.** Warriors master weapons and signature techniques; mages bend the elements, the body, or summon the dead; priests earn divine power through devotion; thieves brew poisons and set traps.

</td>
<td width="50%">

![Talent tree](docs/readme/S10-talent-tree.png)
**Branching talent trees.** Spend points to unlock active powers, passives, and upgrades along the paths that define your build. No two characters need ever be alike.

</td>
</tr>
<tr>
<td width="50%">

![Races](docs/readme/15-race-showcase.png)
**A realm of peoples.** Generic kindreds plus original ones — the wind-born **Sylphkin**, the grey Hollow-touched **Ashkin** — each with its own gifts and place in the world.

</td>
<td width="50%">

![A tavern](docs/readme/S14-tavern-rp.png)
**Live among the living.** The realm is a real society — full of the whole range of people, the brilliant and the foolish, the brave and the craven. Gather in taverns, scheme, ally, and tell your story in-character.

</td>
</tr>
</table>

### The 21 Kindreds (Races)

*Generic fantasy peoples plus original kindreds of Vhael. Lifespan (and so death-by-aging) varies by kindred — elves and giants outlast humans by generations. Each has its own attribute caps, resistances, innate gifts, and allowed classes.*

| Kindred | Nature | Lifespan |
|---|---|---|
| **Human** | versatile, ambitious, found everywhere | short |
| **Elf** | keen-minded and quick, but frail | very long |
| **Half-Elf** | balanced and adaptable | long |
| **Wood-Elf** | sturdier, nature-bound elves | very long |
| **Dark-Elf** | nimble, intelligent, drawn to shadow | very long |
| **Ashkin** *(original)* | grey, Hollow-touched elves — distrusted, gifted in thin places | very long |
| **Dwarf** | hardy, poison/magic-resistant, proud | long |
| **Deep Dwarf** | evil, agile kin of the dwarves | long |
| **Gnome** | small, tough, wisest of folk | long |
| **Deep Gnome** | strong, very wise cavern gnomes | long |
| **Orc** | savage, strong, destructive | short |
| **Goblin** | cunning, weak, sly | short |
| **Minotaur** | rare, mighty, fierce | medium |
| **Centaur** | swift, proud, untamed | medium |
| **Storm Giant** | towering, lightning-blooded | very long |
| **Fire Giant** | burning, brutal | very long |
| **Frost Giant** | immense, cold-hearted | very long |
| **Cloud Giant** | vast and aloof | very long |
| **Sylphkin** *(original)* | winged bird-folk of the high crags | medium |
| **Felisar** *(original)* | agile cat-kin of the southern wastes | medium |
| **Saur** *(original)* | cold-blooded scale-folk of the drowned coasts | medium |

### The 17 Classes

*Generic archetypes, deeply specialized. Attributes use Vhael's six pillars — **Might, Finesse, Intellect, Spirit, Endurance, Presence**. Priests (✝) must earn divine **empowerment** through roleplay to unlock their full power.*

| Class | Role | Prime | Alignment |
|---|---|---|---|
| **Warrior** | peerless weapons master; weapon specializations + signature legacies | Might | Any |
| **Berserker** | frenzied destroyer (the savage kindreds' calling) | Might | Evil |
| **Raider** | savage skirmisher — warrior/thief hybrid | Might/Finesse | Neutral/Evil |
| **Ranger** | wilderness warrior, archery & beast-companions | Might | Any |
| **Thief** | stealthy rogue — poisons, traps, theft, paths of specialization | Finesse | Any |
| **Assassin** | poisoner & martial artist | Finesse | Any |
| **Bard** | musician & loremaster; songs of power | Presence | Any |
| **Paladin** ✝ | holy knight, blade and blessing | Might | Good |
| **Unhallowed Knight** ✝ | unholy knight; soul-bound weapon grows with every kill | Might | Evil |
| **Shaman** ✝ | offensive war-priest; deity-granted paths | Spirit | Good/Evil |
| **Healer** ✝ | defensive priest, unmatched at mending | Spirit | Any |
| **Druid** ✝ | nature priest; beast-forms, wilderness power | Spirit | Neutral |
| **Invoker** | elementalist mage — seven elemental paths | Intellect | Any |
| **Transmuter** | alteration mage — twist body and matter | Intellect | Any |
| **Conjurer** | summoner mage — bind extraplanar allies | Presence | Any |
| **Necromancer** | death mage — undead, life-drain; may undergo the Withering | Intellect | Evil |
| **Shapeshifter** | form-changing mage — animal & monstrous forms | Intellect | Any |

## Fight, win, lose everything

<table>
<tr>
<td width="50%">

![PvP duel](docs/readme/16-pvp-duel.png)
**PvP, full loot.** Multiplayer from day one. You can kill anyone — and killing a player lets you **loot everything they carry.**

</td>
<td width="50%">

![Full loot](docs/readme/S11-full-loot.png)
**The fallen leave everything behind.** Common gear is unlimited, so you can re-equip and get back into the fight — but the rarest relics change hands only by force.

</td>
</tr>
<tr>
<td width="50%">

![Item rarity](docs/readme/S12-rarity.png)
**Scarcity is real.** The most powerful items are **finite in the entire world** — maybe one or two legendaries exist, ever. While you hold one, the monster that drops it can't respawn with it. Hoard it and stop playing, and it **decays back into the world.** Power must be *used*.

</td>
<td width="50%">

![Boss fight](docs/readme/17-boss-dragon.png)
**Face the eldest things.** Party up to challenge the dragons and horrors of the Reach for the realm's greatest relics — powers that, stacked, make you nigh-unkillable, and the most hunted soul in Vhael.

</td>
</tr>
</table>

## The stakes: mortality

![Constitution and permadeath](docs/readme/S01-constitution.png)

**Death is permanent, and it erodes you.** Your **constitution** is your hold on life. Every few deaths costs a permanent point of it — and when it runs out, your character is **gone forever**. There is no respawn, no second chance.

![Aging](docs/readme/S02-aging.png)

**And even the careful die.** Every character ages and, in time, dies of old age — and lifespan depends on your race: elves and the long-lived outlast humans by generations. A life hoarded is a life wasted. This is why the realm's only greeting is: *by what name do you wish to be mourned?*

## 🪦 The Hall of the Remembered

*In a world where every character truly dies, death must **mean** something. So when a character falls for the last time, they are not deleted — they are **enshrined**.*

![The mourning](docs/readme/S15-mourning.png)

In Galdmere, the **mourning-wall** carries the names of the fallen, carved in stone for as long as the world stands. When a character dies for the last time, the realm stops to remember them — and a new name is added to the wall.

![The graveyard](docs/readme/G01-graveyard.png)

**① A graveyard for the fallen.** Every permanently-dead character earns a place in the **Hall of the Remembered** — a vast, browsable memorial of everyone who ever lived and died in the world. Their name, their lineage, their faction, the span of their life: all kept, forever, for anyone to visit.

![A hero's memorial](docs/readme/G02-memorial.png)

**② A legend, enshrined.** Each memorial is a highlight reel of a life: the bosses slain, the relics held, the raids won, the ranks reached, the titles earned — and the **eulogies and tributes written by other players.** It lets the whole world see *exactly how badass that character was.* Your character dies, but your **legend endures.**

![A funeral](docs/readme/G03-funeral.png)

**③ Why we mourn.** Remembering the dead is one of the deepest human needs — it is *why we hold funerals.* The same reasons apply here, exactly: mourning gives **meaning to loss**, honors a **legacy**, brings **closure**, and binds a **community** in shared memory. This is the emotional heart of the whole game. Permadeath without remembrance is just punishment; permadeath *with* a memorial is what makes a life — and its risks, and its end — truly matter.

## The darkest paths

<table>
<tr>
<td width="50%">

![The Withering](docs/readme/S03-withering.png)
**Cheat death — become undead.** A master of dark magic can undertake a secret, forbidden ritual — **the Withering** — to become a **lich, mummy, or wraith.** The undead no longer age and grow vastly powerful… but their hold on life resets dangerously low, and they can only be ended by being **hunted down and destroyed.**

</td>
<td width="50%">

![The undeath transformation](docs/readme/B02-lichlord.png)
**Lords of the undead.** Those who complete the rite gain unique rituals — siphoning the life of the dead, stealing flesh and knowledge, walking through walls. A lich who is never killed is, in a sense, immortal: a permanent power in the world, until a strong enough party comes for them.

</td>
</tr>
<tr>
<td width="50%">

![The unhallowed knight](docs/readme/S04-unhallowed-knight.png)
**The Unhallowed Knight.** An evil knight whose signature is a **soul-bound weapon** — and every life it claims makes it permanently stronger.

</td>
<td width="50%">

![The soul weapon](docs/readme/S05-soulweapon.png)
**A blade that feeds.** The more they kill, the deadlier their weapon becomes — a snowballing terror whose power can be transferred between weapons, and whose destruction wounds its master. The longer one lives, the more fearsome it grows.

</td>
</tr>
</table>

---

## 🔥 The Ultra End-Game · The Infernal Deeps

![The Infernal Deeps](docs/readme/S06-infernal-deeps.png)

*At the bottom of the world, below the scar of the Sundering, lies **Hell** — the Infernal Deeps. A realm of devils and fire ruled by arch-tyrants, where the Weave is gone entirely and only ferocity and ambition survive. It is the single most lethal place in Vhael — and the only source of the world's mightiest relics. This is where legends are forged, and where most of them die.*

> The Deeps are **ordered** hell — hierarchy, legions, and bargains — distinct from the formless eldritch horror of the Outer Dark. Here, everything wants something from you, and everything can be negotiated with… for a price.

### Getting in — you do not simply walk into Hell

There is **no recall into the Deeps.** You earn the descent, by one of several perilous paths:

<table>
<tr>
<td width="50%">

![The Gate of Cinders](docs/readme/H01-gate-of-cinders.png)
**The Gate of Cinders.** A colossal sealed hellgate at the bottom of the Sunken Vaults. It opens only to one who bears a **Cindermark** — an infernal sigil earned through a deadly questline, or branded onto you by a devil who will want it repaid.

</td>
<td width="50%">

![The Descent](docs/readme/H02-descent.png)
**The Long Descent.** First you must *reach* the Gate — a grueling delve through the deepest dungeon in the world. Others bargain a **pact** with a lesser devil for passage; the **Hollow Pact** can tear a temporary seam; and the unlucky are simply *dragged under* when they die in a thin place.
**Once inside, escape magic fails. You find a way out — or you die there.**

</td>
</tr>
</table>

### The Seven Circles

*Hell descends in seven circles, each deadlier than the last, each with its own hazards, its own legions, and its own ruling lord.*

<table>
<tr>
<td width="50%">

![The Ember Wastes](docs/readme/H03-ember-wastes.png)
**I–II · The Threshold & the Ember Wastes.** Past the entry waste of the damned lie burning plains of lava and ash where infernal behemoths roam. The very air *burns* — heat drains your life the longer you linger.

</td>
<td width="50%">

![The Iron Bastion](docs/readme/H04-iron-bastion.png)
**III · The Iron Bastion.** A vast fortress-city of disciplined devil-legions under the **Iron Praetor** — ordered, militaristic, and patrolled. Brute force fails here; you must outwit an army.

</td>
</tr>
<tr>
<td width="50%">

![The Hungering Dark](docs/readme/H05-hungering-dark.png)
**IV · The Hungering Dark.** Pitch-black caverns where eyeless soul-eaters crawl from the walls, ruled by **the Maw**. Your torch is a fragile thing, and the dark presses in. Sanity and supplies both run short.

</td>
<td width="50%">

![The Wailing Tombs](docs/readme/H06-bone-choir.png)
**V · The Wailing Tombs.** A vast ossuary where the souls of the damned are tormented under **the Choirmaster** — wraiths, undead, and dread. Necromancers covet it: the rarest reagents for the path to undeath are found nowhere else.

</td>
</tr>
</table>

<table>
<tr>
<td width="50%">

![The Obsidian Throne](docs/readme/H07-obsidian-throne.png)
**VI–VII · The Molten Throne-road & the Obsidian Throne.** The deadliest road in the game leads to the seat of **Maldrithax, the Ashen Tyrant** — the arch-devil who rules the Deeps, and the apex boss of the world. Few parties ever stand before him; fewer leave.

</td>
<td width="50%">

![The relic vault](docs/readme/H08-relic-vault.png)
**The hell-forged vaults.** Guarded by chained demon-wardens, the relic vaults hold the world's mightiest, world-finite **legendary artifacts** — the crowns, soul-blades, and cursed relics that make a character nigh-unkillable. There is no other source of such power anywhere in Vhael.

</td>
</tr>
</table>

### Why it is brutal — and why it matters

![Legendary loot](docs/readme/S07-legendary-loot.png)

The Deeps are the purest expression of the realm's creed: **the greatest power demands the greatest risk.** There is no recall and no safety; demon patrols never stop; the environment itself — heat, dark, soul-drain — wears your **life and constitution** down with every step. Die deep in Hell and your corpse, and *everything you carry*, is all but unrecoverable — a full-loot prize for demons or rival players. A single Infernal run can cost a character their life **permanently.**

But those who conquer it return with the artifacts that reshape the balance of the entire world — and a name that will be carved on the mourning-wall and sung of for an age. **To descend into the Deeps and walk back out is the pinnacle of any story in Ashen Reach.**

## Rise through the ranks — by *roleplay*

<table>
<tr>
<td width="50%">

![Cabal ranks](docs/readme/S08-ranks.png)
**Earn your power.** Within a faction, you advance through ranks — and each rank unlocks new cabal powers. But advancement isn't grinding: it's earned by how well you **embody your faction's cause**, judged by its leaders.

</td>
<td width="50%">

![Immortal reward](docs/readme/S09-immortal-reward.png)
**The gods are watching.** Calyx measures how well you live your role, and the Immortals reward it — with boons, sacred tattoos, titles, and powers. **Good roleplay is rewarded; bad roleplay is punished.** So is skill in battle.

</td>
</tr>
</table>

![Hybrid Immortals](docs/readme/S19-hybrid-immortal.png)

**The Immortals are AI *and* human — seamlessly.** A god like Korthac might be driven by a Calyx AI agent one hour and a human admin the next, granting rulings and rewards as one consistent deity. The world's gods are always present, always watching — and players can rise to *become* them.

## Build it — and host it

<table>
<tr>
<td width="50%">

![Quest designer](docs/readme/S16-quest-designer.png)
**Design epic questlines, no code.** A visual node-graph of branching objectives, conditions, and rewards — author a sprawling campaign as easily as you sketch one.

</td>
<td width="50%">

![Lore authoring](docs/readme/S18-lore-authoring.png)
**Weave a whole mythology.** The lore system + AI co-writer let you author an original, interconnected world — gods, factions, histories — kept consistent by Calyx. *Lore is the story.*

</td>
</tr>
</table>

![Multiplayer hosting](docs/readme/S13-multiplayer-host.png)

**Host a world for everyone.** Anyone can run a server on their own machine — up to ~500 players — and friends connect over **localhost, LAN, an SSH tunnel, or a direct invite.** Admins control who plays and how strictly roleplay is watched and rewarded. **The benchmark for success: the entire world runs on one personal computer** — which means anyone can host it for everyone else.

---
---

# 🎯 Part IV · The Road Ahead

![What will you build](docs/readme/11-future.png)

**The end goal:** build *Ashen Reach* — an original RPG with the depth of a 30-year MUD, rendered beautifully, with AI running every god, faction, and NPC. When the engine can build *that*, it can build anything — and every other kind of game becomes a small feature add-on.

This is the most ambitious no-code game creator ever attempted: **the depth of a classic MUD, the accessibility of the Warcraft III editor, the beauty of a modern real-time engine, and the soul of an AI that brings every world to life.**

**What will you build? And by what name will you be remembered?**

---
---

# 📚 Appendix · The Complete Build Map

*Every system, indexed to its tracked issues — so any agent or contributor can find exactly where each capability lives. This is the authoritative map; the issues are the source of truth.*

### 📜 Binding doctrine — read first
- **#884** Visual storytelling is the #1 goal · **#885** End-user experience (WC3-simple)
- **#915** The engine IS an MCP server (every capability is an agent tool) · **#995** Core game design — multiplayer, PvP, full-loot, roleplay-enforced
- **#777** Calyx usage · **#778** ≥10-diverse-embedder mandate · **#800** Engineering & FSV-against-reality standards
- **#959** END GOAL — build *Ashen Reach* to classic-MUD depth · **#1055** Original world bible (canonical) · **#928** Performance target (the workstation, maxed; no min-spec)

### 📦 The Engine — pillar epics
- **#730** WC3-style visual editor → WC3-parity sub-epics **#801–805**, atomic children **#806–858**
- **#717** AI-assisted authoring + the lowering valve + Ward content gate
- **#886** Visual storytelling & narrative → directorial layer **#909–914**, children **#887–897**
- **#898** End-user UX & onboarding → **#899–906**
- **#916** MCP control surface → tool-groups **#917–927**
- **#1072** Custom proprietary Rust engine (BUILD PRIORITY #1) → subsystem epics: foundation #1074 · core/ECS #1080 · renderer(wgpu) #1085 · assets #1098 · animation #1104 · physics #1109 · **Calyx backend #1113** · audio #1125 · VFX #1128 · deterministic sim #1132 · editor(egui) #1137 · MCP #1143 · quality/acceptance #1146 (engine doctrine #1071; dep policy #1073). *(g3n #861/#873 dropped/superseded.)*
- **#744** Particle & VFX editor → **#745–753**
- **#781** In-game AI agents (every NPC/object a Calyx agent) → **#783–788, #859–860, #965–971**
- **#782** AI asset-generation pipeline → **#789–796**
- **#770** Local-first platform (Calyx + 50 embedders on Windows + RTX 5090) → **#771–776, #694**
- Calyx capabilities: **#797** Lodestar kernel · **#798** Oracle prediction · **#799** alerting · **#907** replay analytics · **#908** asset semantic index

### 🗺️ The Game — *Ashen Reach*
- **#933** Ultimate RPG creator → **#934–944** · **#945** MUD-grade systemic depth → **#946–958, #960–964**
- **#972** Scarcity economy & full-loot PvP → **#973–981**
- Roleplay engine: **#982** RP-quality measurement · **#983** rank/power progression · **#996** bad-RP punishment · **#997** player social graph · **#998** faction ambition & leadership
- **#1037** Self-hosted servers & dead-simple multiplayer → **#1038–1044**
- Immortals (AI + human, hybrid): **#965–967, #1045–1047**
- Mortality & the dark paths: **#1060** constitution/permadeath/aging · **#1061** undeath (the Withering) · **#1062** the Unhallowed Knight soul-weapon · **#1063** the Infernal Deeps (Hell) · **#1064** hardest raid + secret undeath path · **#1065** the Hall of the Remembered (memorial)
- Content build: **#984** full world build → **#985–994**; **21 races #999–1019**; **17 classes #1020–1036**
- Lore: **#1048** lore system → **#1049–1054**; world bible **#1055**; lore compendium **#1056–1059**

### 🧱 Content completeness — nothing planned is left out
Every content category has both an illustrated overview here **and** an exhaustive, atomic spec in the issues:
- **21 Kindreds** (races) — overview table above; full per-race specs (stat caps, resistances, innate gifts, allowed classes, lifespan, model) at **#999–1019**.
- **17 Classes** — overview table above; full per-class specs (leveled ability lists, specializations/paths/forms, restrictions) at **#1020–1036**.
- **7 Factions** + **7 Gods** — lore in Part II; mechanics & per-entity build at **#945/#948/#949/#990**, world bible **#1055**, lore compendium **#1056–1059**.
- **Bestiary & bosses** — the 10 mightiest beings gallery + the Ashen Dragons & Outer Dark above; all mobs/creatures/bosses = data + engine-native models + Calyx agents at **#991**.
- **Abilities/spells/skills/songs/powers** — one universal ability framework **#961**; per-ability content + VFX at **#988**.
- **Items** — scarcity-tiered itemization **#972–981**; full item set + models at **#989**.
- **World/regions, quests, lore text** — gazetteer above; areas **#987**, quests **#931/#992**, original lore authored via **#1048–1059**.
- **Mortality, undeath, the Infernal Deeps, the Hall of the Remembered** — **#1060–1065**.

The README is the illustrated map; the issues are the exhaustive, build-ready specification. Together they leave no blindspot.

> **For agents:** start at the pinned **START HERE** issue (#780), then the doctrine above. Every task carries a Full-State-Verification-in-reality close gate (#800). State lives in the issues — keep them current.

---

<div align="center">

*Light in the Dark · the world of Ashen Reach*

**[📋 Browse the 470+ issue backlog →](../../issues)** · A proprietary Rust engine built on Calyx · Windows · RTX 5090

</div>
