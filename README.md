<div align="center">

![Light in the Dark](docs/readme/01-hero.png)

# Light in the Dark

### The ultimate **RPG creation engine** — where you build living worlds by telling visual stories, and every character is alive.

*A multiplayer, PvP, full-loot, roleplay-enforced living world — and the no-code editor to build it. A Warcraft III World Editor for the age of grounded AI, powered by [Calyx](#-calyx--the-living-world-brain) and rendered by [g3n](#-what-your-games-look-like).*

</div>

---

> **Status:** Active development. This README tells the story of **what is coming** — the vision we are building toward, captured as a living backlog of atomic tasks. **State lives in [GitHub Issues](../../issues)** (start at the pinned **START HERE** issue). Nothing here is marketing fluff; every capability below is an open, tracked issue.

---

## ✨ What is this?

**Light in the Dark** turns a deterministic Go game engine into a **WC3-World-Editor-class, no-code creation platform** for building deep, beautiful **RPGs** — the kind of worlds that birthed DotA, Tower Defense, and thousands of legendary custom games, reborn for the age of AI.

Three things make it different:

1. **🥇 Visual storytelling is the heart of it.** You don't write code — you *direct scenes*. Stage characters, set mood and lighting, frame the camera, branch the dialogue, and let the world tell an unforgettable story.
2. **🧠 Every NPC, creature, and object is a living AI agent.** Not scripted behavior trees — actual AI, orchestrated by **Calyx**, a grounded-association intelligence that watches everything happening and decides when to bring the world to life.
3. **🛠️ An AI can build the entire game for you.** The whole engine is an **MCP server** — every capability is a tool an AI agent can call. Describe what you want; watch it get built.

Our **north-star end goal**: faithfully recreate the systemic depth of **[Carrion Fields](https://carrionfields.net)** — a 30-year-old MUD that is the benchmark for RPG depth — *visually*, with AI running every Immortal, cabal, and NPC. **If the engine can build Carrion Fields, it can build any RPG you can imagine.**

---

## 🥇 Visual storytelling, first

![Visual storytelling](docs/readme/02-storytelling.png)

Players don't stay for stats — they stay for **emotion, character, and meaningful choices.** So the entire engine is optimized for telling stories *visually and textually*, every way a story can be told:

- **Branching dialogue** editor (Ink/Yarn-class) — write a screenplay, get a living conversation.
- A **directorial layer**: scene staging, **mood & atmosphere** (pick a *feeling* — dread, warmth, awe — not a shader), **camera as director**, character **expression & emotion**, and **adaptive emotional scoring**.
- **Environmental storytelling**, **cinematics/cutscenes**, **quests & epic questlines**, a **Calyx-grounded lore bible** that keeps your world consistent, and an **AI co-writer** that drafts scenes you edit.
- A **visual-novel mode** for image-and-text stories with zero 3D work.

> *Show, don't tell.* Text is first-class — but the engine leads with the image.

---

## 🧠 An AI builds your game — the engine *is* an MCP server

![AI builds your game](docs/readme/03-mcp-agent.png)

This is the core architectural bet: **every single capability of the editor is an agent-controllable MCP tool.** The GUI and the MCP server are two faces of one capability layer — if a human can do it, an AI agent can do it, and vice versa.

Tell an agent *"build a 4-player dungeon crawl with an emotional intro cutscene and a boss that taunts the party"* — and it authors the terrain, units, abilities, triggers, dialogue, VFX, and agents end-to-end, then plays it back to verify its own work. No clicking required.

---

## 🛠️ The no-code editor — Warcraft III's World Editor, reborn

![The editor](docs/readme/04-editor.png)

The WC3 World Editor was legendary because it was **opinionated and effortless** — copy a unit, tweak its stats, and you had a new character; snap together Event → Condition → Action blocks, and you had a game. We're rebuilding that magic with modern tech and AI underneath:

- **Object Editor** — copy-paste-customize units, heroes, abilities, items, buffs, upgrades, tech trees.
- **Trigger Editor** — visual Event/Condition/Action logic, no code (Lua escape hatch when you want it).
- **Terrain Editor** — paint tiles, cliffs, water, doodads, regions, cameras, weather.
- **Asset & Object managers**, live in-editor playtest, undo/redo, autosave.

The whole surface stays **WC3-simple** even as the power underneath grows — because *the magic is the simplicity of piecing together an entire game.*

---

## 🧠 Calyx — the living-world brain

![Calyx and its lenses](docs/readme/05-calyx-lenses.png)

**Calyx** is a Rust grounded-association semantic engine — an *intelligent database* that ingests everything (logs, bits, game-state signals), finds associations with grounded intelligence, and **decides when to activate and prompt AI agents** to make the world feel alive.

It runs locally on the RTX 5090 with a curated roster of **50 diverse embedder "lenses"** — image, audio, text, semantic, medical, legal, finance, code, DNA, molecule, protein — so it understands content across every modality. Calyx grounds the AI: agents can only do what's *in distribution*. Out-of-distribution answers, physics, or behavior **fail closed** — never hallucinated.

> Calyx never touches the deterministic game loop. Its intelligence is **"lowered"** — one-way — into frozen, fingerprinted game data. The game stays bit-exact and replayable; the AI stays out of the frame.

---

## 👹 Every NPC is alive

![Living-world AI agents](docs/readme/06-living-agents.png)

Every creature you fight, every NPC you talk to, every object in the world can be a **Calyx-driven AI agent** — right-sized to its role:

- **Object/visual micro-agents** (~5–20 MB tiny models) bring props and particles to life.
- **Conversational NPCs** run small local LLMs (Gemma-class 2–4B) for in-character dialogue, grounded by Calyx.
- **AI agents run the world's factions** — they play the Immortals, lead the cabals and religions, and even occupy player-character slots, fighting and roleplaying alongside humans.

A guardian envelope (**Ward**) bounds what agents can do — creative *inside* the envelope, impossible physics *refused* at the edge. Bind a prompt, pick a model, attach it to an object. Set them loose.

---

## 🎮 What your games look like

![Gameplay](docs/readme/08-gameplay.png)

Everything is rendered by **g3n** — and **g3n generates everything**: every object, model, material, and particle is g3n-native, pushed to its high-tech ceiling on the RTX 5090. PBR materials, dynamic shadows, custom GLSL shaders, dense GPU particle VFX, bloom and color grading.

> **Honest visual target:** best-in-class **stylized real-time PBR** — the look of *Sunderfolk*, *Warcraft III Reforged*, or *League of Legends*, running smoothly at high framerate. Beautiful and achievable, not pre-rendered film CGI.

### 🔥 Particles & VFX — just describe it

![VFX editor](docs/readme/09-vfx.png)

Fireballs, explosions, lightning strikes, magic vortexes — author them visually with emitters and curves, or **describe them in words** and let Calyx generate the effect. All g3n-native, all real-time.

### 🎨 AI asset pipeline — text → playable creature

![AI asset pipeline](docs/readme/07-asset-pipeline.png)

Describe a creature and watch it become real: **prompt → 2D concept → 3D mesh → auto-rig → animated, game-ready model**, conformed to the render budget and ingested into Calyx for semantic search and dedup.

---

## ⚔️ The ultimate RPG creator

RPG is the **flagship** — because RPGs are *stories*, and stories are what we're built for. Everything the great MUDs and ORPGs had, made buildable with no code:

| System | What you get |
|---|---|
| **Characters** | Races (traits, resistances, restrictions), classes & multi-class, alignment × ethos, character creation |
| **Growth** | Skill/talent trees, leveling, attributes & derived stats, ability framework (skills/spells/songs/powers), specializations & legacies |
| **Loot & gear** | Equipment slots, set bonuses, **scarcity-tiered itemization** (see below), ARPG affixes |
| **Factions** | Cabals/guilds/religions with deities, **AI-led ranks**, item-of-power warfare/raiding |
| **World** | Zones with resets/repop, navigation, quests & epic questlines, interactables, bestiary |
| **Risk** | Permadeath, aging, survival needs, **full-loot PvP**, configurable PK rules |
| **Roleplay** | Character roles, **Calyx-measured RP quality** that advances your rank and unlocks powers |

### 💎 A scarcity economy with real stakes

The most powerful items are **finite in the entire world** — maybe one or two legendaries exist, ever. While you hold one, the monster that drops it *can't respawn with it*. Want it? **Kill the holder and loot everything.** Common gear is unlimited so you can always re-equip and get back in the fight. And if you hoard rare items and stop logging in for a week, they **decay back into the world** — power must be *used*, not locked away.

### 🎭 Roleplay *is* progression

AI cabal and religion leaders **watch how you play** and measure — with Calyx's grounded intelligence — how well you embody your faction's theme. Roleplay well, and they promote you, interact with you, and unlock new powers. This is a primary way you *grow*.

---

## 🏛️ Architecture — three tiers, strictly separated

![Three-tier architecture](docs/readme/10-architecture.png)

```
🧠 KNOWLEDGE / AI TIER  — Calyx (Rust, GPU) + the 50-lens brain + AI agents
        │  one-way "lowering valve"  (offline, fingerprinted)
⚙️ DETERMINISTIC SIM    — fixed-point, zero-alloc Go core   ← AI never runs here
🎨 PRESENTATION         — g3n render + audio (GPU per-frame) ← AI never in the frame
```

The AI tier is brilliant but non-deterministic; the sim core is bit-exact and replayable. The **lowering valve** is the only bridge — learned intelligence becomes frozen game data, one way. This is what lets the world be *alive* and *deterministic* at the same time.

---

## 💻 Built for your machine — maxed out

No compromises, no min-spec. The engine targets one high-end workstation and uses every watt of it:

| | |
|---|---|
| **CPU** | AMD Ryzen 9 9950X3D — 16C/32T, 128 MB 3D V-Cache, AVX-512 |
| **GPU** | NVIDIA RTX 5090 — 32 GB GDDR7, Blackwell (sm_120), CUDA 13.3 |
| **RAM** | 128 GB DDR5 |
| **Everything local** | Calyx + 50 embedders + AI agents + g3n, all on one box. No cloud. |

---

## 🗺️ Roadmap

- **Phase 1 — Single-player, local-only, fully agent-driven.** Every NPC, creature, and object is a Calyx agent. Build toward recreating Carrion Fields, solo and local.
- **Phase 2 — Multiplayer & persistent world.** Full-loot PvP, anti-hoard item decay, AI agents as player characters, persistent ORPG characters.
- **Future expansion** *(deferred)* — cloud hosting and a creator marketplace, once the local engine is complete.

---

## 🎯 The end goal

![What will you build](docs/readme/11-future.png)

> **Recreate Carrion Fields** — every system, rendered visually, with AI running every Immortal, cabal, and NPC. When we can do that, we can build *anything* — and every other kind of game becomes a small feature add-on.

This is the most ambitious no-code game creator ever attempted: **the depth of a 30-year MUD, the accessibility of the Warcraft III editor, the beauty of a modern real-time engine, and the soul of an AI that brings every world to life.**

**What will you build?**

---

<div align="center">

*Light in the Dark — by what name do you wish to be remembered?*

**[📋 Browse the backlog →](../../issues)** · Built with Calyx + g3n · Windows + RTX 5090

</div>
