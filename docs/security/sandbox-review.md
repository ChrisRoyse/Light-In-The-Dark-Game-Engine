# Lua Sandbox Security Review (D-20 hard gate, #319)

**Status:** IN PROGRESS — sign-off **withheld** (two vectors not yet verifiable; see §4).
**Reviewer:** agent:opus (sole agent; delegated authority does NOT extend to clearing a D-20 hard gate that greenlights the sharing features #176–#181 while attack vectors remain untestable — that final sign-off needs the save/load and cross-platform vectors closed first).
**Scope:** `litd/luabind` sandbox (`NewSandbox`/`Register`) as shipped today. R-SEC-1; D-20 (non-negotiable); milestones.md M9 hard gate.
**Source of truth:** the escape-attempt corpus in `litd/luabind/sandbox_escape_test.go` + `bindings_test.go`. Every vector below is a permanent test asserting a *loud* failure (error text quoted), run and read manually — not trusted by exit code.

## 1. Threat model

A world archive (#175–#181) carries untrusted Lua authored by a third party. It runs in-process against the live `Game`. The sandbox must guarantee a malicious world cannot: reach the host filesystem / process / network; load or emit bytecode; reflect or mutate the host environment; exhaust host CPU or memory; confuse the binding layer into a host panic; or (once save/load ships) resume tampered serialized state. All authority flows from the `Game` value bound at `Register`; there are no ambient capability globals.

## 2. Vectors swept — RESULT: BLOCKED (loud)

Each raises a located Lua error; the capability global is absent, so touching it is a runtime error, never a value. Verified by `TestSandboxEscapeBlockedVectors` (one subtest per row) unless noted.

| Vector | Attempt | Observed result |
|---|---|---|
| Filesystem read | `io.open("/etc/passwd","r")`, `io.read()`, `dofile`, `loadfile` | nil-index error (io/dofile/loadfile absent) |
| Process spawn | `os.execute("echo pwned")`, `os.exit(1)` | os.* absent → error |
| Wall-clock leak | `os.time()`, `os.clock()` | absent → error (determinism: no nondeterministic clock) |
| Env read | `os.getenv("HOME")` | absent → error |
| Code loading | `require("os")`, `package.loaded` | absent → error |
| Arbitrary chunk | `load("return os")()`, `loadstring(...)` | load/loadstring absent → error |
| **Bytecode emit** | `string.dump(function() end)` | `GopherLua does not support the string.dump` |
| **Bytecode load** | chunk beginning with the `\x1bLua` signature | `Invalid token` — compiler rejects; fork has **no undump path** (`TestSandboxBytecodeRejected`) |
| Reflection | `debug.getinfo(1)`, `debug.getupvalue(print,1)` | debug.* absent → error |
| Env reflection/swap | `getfenv(1)`, `setfenv(1,{})` | absent → error |
| Userdata escalation | `newproxy(true)` | absent → error |
| GC side channel | `collectgarbage("count")` | absent → error |
| Concurrency | `coroutine.create(...)`, `channel.make()` | absent → error (cooperative scheduler only; no child LState / goroutine) |

### Metatable escalation — BLOCKED (`TestSandboxStringMetatableLocked`)
- `getmetatable("")` returns the `"locked"` sentinel, not the mutable string library.
- `("").__index` is not the string table; `string.upper = ...` and `("").x = 1` both raise.
- Positive control: `("hello"):upper()` still returns `HELLO` — the lock is not collateral damage.

### Resource exhaustion — BLOCKED (loud, no host OOM/hang)
- **String memory bomb** (`TestSandboxMemoryBomb`): `("A"):rep(1 GiB)` → `memory budget exceeded`, refused *before* allocation; host `HeapAlloc` delta stays < 64 MiB (the GiB never materialized).
- **Table memory bomb** (`TestSandboxTableBomb`): unbounded `t[k]=k` loop → bounded loudly (instruction ceiling hit first; memory ledger intact), never an OOM.
- **CPU infinite loop** (`TestSandboxInstructionBudget`): `while true do end` → `instruction budget exceeded`.
- **Quota dodge via pcall** (`TestSandboxQuotaDodgePcall`): `while true do pcall(...) end` → `instruction budget exceeded`. pcall catches Lua errors but the budget is enforced by the VM below the error layer, so a pcall loop cannot reset or evade it.
- **Under-budget accounting** (`TestSandboxMemoryBudgetUnderLimit`): a 200-byte alloc debits exactly 200 bytes — deterministic charge.

### Binding-layer type confusion — BLOCKED (`TestBindingTypeConfusionFSV`, #267)
Generated dispatch reads handle args via `CheckUserData` + an ok-checked concrete-type assertion (`register.go` `argUnit`/`argPlayer`/…). Every confusion raises a located `ArgError`; **no Go panic path**:
- primitive where handle expected → `bad argument #N … (userdata expected, got string/nil)`;
- number where Vec2 table expected → `Vec2: want table, got number`;
- **wrong noun handle** (`Unit_Kill(Game_Player(0))`) → `expected Unit userdata, got litd.Player`.

Faithfulness control (`TestGoVsLuaIdenticalHashFSV`): the same scenario via direct Go calls and via the bindings yields a bit-identical `StateHash` (`0xda32a7d559477354`) — the Lua skin cannot diverge sim state from the Go surface.

## 3. Positive controls (lockdown is not collateral)
`TestSandboxLegitCodeWorks`: `math.floor`, `string.format`, `table.concat`, `pairs` iteration, and the deterministic `math.random` (bound source) all behave. Real worlds are not broken by the lockdown.

## 4. Vectors NOT yet verifiable — sign-off blockers

| Vector (from #319) | Why not yet testable | Blocked by |
|---|---|---|
| Malicious serialized **coroutine state** (tampered save file → load must refuse with a structured error) | Suspended-coroutine serialization is not built; `persist.go` is only the #264 foundation, the LState-graph serializer is unbuilt. There is no save format to attack yet. | **#270** |
| **Quota determinism across OS/arch** (same script, identical instruction-exhaustion point on linux/windows/macos × amd64/arm64) | No CI matrix — GitHub Actions disabled (billing). Single-platform determinism holds locally; cross-platform is unverified. | **#284** |

D-20 sign-off is **withheld** until both are closed and their escape cases added to this corpus. The sharing features they gate (#176–#181, M9) must not ship before then.

## 5. Findings
**None requiring a fix.** Every testable vector was already blocked loudly by the existing `#266`/`#262` sandbox + budget work; this review extended the corpus (bytecode emit/load, pcall quota-dodge, table bomb, binding type-confusion) and found no holes. When #270 and #284 land, re-open this review for the two §4 vectors before sign-off.
