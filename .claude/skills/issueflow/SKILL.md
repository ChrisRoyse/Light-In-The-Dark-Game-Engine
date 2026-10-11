---
name: issueflow
description: File and manage GitHub issues the GameEditor house way. Use when creating/updating issues or epics, decomposing an epic into atomic children, pinning a child-roster comment, or before any gh issue create. Covers title taxonomy ([EPIC]/[DOCTRINE], area prefixes stealth:/world:/rpg:/ux:/content:, P0-P2), body form (parent refs, CF-factual ids, 3D design, balance, FSV gate line), and gh CLI traps (heredoc, 8KB batch cap, 504 verify, dedup).
---

# Issue flow (GameEditor house way)

GitHub issues are the SINGLE source of project state. North-star #959: recreate
Carrion Fields. Every issue must be legible to that goal. There is NO CI; the only
resolution proof is FSV in reality (#800). Match the existing corpus exactly -- read
a sibling issue before filing (`gh issue view <n>`).

## 1. ALWAYS search before filing (dedup)

Issue numbers share a counter with PRs and can skip; duplicates DO happen (see #1700
vs #1701, a real dup pair). Before every create:

```bash
gh issue list --state all --search "sneak stealth acuity" --limit 20
```

Search a few keyword sets. If a match exists, comment/cross-link instead of re-filing.

## 2. Title taxonomy

Bracket prefixes, then an area prefix, then a specific noun phrase:

- `[EPIC]` -- a parent that decomposes into atomic children. `[DOCTRINE]` -- a
  binding methodology/decision doc. Combine when both: `[DOCTRINE][END GOAL]` (#959).
- Area prefix on non-epic children, lowercase + colon: `stealth:` `world:` `rpg:`
  `combat:` `magic:` `item:` `content:` `ux:` `balance:`. Pick the one system the
  issue advances. Examples: `stealth: acquisition matrix -- ...` (#1623),
  `world: standalone area registry -- ...` (#1704), `ux: HUD & prompt system -- ...`.
- Severity: priority rides as a LABEL (`priority:p0`/`p1`/`p2`), not the title, on
  epics/doctrine. If a title-level severity tag is wanted, use `[P0]`..`[P2]` right
  after the bracket prefixes. Default new children to no priority label unless told.
- Keep titles concrete and scannable; a long dash-separated clause listing the scope
  is idiomatic (see the open list). ASCII only -- use `--` not an em dash.

## 3. Body conventions (in this order)

1. **Parent refs FIRST line.** `Parent: #945 (MUD depth) / #933 (RPG flagship).
   North-star #959.` then feeds/related refs. Children of an epic name the epic:
   `Parent #1597; feeds #986/#985.`
2. **CF-factual facts inline WITH source ids.** State mechanics as transcribed data,
   never invented, and cite the helpfile/wiki id right there:
   `Source of truth (factual mechanics, transcribed <date>): carrionfields.net
   helpfiles 503/1169/467 + wiki.carrionfields.net class/race pages.` For exact
   numbers, inline them (`thief L4, assassin L4, bard L30; elf innate`). Use the
   /cf-source skill to gather these correctly.
3. **3D design / translation section.** How the CF text mechanic becomes a real-time
   3D system, citing the MUD-to-3D doctrine #1672. This is non-optional for gameplay
   issues -- CF booleans become continuous 3D (sound/light/shader/server perception).
4. **Balance levers.** What knobs tune it (levels, radii, costs, cooldowns), and the
   drift check vs the balance framework #1680 / ability framework #961.
5. **Originalization note** where content is involved: `Originalize names/lore for
   public release per #984; mechanics are factual.`
6. **FSV resolution gate LINE, always last.** One of:
   - `Resolution gate: Full-State-Verified in REALITY via Synapse before close
     (#800); no CI/CD.`
   - epic form: `Resolution gate per #800: each child Full-State-Verified in REALITY
     via Synapse; epic closes when <the whole-parent walk> passes.`
   - terse child form: `FSV: data-load + cross-ref test. #800.`

references/issue-template.md is a fill-in skeleton -- copy it, delete unused sections.

## 4. Atomic decomposition (epics)

Decompose so that **completing ALL children provably completes the parent** -- no gap,
no overlap. Group children by role (Core systems / Abilities / Counters / Integration,
or Systems / Cities / Clusters / Ladders for world). Each child is independently
FSV-able. The epic body carries a prose child-scope outline; the exact numbers are
NOT known until the children exist (shared PR/issue counter skips).

**After** children are created, pin a **child-roster COMMENT** on the epic listing
every child number under its role heading, ending with
`Epic closes when all of the above are FSV-accepted.` (see #1597's roster comment).
Update the roster comment when a later wave adds children -- add a dated addendum
comment rather than editing silently.

## 5. gh CLI mechanics on this box (Windows, Git Bash)

Use the **bash heredoc** form so multi-line bodies survive quoting:

```bash
gh issue create \
  --title "stealth: creep -- ranger L19 terrain-gated approach vs alert NPCs" \
  --body "$(cat <<'EOF'
Parent #1597; feeds #1020-#1036. North-star #959.
...body...
FSV: <what proves it real>. #800.
EOF
)"
```

- **Keep each command batch under ~8KB.** A larger heredoc truncates and dies with a
  cryptic `unexpected EOF while looking for matching quote`. Split big filings into
  several create calls; put long tables in a follow-up comment, not the create body.
- **Large or important bodies: use `scripts\new-issue.ps1`** -- write the body to a file
  (Write tool), then `powershell -NoProfile -File scripts\new-issue.ps1 -Title "..." -BodyPath <file>`.
  Errors loudly, survives any body size, verifies the created issue by readback, and
  detects the gh-504-created-anyway case. FSV-proven on issue #1707.
- **After any 504 / timeout, VERIFY before retrying** -- GitHub may have created the
  issue anyway. `gh issue list --state all --search "<title words>" --limit 10`; only
  re-create if truly absent (a blind retry makes the #1700/#1701 dup).
- Cross-link with comments on the parent (`gh issue comment <parent> --body ...`),
  not just body refs, so the parent's thread shows the new child.
- Quote the title (it holds `:` and `--`). Confirm each create prints the new URL.

## References
- references/issue-template.md -- copy-paste fill-in body skeleton (child + epic).
