[CmdletBinding()]
param(
    [ValidateSet("litd-engine", "litd-editor")]
    [string]$Package = "litd-engine",
    [string]$FsvRoot,
    [string]$TargetDir,
    [switch]$Locked,
    [int]$TestTimeoutSeconds = 900
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$effectiveFsvRoot = if ($FsvRoot) { $FsvRoot } else { Join-Path $repoRoot "target\fsv\trust-critical-examples\$Package" }
$effectiveTargetDir = if ($TargetDir) { $TargetDir } else { Join-Path $effectiveFsvRoot "cargo-target" }
$examples = if ($Package -eq "litd-engine") {
    @(
        "issue1112_physics_determinism_fsv",
        "issue1115_calyx_persistence_fsv",
        "issue1110_rapier_physics_fsv",
        "ecs_fsv",
        "issue1047_skilled_rewards_fsv",
        "issue1063_infernal_deeps_fsv",
        "issue1070_infernal_attrition_fsv",
        # RPG-content FSV example: asserts the Calyx learned-lens roster count. `check-examples.ps1`
        # only `cargo check`s examples, so a runtime assertion drift (roster 13->10) went undetected
        # until manually run (#1523). Running it here fails loud on roster/measurement drift.
        "issue1061_undeath_fsv"
    )
} else {
    @(
        "issue1467_surface_parity_readback",
        "issue870_scene_composition_fsv",
        "issue872_hud_ui_builder_fsv",
        "editor_rpg_map_runtime_issue1445_fsv",
        # MCP world/object tool FSV harnesses. `check-examples.ps1` only `cargo check`s
        # examples, so a runtime failure - these fs::read a Calyx Aster VAULT DIRECTORY,
        # which on Windows aborts with `Os error 5 PermissionDenied` - went undetected on
        # main (#1529). Running them here fails loud on any such regression.
        "mcp_world_tools_fsv",
        "mcp_object_tools_fsv"
    )
}

Write-Host "FSV_EXAMPLE_GATE_START package=$Package examples=$($examples -join ',')"
if ($Package -eq "litd-engine") {
    Write-Host "FSV_EXAMPLE_GATE_MANUAL_ONLY issue1418=runtime_state issue1456=real_panel_a37 issue1460=game_host_gpu"
}
$args = @{
    Package = $Package
    Examples = $examples
    # FSV example binaries are gated behind the `fsv` feature (#1497) so routine `cargo test`
    # gates never build them; the example gate must opt back in or every --example resolves to
    # "no such target".
    Features = @("fsv")
    FsvRoot = $effectiveFsvRoot
    TargetDir = $effectiveTargetDir
    TestTimeoutSeconds = $TestTimeoutSeconds
}
if ($Locked) { $args.Locked = $true }
& (Join-Path $PSScriptRoot "test-gameeditor.ps1") @args
Write-Host "FSV_EXAMPLE_GATE_OK package=$Package examples=$($examples.Count)"
