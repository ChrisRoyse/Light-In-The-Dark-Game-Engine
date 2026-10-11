# DEC-1519: Calyx is a Blueprint - Fully Customize It Per-Application; the "Clean Vendored Snapshot / Preserve on Re-Vendor" Rule is Retired

- **Status:** DECIDED (operator directive on #1519, 2026-07-10; this document is the durable record)
- **Type:** decision (doctrine #800 - binding)
- **Decides:** what `C:\code\GameEditor\Calyx` IS to this repo - an owned, editable, app-specialized copy, NOT a pristine mirror of upstream `calyx-dev` kept in sync
- **Retires:** the earlier constraint "the only local edit is the ONNX provider policy; preserve on re-vendor; keep it a clean snapshot"
- **Still-binding:** the cargo-deny advisory/license gate for Calyx (`scripts/calyx-deny.toml` + `scripts/lint.ps1`, #1517) - customization MUST keep Calyx advisory-clean and permissive-licensed
- **Consistent with:** #1348 (Calyx is the single source of truth for structured intelligence) and the binding builder's handbook docs2/BUILDING_ON_CALYX.md (fix problems at the right layer, often inside Calyx)

## 1. Context

`C:\code\GameEditor\Calyx` is GameEditor's OWN copy of Calyx. An earlier working
rule treated it as a clean vendored snapshot of upstream `calyx-dev`: the only
sanctioned local edit was the ONNX provider policy, and any local change had to be
"preserved on re-vendor" so the tree stayed a pristine mirror that could be re-synced
from upstream.

That rule described a relationship this project does not actually have. There is no
re-vendor step to protect against; there is no upstream sync this repo performs.
Calyx here is the substrate GameEditor builds its structured intelligence on, and it
is meant to be specialized for GameEditor. Freezing it as an untouchable snapshot did
not keep the tree clean - it forced editor-side workarounds that were strictly worse
than fixing the problem at its source inside Calyx (see the rationale in section 5).

## 2. The Doctrine (operator directive, 2026-07-10)

**Calyx is a blueprint. When used in an application it must be fully customized.**

A blueprint gives structure; an application gives it purpose. `C:\code\GameEditor\Calyx`
is a blueprint to be fully customized in-app - edited, extended, and specialized for
GameEditor - not a copy to be kept identical to some external original.

## 3. What Changes

- **Editing Calyx is expected and encouraged.** When a problem is best fixed inside
  Calyx - a missing static accessor, a policy hook, a seam the editor needs, a bug -
  fix it there. Do NOT contort the editor to avoid touching Calyx.
- **Fix problems at the right layer.** If the correct layer for a change is inside
  Calyx, that is where the change goes. This matches the handbook stance in
  docs2/BUILDING_ON_CALYX.md: extend Calyx rather than bolting a workaround onto the
  layer above it.
- **The "clean snapshot / preserve on re-vendor / pristine mirror" constraint is
  retired.** There is no re-vendor step; Calyx edits are permanent app customizations,
  not local patches to be re-applied after a sync that never happens.

## 4. What Stays Binding - the cargo-deny Gate (#1517)

Customization is NOT a licence to import risk. The cargo-deny advisory/license gate
for Calyx STILL APPLIES to every edit:

- `scripts/calyx-deny.toml` is the Calyx advisory/license policy; `scripts/lint.ps1`
  runs `cargo-deny check` against it for the Calyx workspace (#1517).
- Any customization MUST keep Calyx **advisory-clean** (no known-vulnerable
  dependencies) and **permissive-licensed** (no copyleft or non-permitted licences
  entering the tree).
- A change that would introduce an advisory or a disallowed licence is rejected by
  the gate, exactly as before. "Calyx is editable" does not weaken this.

### Existing app-specific Calyx customizations (all permanent)

These are the current, deliberate GameEditor specializations of Calyx. They are
permanent app customizations, not snapshot drift to be reverted:

- **`calyx-registry` configurable ONNX provider policy** -
  `from_lens_spec_with_policy` (the original sanctioned local edit; now simply one
  customization among others).
- **`Calyx/Cargo.lock` security bumps** - crossbeam-epoch 0.9.20 and memmap2 0.9.11,
  taken to keep the tree advisory-clean under the #1517 gate.
- **`calyx-mcp` `McpServer::tool_requires_authn` static accessor** (#1488) - the seam
  the MCP bridge needs to classify whether a tool mutates state WITHOUT executing it.

## 5. Rationale

A frozen snapshot forced editor-side workarounds instead of the right fix. The clearest
case is #1488: the MCP bridge needed to know whether a Calyx tool mutates state, but the
snapshot rule discouraged adding the obvious static accessor to Calyx. The workaround
was to classify a tool's mutation behavior by **executing the tool** - running code to
learn metadata that a one-line static accessor could have exposed directly. That is a
worse design (side effects, cost, fragility) adopted only to avoid touching Calyx.

Under this doctrine the correct move is the direct one: expose the seam
(`McpServer::tool_requires_authn`) inside Calyx and let the editor read it. The blueprint
is customized to serve the application, and the workaround disappears.

## 6. Consequences

- No future work should be shaped around "don't edit Calyx." When the right layer for a
  fix is inside Calyx, edit Calyx.
- No agent or session should re-introduce a "keep Calyx a clean vendored snapshot",
  "preserve on re-vendor", or "pristine mirror" rule. That constraint is retired here;
  reasserting it contradicts binding doctrine and must be rejected loud.
- Every Calyx edit still passes the #1517 cargo-deny advisory/license gate. Editable is
  not the same as unguarded.
- This decision governs only what Calyx IS to this repo and how it may be changed. It
  does not alter the Calyx architecture or the builder's method in
  docs2/BUILDING_ON_CALYX.md, which remain binding.

## 7. Sources

- Operator directive on #1519 (2026-07-10): "Calyx is a blueprint. When used in an
  application it must be fully customized."
- Repo doctrine: #800 (research-first / FSV resolution gate, binding), #1348 (Calyx is
  the single source of truth for structured intelligence).
- Still-binding gate: #1517 (`scripts/calyx-deny.toml` + `scripts/lint.ps1` cargo-deny
  advisory/license policy for the Calyx workspace).
- The workaround this doctrine retires: #1488 (MCP authn classification that executed a
  tool because the bridge could not read Calyx's static authn metadata; now a static
  `McpServer::tool_requires_authn` accessor).
- Binding builder's handbook: docs2/BUILDING_ON_CALYX.md.
- Prior decision-doc convention: docs2/decisions/DEC-957-hybrid-world-model.md,
  docs2/decisions/DEC-1474-script-escape-hatch.md.
