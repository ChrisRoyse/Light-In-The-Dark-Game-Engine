// reinforce.j — sample JASS map fragment for M2 Q2 sample-port validation.
//
// PROVENANCE: self-authored fragment in the canonical Blizzard GUI-generated
// "BJ" idiom (the function family the WC3 World Editor emits when a GUI trigger
// is saved). It is NOT extracted from a published map binary: the M2 manifest
// maps only the six D1-D5 exemplar functions, so no real published map could
// compile against the M2 stub surface — see FINDINGS.md and the type:discovery
// issue filed alongside this port. The fragment is deliberately representative
// of real GUI output (BJ wrappers, location handles, polled waits, the
// bj_lastCreatedUnit side-channel idiom) so the Q2 lookup exercise is faithful.
// License: CC0 (authored for this repo).
//
// Map logic: a "reinforcement" trigger. While the player owns fewer than five
// footmen, spawn one at the start location, heal it to full, briefly pause it,
// teleport it to the staging point, then unpause. This touches the tombstone,
// D4-helper, and ...Loc-variant edge cases the Q2 validation requires.

function ReinforceConditions takes nothing returns boolean
    // 'hfoo' = human Footman. GetUnitCount is a common.ai native.
    return GetUnitCount('hfoo') < 5
endfunction

function ReinforceActions takes nothing returns nothing
    local unit u
    local location spawn = GetStartLocationLoc(0)
    local location staging = GetRectCenter(bj_mapInitialPlayableArea)

    // Spawn one footman for player 0, then grab it via the bj_lastCreatedUnit
    // side channel — the classic GUI idiom.
    call CreateNUnitsAtLoc(1, 'hfoo', Player(0), spawn, bj_UNIT_FACING)
    set u = GetLastCreatedUnit()

    // Heal to full via the unit-state accessor (D5 enum-keyed setter).
    call SetUnitState(u, UNIT_STATE_LIFE, 100.0)

    // Freeze the unit, wait, teleport via the location variant, then unfreeze.
    call PauseUnitBJ(u, true)
    call PolledWait(2.0)
    call SetUnitPositionLoc(u, staging)
    if (IsUnitPausedBJ(u)) then
        call PauseUnitBJ(u, false)
    endif

    call RemoveLocation(spawn)
    call RemoveLocation(staging)
    set u = null
    set spawn = null
    set staging = null
endfunction
