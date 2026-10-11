# DEC-1474: Custom-Script Escape Hatch = WASM (wasmtime), Fuel-Metered, Deterministic

- **Status:** DECIDED (recorded on #1474, 2026-07-10; this document is the durable record)
- **Type:** decision (doctrine #800 §5 — research-first)
- **Decides:** the Rust-era replacement for the WC3-parity "custom script" escape hatch below the GUI/ECA trigger system (#803)
- **Supersedes:** the Go-era gopher-lua plan (#769, closed; #803/#834 original wording)
- **Unblocks:** #834 (implementation tracker, rewritten)

## 1. Context

WC3's power ceiling came from the JASS (later Lua) escape below GUI triggers. The current
trigger system is GUI/ECA only; advanced creators and AI agents (#717) have no down-shift
when ECA blocks are insufficient.

At decision time, no scripting runtime existed in the workspace. The 2026-07-11
pre-implementation baseline was:

```
grep mlua|rlua|rhai|wasmtime|wasmer across **/Cargo.toml  ->  zero hits
```

and the prior #1474 FSV confirmed `Cargo.lock` contained none of
`mlua/rlua/lua/hlua/wasmtime/wasmer/wasmi/rhai/extism/deno_core`; source grep for
`lua_State/script_engine/ScriptRuntime/gopher-lua` returns nothing.

### The binding sim contract (the decision's dominant constraint)

Whatever runs creator logic inside the sim tick MUST satisfy:

1. **Fixed-point math** — the sim core is fixed-point (#1463 determinism boundary);
   guest logic on sim-affecting paths must not do IEEE-float math.
2. **Replayable / lockstep** — bit-identical execution across runs and machines;
   only commands cross the wire; state hashes must match (#1472 tracks divergence).
3. **No allocation on the hot path** (#800/#1461).
4. **Instruction-metered** — a runaway script must halt at a deterministic point,
   identically on every peer, or the lockstep hash diverges.
5. **Fail-closed** — invalid/nondeterministic/over-budget scripts are rejected loud
   (`GATE_FAILED:`-style, greppable), never silently degraded.
6. **Untrusted UGC** — marketplace-published scripts (#691/#754, #1473 trust & safety)
   run with zero ambient authority.

## 2. Options Matrix

| Criterion (sim contract) | A. Lua (mlua, restricted) | B. WASM (wasmtime, fuel) | C. Rhai (native Rust) | D. No escape (ECA-only) |
|---|---|---|---|---|
| Cross-machine bit determinism | POOR — native f64 core; NaN sign/payload, `tostring(float)` via libc, table iteration order (`pairs`/`next`) depends on pointer addresses and VM invocation; LuaJIT trace timing adds more | STRONG — spec is deterministic except NaN payloads + relaxed-SIMD/threads; wasmtime closes NaN via `cranelift_nan_canonicalization(true)`, and we disable the nondeterministic proposals | MODERATE — tree-walking interpreter in Rust, deterministic if float ops are kept out, but no spec-level guarantee and `Dynamic`/map ordering must be audited per release | TRIVIAL — no guest code |
| Deterministic instruction metering | WEAK — debug-hook counting (`lua_sethook` every N instrs) is coarse and interacts with GC timing; GC pauses are themselves nondeterministic observable via `collectgarbage("count")` | STRONG — fuel metering is *fully deterministic*: same module + same fuel traps at the identical instruction, every run, every machine (wasmtime docs; epoch interruption is cheaper but NOT deterministic — fuel only for sim scripts) | MODERATE — `set_max_operations` halts runaway scripts deterministically for a fixed engine version, but "operation" granularity is not spec-stable across rhai versions | N/A |
| Fixed-point discipline enforceable | WEAK — Lua numbers are f64 (5.3+ has i64 but stdlib mixes freely); would require forking the VM's number model | STRONG — host imports are the only sim surface; expose ONLY typed fixed-point API, validate modules to reject f32/f64 sim-path usage if desired | MODERATE — can register only fixed-point types, but script-local float literals still exist | N/A |
| Hot-path allocation | WEAK — Lua GC allocates constantly; incremental GC timing observable | STRONG — linear memory pre-allocated; pooling allocator + pre-instantiation = zero per-tick host alloc | WEAK — AST walk allocates `Dynamic` values per op | N/A |
| Sandbox for untrusted UGC | MODERATE — mlua sandbox mode exists, but C-VM CVE surface; stdlib must be aggressively stripped | STRONG — linear-memory isolation, no ambient authority, deny WASI by default; the industry-standard UGC sandbox | MODERATE — safe Rust, but engine shares address space semantics via registered types | N/A (no UGC code at all) |
| Authoring familiarity (WC3 audience) | STRONG — WC3 Reforged itself moved JASS→Lua; modders know Lua | MODERATE — nobody writes .wat by hand; BUT WASM is a *target*: creators author in Rust/AssemblyScript/C, and the editor's "convert GUI→text" surface (#834) is a small DSL that compiles to wasm | WEAK — niche language, no modding-community mindshare | POOR — hard power ceiling; states "no parity" in doctrine |
| AI-agent authorable (#717) | MODERATE — agents emit Lua fine, but determinism lint burden lands on us | STRONG — language-agnostic compile target; agents emit WAT or compile Rust; validation is mechanical (module validation + import allowlist) | MODERATE | N/A |
| Rust-host purity (#1071 no-cgo) | MODERATE — mlua vendors/binds a C VM (or Luau, also C++) | STRONG — wasmtime is pure Rust | STRONG — pure Rust | STRONG |
| Lowering-valve fit (#1348) | MODERATE — source text as content; but VM state is opaque to provenance | STRONG — compiled `.wasm` is a hashable, provenance-tracked, gated artifact; ingest → validate → lower like any asset | MODERATE | STRONG |

### Why not each runner-up

- **Lua (A):** every determinism property we need is a *retrofit against the VM's design*
  (f64 numbers, GC timing, pointer-order iteration, libc-dependent formatting). WC3
  familiarity is real but buys us an unbounded audit surface. The Go engine's gopher-lua
  needed custom determinism patches (#769) — that is the failure mode we are exiting, not
  re-entering in C.
- **Rhai (C):** best-in-class embedding ergonomics and pure Rust, but determinism is
  *implicit* (whatever the interpreter happens to do this release), metering granularity is
  not contractual, and per-op allocation violates the hot path. Fine for editor-side tooling
  scripts; wrong for the sim tick.
- **No escape (D):** cheapest, but permanently caps the ceiling below WC3 parity and
  contradicts the project's stated goal; would force rewriting #803's charter.

## 3. Decision

**The escape hatch is a fuel-metered, sandboxed WebAssembly module runtime hosted by
`wasmtime`.** Option B. (Recorded by operator on #1474, 2026-07-10.)

### Binding determinism configuration

Any host that executes guest sim-logic MUST set:

1. `Config::cranelift_nan_canonicalization(true)` — closes the one spec-level float
   nondeterminism (NaN payloads), the technique used by consensus systems (NEAR).
2. `Config::consume_fuel(true)` + a per-invocation fuel budget. Over-budget = trap =
   loud fail (`GATE_FAILED: step=script_fuel detail=<module> budget=<n>`); never hangs
   the tick. Fuel, NOT epoch interruption (epochs are wall-clock — nondeterministic).
3. Disable relaxed-SIMD, threads, and every proposal whose spec permits
   nondeterministic results.
4. **No ambient nondeterminism:** no `time`/`random`/fs/net host imports for sim-logic
   guests; deny WASI entirely for UGC. The ONLY imports are the sim's typed
   **fixed-point** state API (#1463).
5. Hot-path discipline: pooling allocator, pre-instantiated modules, zero per-tick
   allocation on the invoke path (#800/#1461).
6. Pin the wasmtime major version in lockstep-relevant releases; a wasmtime upgrade is a
   replay-format-breaking change and must bump the replay/protocol version.

## 4. Integration Sketch with the ECA Trigger System (#803)

The ECA graph (events/conditions/actions) already exists in crates/. The escape hatch
slots UNDER it, WC3-style:

```
GUI trigger editor (ECA blocks)                    "custom script" leaf
        |                                                  |
        v                                                  v
  ECA graph (content, #1348)                 authoring source (DSL/Rust/WAT) — content
        |                                                  |
        |  "convert GUI -> text" (#834): lower              |  compile + validate:
        |  ECA graph to authoring DSL (one-way              |   - module validation
        |  editable escape, like WC3)                      |   - import allowlist (fixed-point sim API only)
        |                                                  |   - determinism lint (no denied proposals)
        v                                                  v
        +----------------> .wasm artifact <----------------+
                     (hashed, provenance-tracked, gated, lowered —
                      never a raw side-channel into the tick)
                                    |
                                    v
                  wasmtime Engine (binding config §3)
                  pre-instantiated, pooled, fuel budget per invocation
                                    |
                     typed fixed-point host imports ONLY
                                    v
                          sim tick (deterministic)
```

- **Event dispatch:** an ECA trigger whose action list contains a `RunScript(module_id, entry)`
  action invokes the pooled instance with the trigger context marshalled through the
  fixed-point ABI. Script execution cost is fuel, charged against the invocation budget.
- **Conditions:** scripts may implement condition predicates returning bool through the
  same ABI — same budget, same determinism config.
- **Replay:** module hash + fuel budget + ABI version are part of the replay header;
  fuel consumed per invocation may be asserted in replay-divergence diagnostics (#1472).
- **Failure model:** validation failures reject at ingest (editor-time, loud); runtime
  traps (fuel exhaustion, unreachable) fail the invoking trigger loudly and
  deterministically — identical on every lockstep peer.

## 5. Disposition of Stale Issues

| Issue | Disposition |
|---|---|
| **#769** (gopher-lua build patches, Go era) | CLOSED (already done) — gopher-lua unreachable in the all-Rust engine. |
| **#803** (Trigger/ECA epic) | Retitled: "Lua escape" → "WASM escape" (already done). ECA core is implemented; the escape leaf tracks via #834. |
| **#834** (GUI<->Lua convert) | REPURPOSED (already done) as the WASM escape-hatch **implementation tracker**: wasmtime host with the §3 binding config, GUI→text lowering via an authoring DSL that compiles to `.wasm`, FSV gate = identical replay hashes across two runs + loud rejection of nondeterministic/over-fuel/denied-import modules. #1474 closes when #834's FSV lands. |

## 6. Implemented Contract

The #834 implementation uses exact workspace pins: `wasmtime = 46.0.1`,
`wasmparser = 0.251.0`, `wasm-encoder = 0.251.0`, and `wat = 1.251.0`.
Exact-commit manual FSV evidence and close state remain authoritative on #834.

- `litd-engine::wasm_script` owns ABI v1. Artifacts persist physical `.wasm` bytes,
  SHA-256, ABI version, pinned Wasmtime version, module id, entry point, and fuel
  budget. Those fields plus fixed host state and any guest-memory bytes feed replay
  identity/state hashing.
- Validation is strict MVP with floats removed. WASI and every ambient import are
  denied; only `litd.get_fixed`, `litd.set_fixed`, and `litd.add_fixed` function
  imports are accepted. Tables, start functions, shared/memory64/custom-page memory,
  mutable globals, and `memory.grow` are denied.
- ABI v1 permits at most one fixed linear memory of at most 64 KiB. It must be
  exported exactly as `memory` so every byte is replay-hashed and restored from a
  preallocated snapshot after any trap. This prevents hidden guest state from
  surviving a failed transaction.
- Runtime preparation compiles, pre-links, and instantiates through Wasmtime's pooling
  allocator. Invocation resets deterministic fuel, rolls host and guest state back on
  failure, and performs no heap allocation on the measured fixed-API or fixed-memory
  invoke paths.
- `litd-editor::trigger_wasm_authoring` lowers bounded GUI fixed-point set/add
  operations to canonical human-readable WAT and embeds canonical reversible GUI
  metadata in `litd.trigger.wasm-gui.v1`. Source, metadata, and generated artifact
  hashes are revalidated before execution and on persisted-state readback.
- Trigger state/editor schemas are v2 and carry `WasmEscape`; stale `lua_escape`
  fields fail deserialization instead of being ignored. Guarded MCP upsert materializes
  and persists the compiled artifact in the canonical Calyx Aster state beneath
  `C:\code\GameEditor\Calyx`.

## 7. Sources

- Wasmtime — Deterministic Wasm Execution: https://docs.wasmtime.dev/examples-deterministic-wasm-execution.html
  (fuel interruption "completely deterministic ... interrupted at the same location"; NaN canonicalization for "entirely deterministic WebAssembly computation")
- Wasmtime `Config` (consume_fuel, cranelift_nan_canonicalization): https://docs.wasmtime.dev/api/wasmtime/struct.Config.html
- Wasmtime — Interrupting Execution (fuel vs epochs): https://docs.wasmtime.dev/examples-interrupting-wasm.html
- WebAssembly design #1463 — motivation for NaN canonicalization: https://github.com/WebAssembly/design/issues/1463
- LuaJIT #719 — table iteration order differs across VM invocations: https://github.com/LuaJIT/LuaJIT/issues/719
- lua-l — deterministic table iteration depends on value addresses: http://lua-users.org/lists/lua-l/2013-02/msg00157.html
- Gaffer On Games — Floating Point Determinism: https://gafferongames.com/post/floating_point_determinism/
- Rhai safety/limits (max_operations, sandbox): https://rhai.rs/book/safety/sandbox.html
