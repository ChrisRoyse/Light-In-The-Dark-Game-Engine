# Batch workflow -- depth reference

How to run an efficient content batch (many entities per PR, one worktree per category).

## Setup

1. Pick ONE category (Race, Class, Ability, ...). Confirm its framework EXISTS by grepping
   `crates/litd-engine/src/rpg/` (see the availability map in SKILL.md). If PARTIAL/MISSING,
   build the generic framework first.
2. Create ONE git worktree for that category. Cross-category work goes in separate worktrees --
   never two agents in the same category concurrently (they collide on the shared fixture,
   `*_tests.rs`, and the `rpg.rs` hub).
3. Set the runtime env once per PowerShell session (see the [[calyx]] skill section 4):
   ```
   . .\scripts\Set-GameEditorCalyxRuntimeEnv.ps1
   $env:LITD_EDITOR_PROJECT_ROOT = "C:\code\GameEditor"
   ```

## The loop per batch

1. **Transcribe facts.** For each entity, fetch the CF page via the [[cf-source]] skill (exa
   `web_fetch_exa`). Record the factual mechanics; originalize name/lore only (#984/#1670).
2. **Author data files.** Add each `*Spec` to the category fixture (`*_test_fixture.rs`). Split
   the fixture BEFORE it reaches 500 lines (line-count ratchet gate).
3. **Emit + lower.** Each spec -> `*_content_records(&spec)` (editor:// provenance, no `rpg://`,
   no `TestPass` anchor) -> `persist_and_lower_rpg_authored_content(store, records)`.
4. **Readback assert.** Assert `store.get_content_for_editor(kind, id)` matches the authored spec
   for every entity in the batch.
5. **ONE consolidated FSV harness per batch.** Do not write a separate FSV example per entity --
   author all N specs, persist+lower all N, then one harness reads back all N and proves
   observed == expected. This keeps the loop fast while still verifying against the bytes.

## The rpg.rs hub edit (serialize it)

The global hub `crates/litd-engine/src/rpg.rs` (~296 lines) needs a `mod` + re-export for any new
file. Because it is a shared file near the 500-line ratchet:

- Make the hub edit as ONE deliberate, serialized step per PR.
- If the edit would push `rpg.rs` past the ratchet, restructure (submodule) rather than growing it.

## Gates to run before commit

- `pwsh scripts/check-calyx-side-store-eradication.ps1` -- proves no structured writes outside
  Calyx and that every content factory routes through `validate_rpg_authored_content_records`.
  Exit 0 required.
- `pwsh scripts/check-format.ps1` (includes the line-count ratchet + doc-reference gate).
- The FSV harness must actually RUN (not just `cargo check`). FSV examples need
  `harness = false` + `required-features = ["fsv"]` in Cargo.toml and must be listed in
  `scripts/test-fsv-examples.ps1`, else the run is a 0-test false-green.

## FSV evidence discipline

- Minimal-sufficient: the smallest dataset that gives 100% proof, not the biggest (#800 doctrine).
- Verify against the BYTES: read the Aster readback / lowered fingerprint, compare observed ==
  expected. A green test is supporting evidence, not proof.
- For MCP-tool artifacts, the vault paths are DIRECTORIES -- never `fs::read` them; use the
  store-computed `.sha256` (see the [[calyx]] skill section 5).
