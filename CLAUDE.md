# Light in the Dark Game Engine

Warcraft III–inspired RTS engine in pure Go on the G3N OpenGL engine. The public API is a deduplicated Go port of the WC3 JASS API. Long-term purpose: a world-building and idea-explanation platform (PRD §1.0), authorable by humans and AI coding agents.

## Sources of truth

| What | Where | Notes |
|---|---|---|
| Product requirements | `docs/PRD.md` | Master PRD. Everything else elaborates it, never contradicts it. |
| Expanded specs | `docs/prd/` | 46 docs; index at `docs/prd/README.md`. Requirement IDs (R-SIM-*, R-RND-*, R-GC-*, R-EXEC-*, R-FSV-*, R-API-*) defined in master PRD. |
| WC3 API surface | `repoes/war3-types/scripts/common.j` (1,536 natives), `scripts/blizzard.j` (985 BJ functions), `core/*.d.ts` (typed) | The API basis. `components.json`/`blizzard.json` do not exist — `api-manifest.json` is *generated* from these (see `docs/prd/09-roadmap/tooling.md`). |
| JASS runtime semantics | https://jass.sourceforge.net/doc/library.shtml | Cooperative coroutine threading, trigger model, AI command stacks. Summarized in PRD §4.4. |
| Rendering engine | `repoes/engine` (vendored g3n fork) | **Used directly via `go.mod` replace directive.** Local patches marked `// LITD-PATCH`. Do NOT bump to upstream module without re-applying patches. |
| Verification protocol | `prompts/fsv.md` | Full State Verification — mandatory for task acceptance (PRD §5.5). |

`repoes/` is gitignored (vendored clones, not tracked). Fresh checkout must restore it — see Setup.

## Setup (fresh clone)

```bash
git clone https://github.com/cipherxof/war3-types repoes/war3-types
git clone https://github.com/g3n/engine repoes/engine
# re-apply LITD-PATCH commits to repoes/engine (grep -rn "LITD-PATCH" for current set)
git clone https://github.com/yuin/gopher-lua repoes/gopher-lua
git -C repoes/gopher-lua checkout 75f497656b1c6864139dd2a7d88cf96d09550814  # then re-apply LITD patches — see docs/lua-patch-log.md
sudo apt-get install -y xorg-dev libgl1-mesa-dev libopenal-dev libvorbis-dev
```

Requires Go 1.26+, gcc (cgo), OpenGL driver. `go.mod` replaces `github.com/g3n/engine` with `./repoes/engine` and `github.com/yuin/gopher-lua` with `./repoes/gopher-lua` (the deterministic Lua fork, D-25; provenance in `docs/lua-patch-log.md`).

## Build

```bash
go build ./...                              # everything
go build -o bin/firstlight ./cmd/firstlight # M0.5 demo
```

## Run

```bash
./bin/firstlight                # interactive: left-click select, right-click move, F12 screenshot
./bin/firstlight -autotest      # scripted FSV run: orders unit to known target, prints state JSON,
                                # saves screenshot, exit 0 = pass / 2 = timeout / 3 = wrong position
```

## Test

```bash
go vet ./...
go test ./...
go test -short ./...    # inner-loop: heavy 10k/5k-tick e2e + determinism + stress
                        # tests self-skip via testing.Short() (~2x faster). The
                        # FULL preflight gate always runs them; --fast runs the
                        # 10k determinism fixtures once as explicit steps.
./bin/firstlight -autotest -shot artifacts/firstlight-autotest.png
```

New long-running tests (multi-second e2e, save/load, 10k-tick, stress) MUST guard
with `if testing.Short() { t.Skip(...) }` so the inner loop stays fast — and if it
is a determinism fixture, add it to the explicit determinism step in
`scripts/preflight.sh` so the gate still runs it.

## Pre-merge gate (no CI — runs locally)

