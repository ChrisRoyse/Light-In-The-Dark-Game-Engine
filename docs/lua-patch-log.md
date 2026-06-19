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
| 2 | deterministic mathlib (`math.random` → sim PRNG; `randomseed` disabled) | #263 | **done — random half** (fork commit `b579629`); transcendental golden half blocked on #284 |
| 3 | coroutine / `LState` persister (serialize suspended coroutines) | #264 | **complete** — VM snapshot/restore (`5c53948`/`54426ff`) + closure/upvalue accessors (`a05c253`); luabind save/load of the full value graph: register closures with shared upvalues (`331aee9`), nested coroutines (`ef55fbd`), frame-closures-with-upvalues (`ea5204d`), and userdata→host-handle rebind via the `HandleMarshaler` seam — handle marshals to an opaque token, rebinds with shared identity on cold load, fails closed (loud) without a marshaler. The api codec it resolves through is `litd/api` `RefOf`/`Game.Resolve` (#267, `c2b8850`) |
| 4 | `LState`/call-frame pooling + golden cross-arch CI test | #265 | pending |
| S | deterministic memory-budget accountant (`string.rep` charge) | #266 | **done** (fork commit `d855815`) |

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

### Patch 2 — deterministic random (#263), random half

Files: `value.go` (LState field `litdRand func() float64`), `litd_math.go`
(`SetRandomSource`/`RandomSourceBound`), `mathlib.go` (`mathRandom` draws from
`litdRand`; `mathRandomseed` raises; `math/rand` import removed). With no
source bound `math.random` raises `litd: math.random has no deterministic
source bound (R-SIM-2)` — fail-closed, no nondeterministic fallback. One draw
advances the shared stream once: `random()`→[0,1) is the raw `u`; `random(n)`→
`int64(u*n)+1`; `random(m,n)`→`int64(u*(n-m+1))+m`. The host (luabind) wires
`litdRand` to the sim's seeded PRNG so Lua and Go-side draws share one hashed
stream. `math.randomseed` is a loud sandbox error: the seed is sim state.

