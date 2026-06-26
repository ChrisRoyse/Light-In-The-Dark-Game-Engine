package sim

// #83 replay-playback FSV. The spec's core property: "playback = re-simulation",
// so the final sim state must be IDENTICAL no matter what speed or pause schedule
// the viewer used to reach the end. SoT = the world's top state hash after
// playback, compared against a reference re-sim of the same command stream.
// X+X=Y: the same recorded inputs, replayed at 0.5×/1×/2×/4×/8× or paused
// mid-way, must all land on the one reference hash.

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const playTicks = 120

func playWorld(t *testing.T) (*World, []EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	w.SetGrid(openGrid())
	ids := make([]EntityID, 0, 4)
	for i := int32(0); i < 4; i++ {
		u := orderUnit(t, w, 10+i*3, 10, 16*fixed.One)
		w.OccupyCell(u)
		ids = append(ids, u)
	}
	return w, ids
}

func playResolve(w *World, ids []EntityID) func(uint32) (EntityID, bool) {
	return func(idx uint32) (EntityID, bool) {
		if int(idx) < len(ids) && w.Ents.Alive(ids[idx]) {
			return ids[idx], true
		}
		return 0, false
	}
}

func syntheticReplay() []ReplayCommand {
	pt := func(cx, cy int32) (int64, int64) {
		c := CellCenter(cellIdx(cx, cy))
		return int64(c.X), int64(c.Y)
	}
	x1, y1 := pt(20, 10)
	x2, y2 := pt(15, 14)
	x3, y3 := pt(25, 18)
	return []ReplayCommand{
		{Tick: 1, Kind: ReplayMove, Unit: 0, Target: NoRosterRef, X: x1, Y: y1},
		{Tick: 1, Kind: ReplayMove, Unit: 1, Target: NoRosterRef, X: x2, Y: y2},
		{Tick: 5, Kind: ReplayMove, Unit: 2, Target: NoRosterRef, X: x3, Y: y3},
		{Tick: 20, Kind: ReplayMove, Unit: 0, Target: NoRosterRef, X: x2, Y: y2},
		{Tick: 50, Kind: ReplayStop, Unit: 1, Target: NoRosterRef},
	}
}

func topHash(w *World) uint64 {
	reg := NewHashRegistry()
	var snap statehash.Snapshot
	w.HashState(reg, &snap)
	return snap.Top
}

// referenceHash is the independent re-sim: apply-then-step each tick, no player.
func referenceHash(t *testing.T, cmds []ReplayCommand) uint64 {
	w, ids := playWorld(t)
	resolve := playResolve(w, ids)
	c := 0
	for tk := uint32(1); tk <= playTicks; tk++ {
		for c < len(cmds) && cmds[c].Tick == tk {
			cmds[c].Apply(w, resolve)
			c++
		}
		w.Step()
	}
	return topHash(w)
}

func playerHash(t *testing.T, cmds []ReplayCommand, drive func(*ReplayPlayer)) uint64 {
	w, ids := playWorld(t)
	rep := &Replay{Version: ReplayFormatVersion, Interval: DefaultCheckpointInterval, Ticks: playTicks, Commands: cmds}
	p := NewReplayPlayer(rep, w, playResolve(w, ids))
	drive(p)
	return topHash(w)
}

func TestReplayPlayerSpeedInvarianceFSV(t *testing.T) {
	cmds := syntheticReplay()
	ref := referenceHash(t, cmds)

	for _, sp := range []PlaybackSpeed{Speed1x, Speed2x, Speed4x, Speed8x, SpeedHalf} {
		speed := sp
		h := playerHash(t, cmds, func(p *ReplayPlayer) {
			p.SetSpeed(speed)
			p.RunToEnd()
			if !p.Done() {
				t.Fatalf("speed %d did not reach the end (tick %d/%d)", speed, p.Tick(), playTicks)
			}
		})
		if h != ref {
			t.Fatalf("speed %d final hash %016x != reference %016x — playback is not re-simulation", speed, h, ref)
		}
	}
	t.Logf("FSV #83 speed-invariance: reference=%016x reproduced bit-exact at 1×/2×/4×/8×/0.5×", ref)
}

