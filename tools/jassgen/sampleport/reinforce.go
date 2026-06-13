// Package sampleport is the M2 Q2 sample-port: reinforce.j ported to idiomatic
// Go using ONLY the generated JASS->Go mapping table
// (docs/prd/03-api/jass-mapping/mapping-table.md) as the lookup source. It
// compiles against the M2 panic-bodied stubs; runtime behaviour is out of scope
// (the stubs panic until M5). Every lookup outcome — found, found-but-unclear,
// or missing/pending — is recorded in FINDINGS.md. Pending functions (no
// canonical symbol in the M2 surface yet) cannot be expressed and are left as
// PENDING comments; that gap IS the Q2 evidence.
package sampleport

import (
	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers"
	ai "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
)

// footmanTypeID is the 'hfoo' four-CC unit type. (FourCC -> type ID resolution
// is itself a PENDING lookup — see FINDINGS.md; the raw int stands in.)
const footmanTypeID = 0x68666f6f // 'hfoo'

// reinforceCondition ports ReinforceConditions.
//
//	JASS: return GetUnitCount('hfoo') < 5
//	table: GetUnitCount (common.ai, D2) -> litd/ai.UnitCount
func reinforceCondition() bool {
	return ai.UnitCount(footmanTypeID) < 5
}

// reinforceActions ports ReinforceActions.
//
// The created unit `u` and spawn position `staging` stand in for the handles
// the PENDING CreateNUnitsAtLoc / GetStartLocationLoc / GetRectCenter calls
// would return. GetLastCreatedUnit is TOMBSTONED (superseded): the table's
// guidance is that the Go creator returns the handle directly, so there is no
// bj_lastCreatedUnit side channel to read — `u` is simply that returned handle.
func reinforceActions(u litd.Unit, staging litd.Vec2) {
	// JASS: SetUnitState(u, UNIT_STATE_LIFE, 100.0)
	// table: SetUnitState (common.j, D5) -> litd/api.Unit.SetLife
	u.SetLife(100.0)

	// JASS: PauseUnitBJ(u, true)
	// table: PauseUnitBJ (blizzard.j, D2) -> litd/api.Unit.SetPaused
	u.SetPaused(true)

	// JASS: PolledWait(2.0)
	// table: PolledWait (blizzard.j, D4) -> litd/api/helpers.PolledWait
	helpers.PolledWait(2.0)

	// JASS: SetUnitPositionLoc(u, staging)  [the ...Loc variant]
	// table: SetUnitPositionLoc (common.j, D3) -> litd/api.Unit.SetPosition
	//        (D3 collapse -> SetUnitPosition; Vec2 absorbs the location handle)
	u.SetPosition(staging)

	// JASS: if (IsUnitPausedBJ(u)) then call PauseUnitBJ(u, false) endif
	// table: IsUnitPausedBJ (blizzard.j, D1) -> litd/api.Unit.Paused
	if u.Paused() {
		u.SetPaused(false)
	}

	// PENDING (M2 backlog) — no canonical symbol in the table yet:
	//   CreateNUnitsAtLoc, GetStartLocationLoc, GetRectCenter, Player,
	//   RemoveLocation. RemoveLocation in particular has no Go analogue at all:
	//   Vec2 is a value type, so there is no location handle to free.
}

// Reference the ported entry points so the package has an exercised surface
// (a real trigger registers these as condition/action callbacks in M5).
var (
	_ = reinforceCondition
	_ = reinforceActions
)
