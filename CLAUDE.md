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
sudo apt-get install -y xorg-dev libgl1-mesa-dev libopenal-dev libvorbis-dev
```

Requires Go 1.26+, gcc (cgo), OpenGL driver. `go.mod` replaces `github.com/g3n/engine` with `./repoes/engine`.

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
./bin/firstlight -autotest -shot artifacts/firstlight-autotest.png
```

Verification of any visual/behavioral change follows `prompts/fsv.md`: run the thing, then independently inspect the source of truth (screenshot file, state JSON, event log) — never trust exit codes alone. The demo's `-autotest` exists specifically so an agent can capture evidence: read the PNG to confirm what rendered, parse the `state:` JSON line to confirm sim coordinates.

## Known environment issues (WSL2/WSLg)

- **`glfwCreateWindow` hangs forever (no error):** stale/duplicate Xwayland listener on `/tmp/.X11-unix/X0` (check `ss -xl | grep X11` — two LISTEN entries = bug). Fix: `wsl --shutdown` from Windows, reopen. See https://github.com/microsoft/wslg/issues/1291
- **`MESA: error: Failed to attach to x11 shm`:** same root cause as above.
- **EGL fallback:** `G3N_EGL=1` env makes the vendored g3n request an EGL context instead of GLX (LITD-PATCH in `repoes/engine/window/glfw.go`) — workaround for broken GLX context creation under WSLg per https://github.com/glfw/glfw/issues/2284
- `GALLIUM_DRIVER=d3d12` selects the hardware-accelerated WSLg mesa driver if mesa picks wrong.

## Architecture rules (PRD §4.1)

- `litd/sim` (deterministic core) never imports `litd/render`; render reads sim state, never mutates.
- Zero heap allocations per sim tick / render frame at steady state (R-GC-1, CI-enforced via `testing.AllocsPerRun`).
- No `map` iteration in gameplay code; all gameplay randomness via the sim's seeded PRNG (R-SIM-2).
- Script logic runs on the deterministic cooperative scheduler, never free goroutines (R-EXEC-1).
- Public API: methods on nouns, value-type math (`Vec2`), options structs, no G3N types in signatures (R-API-1..6).
- Dedup policy D1–D5 (PRD §4.2): every JASS function maps to exactly one canonical Go symbol or is tombstoned with reason.

## Conventions

- Commits: conventional commits, normal prose.
- Assets: core glTF 2.0 GLB only, no KHR extensions except `KHR_materials_unlit` (R-FMT-1); CC0 licensed only.
- Asset binaries (`assets/**/*.glb`) are **gitignored** — only `assets/MANIFEST` (provenance ledger: path/pack/source/license/sha256) is tracked. Fresh checkout: re-download packs from the MANIFEST `source` URLs, verify with `go run ./tools/assetcheck ./assets`.
- `cmd/dbg/` is scratch space for environment debugging — delete freely.
