# gopher-lua fork — LITD patch log

The Lua runtime is a **vendored fork of [`yuin/gopher-lua`](https://github.com/yuin/gopher-lua)**
under `repoes/gopher-lua` (gitignored, like the g3n engine fork), wired into
the build by a `go.mod` replace directive. This file is the tracked record of
what the fork is pinned to and which local patches sit on top of it.

> Decision: **D-2026-06-11-25** (decisions.md) — embedded deterministic Lua VM.
> Rationale (why gopher-lua specifically): VM-level coroutines are plain heap
> data, so they serialize into saves (R-SIM-6 / S-5); `pairs()` iterates in a
> deterministic, insertion-influenced order and never ranges a Go map; and
> number→string conversion is pure-Go `strconv`, identical across platforms —
> no libc/printf locale or arch drift.

## Pinned upstream

| | |
|---|---|
| Upstream repo | `https://github.com/yuin/gopher-lua` |
| Module path | `github.com/yuin/gopher-lua` |
| Pinned commit | `75f497656b1c6864139dd2a7d88cf96d09550814` |
| Upstream describe | `v1.1.2-1-g75f4976` |
| go.mod require | `github.com/yuin/gopher-lua v1.1.1` (resolved via replace to the local tree) |
| Replace | `replace github.com/yuin/gopher-lua => ./repoes/gopher-lua` |

Do **not** bump the upstream pin without re-applying every LITD patch below.
The marker convention matches the engine fork: each patch hunk carries a
`// LITD-PATCH` comment so the full set is greppable:

```bash
grep -rn "LITD-PATCH" repoes/gopher-lua | wc -l
```

## Restore (fresh clone)

`repoes/` is gitignored; a fresh checkout restores the fork with:

```bash
git clone https://github.com/yuin/gopher-lua repoes/gopher-lua
git -C repoes/gopher-lua checkout 75f497656b1c6864139dd2a7d88cf96d09550814
# then re-apply the LITD patches below (none yet at #261)
```

## Patches

The four D-25 patches land in their own issues on top of this pinned base:

| # | Patch | Issue | Status |
|---|---|---|---|
| 1 | instruction-budget counter in `mainLoop` | #262 | **done** (fork commit `46381dc`) |
| 2 | deterministic mathlib (fixed-point; `math.random` → sim PRNG) | #263 | pending |
| 3 | coroutine / `LState` persister (serialize suspended coroutines) | #264 | pending |
| 4 | `LState`/call-frame pooling + golden cross-arch CI test | #265 | pending |

### Patch 1 — instruction budget (#262)

Files: `value.go` (LState fields `litdInstrLimited`/`litdInstrLeft`),
`vm.go` + `_vm.go` (`mainLoop` and `mainLoopWithContext` per-instruction
check), `litd_budget.go` (`SetInstructionBudget`/`RemainingBudget`/
`InstructionBudgetEnabled`). The check is `if L.litdInstrLimited { if
L.litdInstrLeft<=0 { RaiseError } ; L.litdInstrLeft-- }` at the top of each
dispatch — checked-then-decremented, so a budget of N runs exactly N
instructions and the (N+1)th raises. Allocation-neutral; overhead below the
benchmark noise floor (see #262 closing comment). `_vm.go` is `_`-prefixed
(Go-tool-ignored template) — patched for regen fidelity, not compiled.
