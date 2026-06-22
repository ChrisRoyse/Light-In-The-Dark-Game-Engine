# Editor FSV Runbook

This runbook verifies the M8 editor exit path: author a playable map in the
editor, save it as a source-form archive, load that archive through the shipped
game client, and inspect the resulting sim state and screenshots.

## Happy Path

Run from the repo root:

```bash
go run ./cmd/litd-editor -autotest -out artifacts/editor-fsv
```

The source of truth is written under:

- `artifacts/editor-fsv/m8-e2e/source/`
- `artifacts/editor-fsv/m8-e2e/editor-e2e.litdworld`
- `artifacts/editor-fsv/m8-e2e/editor-e2e-reopened.litdworld`
- `artifacts/editor-fsv/m8-e2e/m8-e2e-dump.json`
- `artifacts/editor-fsv/43-m8-e2e-authored.png`
- `artifacts/editor-fsv/44-m8-e2e-reopened.png`
- `artifacts/editor-fsv/45-m8-e2e-initial.png`
- `artifacts/editor-fsv/46-m8-e2e-played.png`
- `artifacts/editor-fsv/47-m8-e2e-replay.png`

Inspect `m8-e2e-dump.json`, not just the command exit code. Expected invariants:

- `sourceHashes` includes `world.toml`, `map/height.txt`, `map/cliff.txt`,
  `map/splat.txt`, `map/entities.toml`, `map/doodads.toml`,
  `data/combat/damage-table.toml`, `data/units/editor.toml`,
  `data/placement/editor.toml`, and `scripts/main.lua`.
- `archiveHashesEqual` is `true` for save -> open archive in editor -> save.
- `initial.state.unitCount` and `initial.state.alive` are both `5`.
- `secondIndependentLoad.state.stateHash` matches `initial.state.stateHash`.
- `played.state.order.issued` is `true`, and the ordered unit moves closer to
  the printed target coordinate.
- `combatResolved` is `true`.
- `replay.state.stateHash` matches `played.state.stateHash`.
- Every screenshot record has a nonzero size and dimensions matching the dump.

## Direct Game Reproduction

After the editor run, independently re-run the production game client:

```bash
go run ./cmd/litd \
  -archive artifacts/editor-fsv/m8-e2e/editor-e2e.litdworld \
  -autotest \
  -ticks 0 \
  -shot artifacts/editor-fsv/manual-initial.png

go run ./cmd/litd \
  -archive artifacts/editor-fsv/m8-e2e/editor-e2e.litdworld \
  -autotest \
  -autotest-order \
  -autotest-order-dx 8192 \
  -autotest-order-dy 0 \
  -ticks 700 \
  -shot artifacts/editor-fsv/manual-played.png
```

Read the `state:` JSON lines. The initial run should print five live units. The
played run should print an issued order, a different `stateHash`, moved
coordinates for the ordered unit, and changed combat life/alive state.

## Edge Cases

1. Reopen edge: the autotest opens `editor-e2e.litdworld` back into the editor,
   saves `editor-e2e-reopened.litdworld`, and requires identical member hashes.
2. Replay edge: the autotest runs the same played command twice and requires the
   same final `stateHash`.
3. Independent load edge: the autotest starts a second local production-loader
   process and requires the same initial `stateHash`.

For the second-machine/OS edge from #138, copy `editor-e2e.litdworld` to the
other machine, run the first direct game command there, and compare the printed
`stateHash` plus screenshot dimensions/hash against `m8-e2e-dump.json`.