There is no GitHub Actions / hosted CI, **permanently** — the operator decision
(2026-06-21, closing #284/#317) is to NEVER use GitHub Actions and gate locally
via `.githooks` only. All workflows were removed (2026-06-19); do not add a
`.github/workflows/` directory. Every gate the old workflows ran is consolidated
into one script — run it green before merging any branch to main:

```bash
scripts/preflight.sh            # FULL gate (vet, build, test, assetcheck, zero-alloc,
                                # importcheck, determlint, apilint, license-scan, jassgen
                                # audit, 10k determinism + -race, benchharness, world-archive)
scripts/preflight.sh --fast     # quick inner-loop subset (skips -race, bench, world-archive)
```

Enforced automatically by a tracked `pre-push` hook (`.githooks/pre-push`, wired via
`git config core.hooksPath .githooks`): pushing to `main` runs the FULL gate and is
rejected on any failure; branch pushes run the FAST subset. Bypass in emergencies only
with `git push --no-verify`. Fresh clones must run `git config core.hooksPath .githooks`
once to arm the hook.

Verification of any visual/behavioral change follows `prompts/fsv.md`: run the thing, then independently inspect the source of truth (screenshot file, state JSON, event log) — never trust exit codes alone. The demo's `-autotest` exists specifically so an agent can capture evidence: read the PNG to confirm what rendered, parse the `state:` JSON line to confirm sim coordinates.

## Known environment issues (WSL2/WSLg)

- **`glfwCreateWindow` hangs forever (no error):** stale/duplicate Xwayland listener on `/tmp/.X11-unix/X0` (check `ss -xl | grep X11` — two LISTEN entries = bug). Fix: `wsl --shutdown` from Windows, reopen. See https://github.com/microsoft/wslg/issues/1291
- **`MESA: error: Failed to attach to x11 shm`:** same root cause as above.
- **EGL fallback:** `G3N_EGL=1` env makes the vendored g3n request an EGL context instead of GLX (LITD-PATCH in `repoes/engine/window/glfw.go`) — workaround for broken GLX context creation under WSLg per https://github.com/glfw/glfw/issues/2284
- `GALLIUM_DRIVER=d3d12` selects the hardware-accelerated WSLg mesa driver if mesa picks wrong.

## Architecture rules (PRD §4.1)

- `litd/sim` (deterministic core) never imports `litd/render`; render reads sim state, never mutates.
- Presentation triggers are a **non-hashing trigger class** (#449/#471). `litd/render`/`litd/audio` react to gameplay by draining the render-event snapshot (`Snapshot.Events` / `World.EmitRenderEvent`), never via the sim-hashing subscription path (`Game.OnEvent` & its sugar, `World.Subscribe`, `NewTrigger`, `OnDamage`). The sim subscription tables serialize *and* hash (R-SIM-6), so an audio-on game must hash identical to an audio-off one. Enforced by `tools/presentlint` in preflight; `OnAudio`/`OnCamera` are the allowed presentation sinks (set a Game field, never touch the sim).
- Zero heap allocations per sim tick / render frame at steady state (R-GC-1, gated by `scripts/preflight.sh` via `testing.AllocsPerRun`).
- No `map` iteration in gameplay code; all gameplay randomness via the sim's seeded PRNG (R-SIM-2).
- Script logic runs on the deterministic cooperative scheduler, never free goroutines (R-EXEC-1).
- Public API: methods on nouns, value-type math (`Vec2`), options structs, no G3N types in signatures (R-API-1..6).
- Dedup policy D1–D5 (PRD §4.2): every JASS function maps to exactly one canonical Go symbol or is tombstoned with reason.

## Conventions

- Commits: conventional commits, normal prose.
- Assets: core glTF 2.0 GLB only, no KHR extensions except `KHR_materials_unlit` (R-FMT-1); CC0 licensed only.
- Asset binaries (`assets/**/*.glb`) are **gitignored** — only `assets/MANIFEST` (provenance ledger: path/pack/source/license/sha256) is tracked. Fresh checkout: re-download packs from the MANIFEST `source` URLs, verify with `go run ./tools/assetcheck ./assets`.
- `cmd/dbg/` is scratch space for environment debugging — delete freely.