**Deferred (blocked on #284):** the transcendental half — replacing
`math.{exp,log,log10,pow,sin,cos,tan,…}`'s `math/*` libc-equivalent calls with
fixed-point / golden-table evaluators verified bit-identical across arch. That
needs a cross-arch golden-vector CI matrix (the Actions runners gated on #284)
and new `litd/fixed` primitives (exp/log/pow are absent today). Tracked there;
the random half above stands on its own and is fully verified (FSV in
`litd/luabind/mathrand_test.go`).

### Patch S — memory-budget accountant (#266)

Files: `value.go` (LState fields `litdMemLimited`/`litdMemLeft`), `litd_mem.go`
(`SetMemoryBudget`/`RemainingMemory`/`MemoryBudgetEnabled`/`litdCharge`),
`stringlib.go` (`strRep` charges `len(str)*n` before `strings.Repeat`). The
sandbox (R-SEC-1) needs a CPU **and** a memory ceiling; the instruction budget
(patch 1) bounds iterative allocators but not a single-instruction bomb like
`string.rep(s, 2^30)`. This accountant is deterministic — it charges *requested
bytes* (not RSS, which would be GC-/arch-dependent), so the same script breaches
at the same point everywhere — and fail-closed: the charge happens BEFORE the
allocation, so a bomb is rejected without the process ever allocating it.
Lettered "S" (sandbox) rather than numbered: it is the #266 deliverable, not one
of the four D-25 patches. Verified in `litd/luabind/sandbox_escape_test.go`
(`TestSandboxMemoryBomb` prints HeapAlloc before/after — heap does not balloon).

### Patch 4a — zero-alloc coroutine resume (#265)

Files: `value.go` (LState field `litdResumeRet []LValue`), `state.go`
(`(*LState).Resume`). The Go-host resume API allocated a fresh
`make([]LValue, …)` for the yield/return payload on **every** resume. A suspended
Lua coroutine waking each sim tick (the hot Lua path) therefore cost one alloc
per resume per tick — an R-GC-1 steady-state violation. The patch appends into a
reused per-host buffer (`ls.litdResumeRet[:0]`) instead. **Contract:** the slice
`Resume` returns aliases that buffer and is valid only until the next `Resume` on
the same LState. The host resume API has a single sequential consumer on the sim
goroutine (`litd/luabind/sched.go` reads the yielded value immediately and never
retains it across a later resume), so the alias is safe. `coResume`
(coroutine.resume in Lua) drives the VM through `threadRun` directly, not through
`(*LState).Resume`, so it is unaffected. Pooling is invisible to VM semantics
(values unchanged) — the #271 determinism golden (`0xcb2b8f8681a2de23`) is
byte-identical before and after.

The companion luabind-side change (no fork edit) is in `sched.go`: `PolledWait`
now stashes its wait seconds on a scheduler field and yields with **no** value,
instead of `L.Yield(lua.LNumber(secs))` — which boxed a float into an interface
(the second per-tick alloc). Together they take a coroutine resuming every tick
from **2 → 0 allocs/op** (steady state). Verified in
`litd/luabind/sched_alloc_test.go` (`TestScriptResumeZeroAllocFSV` — fails at 2/op
before the patch, passes at 0/op after).

Lettered "4a": this is the steady-state-alloc slice of D-25 patch 4 (#265). The
remaining patch-4 scope — cross-world LState pooling and the golden cross-arch CI
matrix — is still open (the matrix is blocked on CI billing #284).

### Patch T — small-table fast path (#416)

Files: `value.go` (LTable field `skv []tableKV` + the `tableKV` type), `table.go`
(`newLTable`, `RawSet*`/`RawGet*`, `RawSetH`, `ForEach`, `Next`, plus `smallFind`
/`spill` helpers and the `smallTableMax` const). A pure string-keyed table — the
engine's ubiquitous `{x=…, y=…}` vector and small config tables — previously
allocated **four** heap objects on construction: the `LTable` struct, the
`strdict` map, the `keys` slice, and the `k2i` map, plus an `LString` interface
box per key. At 20 tps with many scripted units reading/writing positions every
tick this was the dominant per-tick GC cost (issue #416, measured ~11 allocs per
`{x,y}` table).

The patch keeps a small string-keyed table's entries in one insertion-ordered
slice `skv []tableKV{key string; val LValue}` — no maps, no `LString` boxing of
keys. The table **spills** to the original `strdict`/`keys`/`k2i` representation
(a permanent one-way transition) the moment a non-string (dict) key is added or
`skv` would exceed `smallTableMax` (8). The array part (`tb.array`) is independent
and coexists with either mode. All semantics are preserved exactly: insertion
order, the LNil-tombstone-on-delete behaviour (a deleted key keeps its slot with
`val == LNil`, skipped on iteration, re-set without duplication), and mixed
array+string iteration. `spill()` migrates `skv` into the maps in order,
reproducing tombstones, so post-spill behaviour is identical to a table that was
never small.

Encapsulation makes this safe: no code outside `table.go` touches the LTable
internals, and the #264 save persister walks tables through `Next`/`ForEach`
(public API), not fields.

Result: a `{x,y}` literal drops from **11 → 3 allocs**; a per-tick
`Unit_Position` read + `Unit_SetPosition` write handler from **23 → 9 allocs/op**.
Verified: fork Go-level table tests (`TestTable*`) and Lua fixtures
(`table.lua`/`vm.lua`/`closure.lua`) pass; the #271 determinism golden
(`0xcb2b8f8681a2de23`) is byte-identical (pairs() order preserved); and
`litd/luabind/table_smallmode_test.go` (`TestSmallTableSemanticsFSV` +
`TestVectorTableAllocFSV`) locks behaviour across the spill/tombstone/mixed/array
cases and the alloc bound.

Lettered "T" (table): a performance patch on the LTable representation, not one of
the four D-25 determinism patches.
