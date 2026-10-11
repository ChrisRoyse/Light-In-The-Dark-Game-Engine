# Issue body templates (fill-in)

ASCII only. Copy the relevant block, replace <...>, delete sections that do not apply.
Order is fixed: parent refs -> CF facts+ids -> 3D design -> balance -> originalize ->
FSV gate LAST. See SKILL.md sections 3-4.

---

## A. Atomic child issue

Title: `<area>: <specific noun phrase> -- <one-clause scope>`
(area = stealth: / world: / rpg: / combat: / magic: / item: / content: / ux: / balance:)

Body:

```
Parent #<epic> (<name>); feeds #<downstream>/#<downstream>. North-star #959.

<One or two sentences: what this delivers and why the parent needs it.>

CF-factual mechanics (transcribed <YYYY-MM-DD>): <exact numbers/behavior inline>.
Source of truth: carrionfields.net helpfiles <id>/<id> (+ wiki.carrionfields.net
class/race pages / forums topic <n> / itemsearch). Originalize names/lore per #984;
mechanics are factual.

3D translation (per doctrine #1672): <how the CF text mechanic becomes a real-time
3D system -- sound/light/shader/server-authoritative perception, animation, terrain>.

Balance levers: <levels/radii/costs/cooldowns knobs>; drift check vs #1680 (#961).

Deliverable: <the concrete artifact -- data table / system / doc / UI>.
FSV: <the smallest reality check that proves it, run via Synapse>. #800.
```

---

## B. Epic issue

Title: `[EPIC] <system> -- <benchmark clause> (CF-grade ...)`

Body:

```
Parent: #945 (MUD depth) / #933 (RPG flagship). North-star #959. Feeds #<...>.

**Goal: <every CF mechanic in this system, rebuilt as first-class 3D real-time
systems>.** <Why it matters to the PK/RP meta.>

## The translation doctrine (operator direction, <date>)
<Bullets: how the CF booleans/rooms/rounds become continuous 3D. Cite #1672.>

## Child issues (atomic; all must close for this epic)
<Prose outline grouped by role -- Core / Abilities / Counters / Integration, or
Systems / Cities / Clusters / Ladders. NO numbers yet; they are unknown pre-create.>

Source of truth (factual, transcribed <date>): carrionfields.net helpfiles <ids> +
wiki.carrionfields.net. Originalize names/lore per #984; mechanics are factual.

Resolution gate per #800: each child Full-State-Verified in REALITY via Synapse; epic
closes when <the whole-parent walk -- e.g. every area reachable, every class buildable>
passes. No CI/CD.
```

---

## C. Epic child-roster COMMENT (pin AFTER children exist)

```
Child roster (filed <date>, complete atomic set):
<Role heading>: #<n> <label>, #<n> <label>, ...
<Role heading>: #<n> <label>, ...
Epic closes when all of the above are FSV-accepted.
```

Later waves: add a NEW dated comment (`<System> layer extended (<date>): #<n> ...`)
rather than editing the original roster.

---

## D. Doctrine issue

Title: `[DOCTRINE] <methodology/decision> -- <the canonical thing it fixes>`

Body: state the decision, the operator-approved canonical mapping/rule (numbered list
if a mapping table), the deliverable doc path (e.g. docs2/MUD_TO_3D.md), and who cites
it. End with `FSV: <doc exists / field live>. #800.`
