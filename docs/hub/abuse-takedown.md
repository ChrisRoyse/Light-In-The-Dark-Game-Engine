# World Hub — Abuse Report & Takedown Process

**Status:** v1 process (D-2026-06-11-23; we operate the hub on our own
infrastructure per D-21/D-22, so we are the operator of record).
**Enforcement mechanism:** the content-hash blocklist honored by the index
generator (issue #181's enforcement half; index service #175).

## How to report

Email **abuse@lightinthedarkanalytics.com** with: the world's hub URL or content
hash, what the problem is, and — for IP claims — who you are and what you own.
One report per world. Reports are read by a human; no automated takedowns.

## What gets taken down

1. **Illegal content** under the operator's jurisdiction.
2. **IP infringement** — credible ownership claims against world content
   (models, audio, text, trademarks). Provenance metadata in the archive's
   MANIFEST (G4.7) is the first evidence consulted.
3. **Sandbox-bypass attempts** — worlds whose scripts probe or exploit the Lua
   sandbox (R-SEC-1). These are also security issues: the takedown is immediate
   and a `type:security` engine issue is filed from the sample.

Bad balance, bad taste, and bad reviews are not abuse.

## Who decides, and how fast

The operator (Light in the Dark Analytics) decides. Target timeline:
**acknowledge within 72 hours, decide within 14 days**; sandbox-bypass reports
are acted on immediately on confirmation, ahead of the timeline.

## What a takedown does

- The world's **content hash enters the blocklist**; the next index generation
  drops its entry and its download URL returns **HTTP 410** with a short notice
  naming the takedown reason category.
- **Re-uploads of the same hash are refused at intake**, citing the blocklist.
  A genuinely revised world has a new hash and is judged on its own.
- Copies players already downloaded are untouched — there is no remote kill,
  by design.

## Appeal

The publisher may reply to the takedown notice email once, with evidence
(for IP: license or ownership proof). Same decision timeline. The decision
after appeal is final for that content hash.

## Records

Each takedown keeps: the report, the decision and its reason category, dates,
and the blocklisted hash. Records are private to the operator; the public sees
only the 410 notice category.