func TestReplayPlayerPauseResumeFSV(t *testing.T) {
	cmds := syntheticReplay()
	ref := referenceHash(t, cmds)

	h := playerHash(t, cmds, func(p *ReplayPlayer) {
		p.SetSpeed(Speed4x)
		for p.Tick() < 60 && !p.Done() {
			p.Frame()
		}
		// Pause: frames advance 0 ticks, the sim is frozen.
		p.Pause()
		frozen := p.Tick()
		for i := 0; i < 10; i++ {
			if n := p.Frame(); n != 0 {
				t.Fatalf("paused frame advanced %d ticks", n)
			}
		}
		if p.Tick() != frozen {
			t.Fatalf("tick moved from %d to %d while paused", frozen, p.Tick())
		}
		// Resume to completion.
		p.Resume()
		p.RunToEnd()
		if !p.Done() {
			t.Fatal("did not finish after resume")
		}
	})
	if h != ref {
		t.Fatalf("pause/resume final hash %016x != reference %016x", h, ref)
	}
	t.Log("FSV #83 pause/resume: final hash unchanged; paused frames advanced 0 ticks, sim frozen")
}

func TestReplayPlayerSpeedBatchingFSV(t *testing.T) {
	w, ids := playWorld(t)
	rep := &Replay{Version: ReplayFormatVersion, Interval: DefaultCheckpointInterval, Ticks: 100}
	p := NewReplayPlayer(rep, w, playResolve(w, ids))

	// Integer speeds batch exactly that many ticks per frame.
	p.SetSpeed(Speed4x)
	if n := p.Frame(); n != 4 || p.Tick() != 4 {
		t.Fatalf("4× frame advanced %d ticks (tick=%d), want 4", n, p.Tick())
	}
	p.SetSpeed(Speed8x)
	if n := p.Frame(); n != 8 || p.Tick() != 12 {
		t.Fatalf("8× frame advanced %d ticks (tick=%d), want 8", n, p.Tick())
	}

	// Invalid speed is ignored (fail-closed: keep the current rate).
	p.SetSpeed(Speed2x)
	prev := p.Speed()
	p.SetSpeed(PlaybackSpeed(99))
	if p.Speed() != prev {
		t.Fatalf("invalid speed accepted: now %d, want %d", p.Speed(), prev)
	}

	// Pause halts the batch entirely.
	p.Pause()
	if n := p.Frame(); n != 0 {
		t.Fatalf("paused frame advanced %d ticks", n)
	}
	t.Log("FSV #83 batching: 4×=4 ticks, 8×=8 ticks, invalid-speed ignored, pause=0 (0.5× cadence in the next test)")
}

func TestReplayPlayerHalfSpeedCadenceFSV(t *testing.T) {
	w, ids := playWorld(t)
	rep := &Replay{Version: ReplayFormatVersion, Interval: DefaultCheckpointInterval, Ticks: 100}
	p := NewReplayPlayer(rep, w, playResolve(w, ids))
	p.SetSpeed(SpeedHalf)
	// Over 6 frames at 0.5×, expect ticks: 0,1,0,1,0,1 → total 3.
	got := make([]int, 6)
	total := 0
	for i := range got {
		got[i] = p.Frame()
		total += got[i]
	}
	if total != 3 || p.Tick() != 3 {
		t.Fatalf("0.5× over 6 frames advanced %d ticks (tick=%d, frames=%v), want 3", total, p.Tick(), got)
	}
	t.Logf("FSV #83 0.5×: 6 frames → ticks %v (one tick per two frames)", got)
}

// #83 edge 3: a replay whose encoding version byte does not match is refused at
// decode (the viewer loads through DecodeReplay), no crash — the fail-closed
// boundary the player relies on.
func TestReplayPlayerRefusesBadVersionFSV(t *testing.T) {
	rep := &Replay{Version: ReplayFormatVersion, Interval: DefaultCheckpointInterval, Ticks: 10}
	var buf bytes.Buffer
	if err := rep.Encode(&buf); err != nil {
		t.Fatal(err)
	}
	raw := buf.Bytes()
	raw[len(ReplayMagic)] ^= 0xFF // corrupt the version word
	_, err := DecodeReplay(bytes.NewReader(raw))
	if err == nil {
		t.Fatal("decode accepted a mismatched version byte")
	}
	t.Logf("FSV #83 fail-closed: mismatched version refused at load: %v", err)
}
